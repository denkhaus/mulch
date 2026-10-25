//! `mulch delete` — remove records (single id, `--records`,
//! `--all-except`).
//!
//! Rewrite model (reference): the surviving records are re-serialized
//! compactly by the format core (`write_records`), which also assigns
//! ids to id-less survivors. Malformed lines and unregistered types are
//! hard errors — nothing is written (reference `readExpertiseFile`).

use mulch::resolve_record_id;

use crate::cli::GlobalOpts;
use crate::output::{Failure, print_json, print_line, success_envelope};

/// Which records the invocation targets (reference mode exclusivity).
pub(super) enum Mode {
    /// One positional id (or bare hash / unique prefix).
    Single(String),
    /// `--records` list.
    Records(Vec<String>),
    /// `--all-except` list (ids to keep; each must resolve).
    AllExcept(Vec<String>),
}

impl Mode {
    /// Parses the mutually exclusive id sources, rejecting the invalid
    /// shapes at construction (plain/json texts differ in casing —
    /// reference `delete.ts`).
    pub(super) fn from_args(
        json: bool,
        id: Option<&str>,
        records: Option<&str>,
        all_except: Option<&str>,
    ) -> Result<Self, Failure> {
        let list = |raw: &str| -> Vec<String> {
            raw.split(',')
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect()
        };
        // Plain carries the `Error: ` prefix and lowercase wording;
        // json capitalizes and drops the prefix (reference delete.ts).
        let render = |text: &str| -> String {
            if json {
                let mut chars = text.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            } else {
                format!("Error: {text}")
            }
        };
        match (id, records, all_except) {
            (Some(id), None, None) => Ok(Mode::Single(id.to_string())),
            (None, Some(raw), None) => {
                let ids = list(raw);
                if ids.is_empty() {
                    return Err(Failure::handled_on_stderr(
                        "delete",
                        render("--records requires at least one ID."),
                    ));
                }
                Ok(Mode::Records(ids))
            }
            (None, None, Some(raw)) => {
                let keep = list(raw);
                if keep.is_empty() {
                    return Err(Failure::handled_on_stderr(
                        "delete",
                        render("--all-except requires at least one ID to keep."),
                    ));
                }
                Ok(Mode::AllExcept(keep))
            }
            (None, None, None) => Err(Failure::handled_on_stderr(
                "delete",
                render("must provide a record ID, --records, or --all-except."),
            )),
            _ => Err(Failure::handled_on_stderr(
                "delete",
                render(
                    "cannot combine a record ID with --records or --all-except. Use only one mode.",
                ),
            )),
        }
    }
}

/// Runs `delete`. The domain check precedes the mode validation
/// (reference `delete.ts` arg order — the mulch-e645 pin: `delete x`
/// with no id reports the unknown domain, not the missing mode).
pub(super) fn run(
    opts: &GlobalOpts,
    id: Option<&str>,
    records: Option<&str>,
    all_except: Option<&str>,
    domain: &str,
    dry_run: bool,
) -> Result<(), Failure> {
    // Real runs hold the writer lock across the cycle; --dry-run
    // reads lock-free like the reference (mulch-228d).
    let store = if dry_run {
        crate::commands::store::open_store("delete", true)?
    } else {
        crate::commands::store::open_store_mut("delete", true)?
    };

    crate::commands::store::ensure_domain(
        "delete",
        &store,
        domain,
        opts.json,
        crate::commands::failure::DomainFailure::Standard,
    )?;

    let mode = Mode::from_args(opts.json, id, records, all_except)?;

    // Resolve every target (missing/ambiguous ids abort before any
    // write); the single-id mode rides the locator seam's read half.
    let (lines, mut removal_indices): (Vec<_>, Vec<usize>) = match &mode {
        Mode::Single(id) => {
            let (lines, position) = crate::commands::store::read_and_resolve(
                "delete",
                &store,
                domain,
                id,
                opts.allow_unknown_types,
            )?;
            (lines, vec![position])
        }
        Mode::Records(ids) => {
            let lines = read_lines(&store, domain, opts.allow_unknown_types)?;
            let mut removal_indices = Vec::new();
            for id in ids {
                removal_indices.push(resolve(&lines, id)?);
            }
            (lines, removal_indices)
        }
        Mode::AllExcept(keep) => {
            let lines = read_lines(&store, domain, opts.allow_unknown_types)?;
            let mut keep_indices: Vec<usize> = Vec::new();
            for id in keep {
                keep_indices.push(resolve(&lines, id)?);
            }
            let mut removal_indices = Vec::new();
            for index in 0..lines.len() {
                if !keep_indices.contains(&index) {
                    removal_indices.push(index);
                }
            }
            (lines, removal_indices)
        }
    };
    // A Set of indices: duplicate ids never double-delete (reference).
    removal_indices.sort_unstable();
    removal_indices.dedup();

    let bulk = !matches!(mode, Mode::Single(_)) || dry_run;
    // RAW summaries (mulch-f06a): the plain lines render
    // `String(value)` (`undefined` for absent), the json entries OMIT
    // the key or pass non-strings through verbatim.
    let deleted: Vec<(String, String, Option<serde_json::Value>)> = removal_indices
        .iter()
        .map(|index| {
            let line = &lines[*index];
            let summary =
                crate::commands::failure::summary_raw("delete", store.registry(), &line.record);
            (
                line.id().unwrap_or_default().to_string(),
                line.record_type(),
                summary.ok().flatten(),
            )
        })
        .collect();
    let kept = lines.len() - removal_indices.len();

    if !dry_run {
        let survivors: Vec<serde_json::Value> = lines
            .iter()
            .enumerate()
            .filter(|(index, _)| !removal_indices.contains(index))
            .map(|(_, line)| line.record.clone())
            .collect();
        crate::commands::store::persist("delete", &store, domain, &survivors)?;
    }

    // The reference prints each deleted record's summary through the
    // type registry; an unregistered type throws AFTER the rewrite
    // (only reachable under --allow-unknown-types).
    if opts.allow_unknown_types {
        for (_, kind, _) in &deleted {
            if !store.registry().is_known(kind) {
                return Err(Failure::error_line(
                    "delete",
                    format!("Unknown record type: {kind}"),
                ));
            }
        }
    }

    if opts.json {
        if bulk {
            let list: Vec<serde_json::Value> = deleted
                .iter()
                .map(|(id, kind, summary)| {
                    // absent OMITS the summary key (JSON.stringify
                    // drops undefined)
                    let mut entry = serde_json::Map::new();
                    entry.insert("id".into(), serde_json::Value::String(id.clone()));
                    entry.insert("type".into(), serde_json::Value::String(kind.clone()));
                    if let Some(summary) = summary {
                        entry.insert("summary".into(), summary.clone());
                    }
                    serde_json::Value::Object(entry)
                })
                .collect();
            let fields = serde_json::json!({
                "domain": domain,
                "dryRun": dry_run,
                "deleted": list,
                "kept": kept,
            });
            print_json(&success_envelope("delete", object(fields)), false);
        } else {
            let (id, kind, summary) = deleted.first().cloned().unwrap_or_default();
            let mut fields = serde_json::Map::new();
            fields.insert(
                "domain".into(),
                serde_json::Value::String(domain.to_string()),
            );
            fields.insert("id".into(), serde_json::Value::String(id));
            fields.insert("type".into(), serde_json::Value::String(kind));
            if let Some(summary) = summary {
                fields.insert("summary".into(), summary);
            }
            print_json(&success_envelope("delete", fields), false);
        }
    } else {
        let mut text = String::new();
        for (id, kind, summary) in &deleted {
            let summary = crate::commands::failure::summary_text(summary.as_ref());
            let verb = if dry_run {
                "[DRY RUN] Would delete"
            } else if bulk {
                "Deleted"
            } else {
                "✓ Deleted"
            };
            let _ = std::fmt::Write::write_fmt(
                &mut text,
                format_args!("{verb} {kind} {id} from {domain}: {summary}\n"),
            );
        }
        if !bulk && !dry_run {
            // single-id form already carries the ✓ on its line
        }
        if bulk && !dry_run && deleted.len() > 1 {
            let _ = std::fmt::Write::write_fmt(
                &mut text,
                format_args!("✓ Deleted {} records from {domain}\n", deleted.len()),
            );
        }
        // Zero deletions print nothing at all (reference).
        let text = text.trim_end_matches('\n');
        if !text.is_empty() {
            print_line(opts.quiet, text);
        }
    }
    Ok(())
}

/// Resolves one identifier against the domain's records.
fn resolve(lines: &[mulch::LineRecord], id: &str) -> Result<usize, Failure> {
    resolve_record_id(lines, id)
        .map_err(|error| crate::commands::failure::resolve_failure("delete", error))
}

/// The strict read of the bulk modes (the seam's read half resolves a
/// single id).
fn read_lines(
    store: &mulch::StoreFiles,
    domain: &str,
    allow_unknown: bool,
) -> Result<Vec<mulch::LineRecord>, Failure> {
    crate::commands::store::read_domain_records("delete", store, domain, allow_unknown)
}

/// A `serde_json::Value::Object` from a `json!` macro result.
fn object(value: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    match value {
        serde_json::Value::Object(map) => map,
        _ => serde_json::Map::new(),
    }
}
