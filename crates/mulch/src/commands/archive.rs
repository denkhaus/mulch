//! `mulch archive` — soft-archive live records by id or `--records`
//! list (reference archive.ts, mulch-7c55).
//!
//! Law map (probe-pinned 2026-10-24, ml 0.10.7): config read first
//! (the read-modify-write no-store wording), then the domain check
//! (two-line plain / capital-D one-line json, `(none)` backfill),
//! then `--reason` trim, then mode exclusivity, then the `--records`
//! list parse. Every id resolves before any write; the survivors
//! rewrite canonically, the archived set is stamped `manual:
//! <reason>` and appended to the archive. Output walks FILE order
//! (the index set), never `--records` order.

use std::fmt::Write as _;

use mulch::{LineRecord, resolve_record_id};

use crate::cli::GlobalOpts;
use crate::commands::failure::{DomainFailure, now_iso, resolve_failure, unknown_domain_failure};
use crate::commands::store::{open_store, open_store_mut, read_and_resolve, read_domain_records};
use crate::output::{Failure, print_json, print_line, success_envelope};

/// Runs `archive`. Real runs hold the writer lock across the cycle;
/// `--dry-run` reads lock-free and never writes (mulch-228d).
pub(super) fn run(
    opts: &GlobalOpts,
    domain: &str,
    id: Option<&str>,
    records: Option<&str>,
    reason: &str,
    dry_run: bool,
) -> Result<(), Failure> {
    let store = if dry_run {
        open_store("archive", true)?
    } else {
        open_store_mut("archive", true)?
    };

    let domains = store.domains();
    if !domains.iter().any(|d| d == domain) {
        return Err(unknown_domain_failure(
            "archive",
            domain,
            &domains,
            opts.json,
            DomainFailure::Standard,
        ));
    }

    // --reason is trimmed; whitespace-only is empty (plain and json
    // differ only in the shared Error:-prefix law).
    let reason_text = reason.trim();
    if reason_text.is_empty() {
        return Err(prologue_failure(opts.json, "--reason must not be empty."));
    }

    // Mode exclusivity: exactly one of positional id / --records, and
    // the list form drops empty items (commas/whitespace only is
    // "no IDs").
    let raw_ids: Vec<&str> = match (id, records) {
        (Some(single), None) => vec![single],
        (None, Some(raw)) => raw
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .collect(),
        (None, None) => {
            return Err(prologue_failure(
                opts.json,
                "must provide a record ID or --records.",
            ));
        }
        (Some(_), Some(_)) => {
            return Err(prologue_failure(
                opts.json,
                "cannot combine a record ID with --records. Use only one mode.",
            ));
        }
    };
    if raw_ids.is_empty() {
        return Err(prologue_failure(
            opts.json,
            "--records requires at least one ID.",
        ));
    }

    let stamped_reason = format!("manual: {reason_text}");

    // Resolve every id against the live file (missing/ambiguous ids
    // abort before any write); the single-id mode rides the locator
    // seam's read half.
    let (lines, mut indices): (Vec<LineRecord>, Vec<usize>) = if let Some(single) = id {
        let (lines, position) =
            read_and_resolve("archive", &store, domain, single, opts.allow_unknown_types)?;
        (lines, vec![position])
    } else {
        let lines = read_domain_records("archive", &store, domain, opts.allow_unknown_types)?;
        let mut indices = Vec::new();
        for raw_id in &raw_ids {
            let index = resolve_record_id(&lines, raw_id)
                .map_err(|error| resolve_failure("archive", error))?;
            indices.push(index);
        }
        (lines, indices)
    };
    // A Set of indices: duplicate ids never double-archive, and the
    // output walks the file, not the id list (reference).
    indices.sort_unstable();
    indices.dedup();

    let archived: Vec<&LineRecord> = indices.iter().map(|index| &lines[*index]).collect();
    let kept = lines.len() - indices.len();

    if !dry_run {
        let survivors: Vec<serde_json::Value> = lines
            .iter()
            .enumerate()
            .filter(|(index, _)| !indices.contains(index))
            .map(|(_, line)| line.record.clone())
            .collect();
        let archived_values: Vec<serde_json::Value> =
            archived.iter().map(|line| line.record.clone()).collect();
        // The two halves carry different parity surfaces (the live
        // write's handled shape vs the archive write's Error:-prefixed
        // stderr shape), so the transaction error maps per half.
        store
            .move_to_archive(
                domain,
                &survivors,
                &archived_values,
                &now_iso(),
                &stamped_reason,
            )
            .map_err(|source| match source {
                mulch::TransactionError::Live(inner) => {
                    Failure::handled("archive", crate::output::chain_message(&inner))
                }
                mulch::TransactionError::Archive(inner) => {
                    Failure::error_line("archive", crate::output::chain_message(&inner))
                }
            })?;
    }

    // The reference renders each archived record's summary through the
    // type registry; an unregistered type throws AFTER the writes
    // (only reachable under --allow-unknown-types).
    if opts.allow_unknown_types {
        for line in &archived {
            if !store.registry().is_known(&line.record_type()) {
                return Err(Failure::error_line(
                    "archive",
                    format!("Unknown record type: {}", line.record_type()),
                ));
            }
        }
    }

    if opts.json {
        let list: Vec<serde_json::Value> = archived
            .iter()
            .map(|line| {
                // RAW summary (mulch-f06a): absent OMITS the key,
                // non-strings pass through verbatim.
                let mut entry = serde_json::Map::new();
                entry.insert(
                    "id".into(),
                    line.id().map_or(serde_json::Value::Null, |id| {
                        serde_json::Value::String(id.to_string())
                    }),
                );
                entry.insert("type".into(), serde_json::Value::String(line.record_type()));
                if let Ok(Some(summary)) =
                    mulch::record_summary_value(store.registry(), &line.record)
                {
                    entry.insert("summary".into(), summary);
                }
                entry.insert(
                    "reason".into(),
                    serde_json::Value::String(stamped_reason.clone()),
                );
                serde_json::Value::Object(entry)
            })
            .collect();
        let fields = serde_json::json!({
            "domain": domain,
            "dryRun": dry_run,
            "archived": list,
            "kept": kept,
        });
        print_json(&success_envelope("archive", object(fields)), false);
    } else {
        let mut text = String::new();
        for line in &archived {
            let verb = if dry_run {
                "[DRY RUN] Would archive"
            } else {
                "Archived"
            };
            // JS truthiness: an empty id renders no segment (and is
            // unreachable — resolution needs a non-empty id).
            let rid = line
                .id()
                .filter(|id| !id.is_empty())
                .map_or_else(String::new, |id| format!(" {id}"));
            let _ = writeln!(
                text,
                "{verb} {}{rid} from {domain} ({stamped_reason}): {}",
                line.record_type(),
                crate::commands::failure::summary_text(
                    mulch::record_summary_value(store.registry(), &line.record)
                        .ok()
                        .flatten()
                        .as_ref(),
                ),
            );
        }
        if !dry_run && archived.len() > 1 {
            let _ = writeln!(text, "✓ Archived {} records from {domain}", archived.len());
        }
        let text = text.trim_end_matches('\n');
        if !text.is_empty() {
            print_line(opts.quiet, text);
        }
    }
    Ok(())
}

/// The prologue validation failure (reference archive.ts): plain
/// carries the `Error: ` prefix and lowercase wording; json
/// capitalizes and drops the prefix.
fn prologue_failure(json: bool, text: &str) -> Failure {
    if json {
        let mut chars = text.chars();
        let capitalized = match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        };
        Failure::handled_on_stderr("archive", capitalized)
    } else {
        Failure::error_line("archive", text)
    }
}

/// A `serde_json::Value::Object` from a `json!` macro result.
fn object(value: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    match value {
        serde_json::Value::Object(map) => map,
        _ => serde_json::Map::new(),
    }
}
