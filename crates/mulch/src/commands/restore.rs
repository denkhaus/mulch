//! `mulch restore` — revive a soft-archived record (reference
//! restore.ts, mulch-7c55).
//!
//! Law map (probe-pinned 2026-10-24, ml 0.10.7): the archive of every
//! configured domain is scanned in config order, one resolveRecordId
//! per domain (an ambiguous prefix inside ONE domain contributes no
//! match — the resolve error is swallowed). Zero matches is the
//! not-found hint (our copy promotes `mulch search`, the shipped-
//! strings law); matches in more than one domain are the ambiguous
//! error; the winner must carry an id. A live record with the same id
//! refuses before the archive is touched; then the archive line is
//! spliced out, the lifecycle fields stripped (order-preserving), and
//! the record appended to the live file through the canonical
//! rewrite. A lost race re-archives the record with its original
//! reason (unreachable under the store writer lock — the pre-check
//! and the re-check read the same file).

use mulch::{LineRecord, resolve_record_id, value_text};

use crate::cli::GlobalOpts;
use crate::commands::failure::render_core_error;
use crate::commands::store::{open_store_mut, read_domain_records};
use crate::output::{Failure, print_json, print_line, success_envelope};

/// Runs `restore` (mutating: the store writer lock spans the whole
/// cycle, subsuming the reference's per-file locks).
pub(super) fn run(opts: &GlobalOpts, id: &str) -> Result<(), Failure> {
    let store = open_store_mut("restore", true)?;

    // Scan every domain's archive; a resolve error is not a match.
    let mut matches: Vec<(String, LineRecord)> = Vec::new();
    for domain in store.domains() {
        let mut records = store
            .read_archive(&domain)
            .map_err(|source| Failure::handled_on_stderr("restore", render_core_error(&source)))?;
        if let Ok(position) = resolve_record_id(&records, id) {
            let line = records.swap_remove(position);
            matches.push((domain, line));
        }
    }

    let (domain, removed) = match matches.len() {
        0 => {
            return Err(Failure::handled_on_stderr(
                "restore",
                format!(
                    "Error: Archived record \"{id}\" not found in any domain. Run \
                     `mulch search --archived <query>` to browse archives."
                ),
            ));
        }
        1 => matches.swap_remove(0),
        _ => {
            let where_list = matches
                .iter()
                .map(|(domain, line)| format!("{domain} ({})", line.id().unwrap_or("undefined")))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(Failure::handled_on_stderr(
                "restore",
                format!(
                    "Error: Identifier \"{id}\" matches archived records in multiple domains: \
                     {where_list}. Use a longer prefix."
                ),
            ));
        }
    };

    // JS truthiness gates the id (an empty string reads "no id" —
    // unreachable, resolution needs a non-empty id).
    let Some(record_id) = removed.id().filter(|id| !id.is_empty()) else {
        return Err(Failure::handled_on_stderr(
            "restore",
            "Error: Archived record has no id; cannot restore safely.",
        ));
    };
    let record_id = record_id.to_string();

    // Pre-check: refuse to restore over an existing live id BEFORE
    // touching the archive.
    let existing = read_domain_records("restore", &store, &domain, opts.allow_unknown_types)?;
    if let Some(conflict) = existing
        .iter()
        .find(|line| line.id() == Some(record_id.as_str()))
    {
        return Err(Failure::handled_on_stderr(
            "restore",
            format!(
                "Error: Live record \"{record_id}\" already exists in domain \"{domain}\" \
                 (classification: {}). Delete or rename the live record before restoring, or run \
                 `mulch search --archived {record_id}` to inspect the archived copy.",
                field_text(conflict.record.get("classification")),
            ),
        ));
    }

    // removeFromArchive: a FRESH archive read, splice by exact id,
    // rewrite. A vanished record (concurrent restore) is an error.
    let archive = store
        .read_archive(&domain)
        .map_err(|source| Failure::handled_on_stderr("restore", render_core_error(&source)))?;
    let Some(position) = archive
        .iter()
        .position(|line| line.id() == Some(record_id.as_str()))
    else {
        return Err(Failure::handled_on_stderr(
            "restore",
            format!(
                "Error: Archived record \"{record_id}\" disappeared between read and write \
                 (concurrent restore?)."
            ),
        ));
    };
    let remaining: Vec<serde_json::Value> = archive
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != position)
        .map(|(_, line)| line.record.clone())
        .collect();

    // The original reason surfaces in the output and survives a
    // rollback re-archive; archives written before the reason stamp
    // existed read "manual". `??` falls through on null AND absent.
    let original_reason = removed
        .record
        .get("archive_reason")
        .filter(|value| !value.is_null())
        .cloned()
        .unwrap_or(serde_json::Value::String("manual".into()));

    let live = strip_archive_fields(&removed.record);

    // The reference's race-lost rollback (re-archive with the
    // original reason) has no equivalent here: the store writer lock
    // spans the pre-check and the live write, so a duplicate cannot
    // appear in between, and the transaction seam reports I/O
    // failures as errors without a conflict signal.

    let mut restored: Vec<serde_json::Value> =
        existing.iter().map(|line| line.record.clone()).collect();
    restored.push(live.clone());
    // Revive owns the archive-first order (the reference's restore
    // sequence); the two halves carry different parity surfaces, so
    // the transaction error maps per half.
    store
        .revive(&domain, &remaining, &restored)
        .map_err(|source| match source {
            mulch::TransactionError::Archive(inner) => Failure::handled_on_stderr(
                "restore",
                format!("Error: {}", crate::output::chain_message(&inner)),
            ),
            mulch::TransactionError::Live(inner) => {
                Failure::handled("restore", crate::output::chain_message(&inner))
            }
        })?;

    if opts.json {
        // RAW summary (mulch-f06a): absent OMITS the key, non-strings
        // verbatim.
        let mut fields = serde_json::Map::new();
        fields.insert("domain".into(), serde_json::Value::String(domain.clone()));
        fields.insert(
            "id".into(),
            live.get("id").cloned().unwrap_or(serde_json::Value::Null),
        );
        fields.insert(
            "type".into(),
            serde_json::Value::String(removed.record_type()),
        );
        if let Ok(Some(summary)) = mulch::record_summary_value(store.registry(), &live) {
            fields.insert("summary".into(), summary);
        }
        fields.insert("archive_reason".into(), original_reason);
        print_json(&success_envelope("restore", fields), false);
    } else {
        // JS truthiness: an empty id renders no segment (unreachable —
        // resolution needs a non-empty id).
        let rid = live
            .get("id")
            .and_then(serde_json::Value::as_str)
            .filter(|id| !id.is_empty())
            .map_or_else(String::new, |id| format!(" {id}"));
        let text = format!(
            "✓ Restored {}{rid} to {domain} (archived: {}): {}",
            removed.record_type(),
            value_text(&original_reason),
            crate::commands::failure::summary_text(
                mulch::record_summary_value(store.registry(), &live)
                    .ok()
                    .flatten()
                    .as_ref(),
            ),
        );
        print_line(opts.quiet, &text);
    }
    Ok(())
}

/// Strips the soft-archive lifecycle fields (`stripArchiveFields`):
/// status/archived_at/archive_reason drop, every other key keeps its
/// position.
fn strip_archive_fields(record: &serde_json::Value) -> serde_json::Value {
    let mut stripped = record.clone();
    if let Some(object) = stripped.as_object_mut() {
        for key in ["status", "archived_at", "archive_reason"] {
            object.shift_remove(key);
        }
    }
    stripped
}

/// A template-literal rendering of an optional field: absent keys
/// read `undefined`, present ones coerce like JS.
fn field_text(value: Option<&serde_json::Value>) -> String {
    value.map_or_else(|| "undefined".into(), value_text)
}
