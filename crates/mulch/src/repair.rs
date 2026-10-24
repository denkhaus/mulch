//! The line-preserving repair laws over [`StoreFiles`] bytes: the
//! generic rewrite passes `doctor --fix` drives over a domain file's
//! current text, shared with the future prune/archive commands
//! (mulch-7c55). Doctor owns the IO — reading the bytes (a missing
//! file skips silently), writing the repaired body back — while this
//! module owns the text-level laws: survivors keep their trimmed
//! bytes verbatim (raw passes) or rewrite canonically through
//! `rewrite_domain` (value passes).

use serde_json::Value;

use crate::{Registry, Result, StoreFiles};

/// One parsed line's fate in a line-preserving repair pass (jsonl
/// integrity, legacy outcome).
#[derive(Clone)]
pub enum LineAction {
    /// Keep the trimmed line verbatim.
    Keep,
    /// Replace the line with a new serialized one.
    Replace(String),
    /// Drop the line.
    Drop,
}

/// One line-preserving repair pass (the jsonl-integrity and
/// legacy-outcome fixes): every line parses, survivors keep their
/// trimmed bytes verbatim, and `action` decides each parsed line.
/// `unparsable` is the policy for lines that fail to parse — `Drop`
/// for jsonl-integrity (malformed lines and `#` comments are its
/// whole job), `Keep` for legacy-outcome (its raw-line loop passes
/// unparsable survivors through byte-identical). Returns the new
/// file body plus the number of lines not kept verbatim, `None` when
/// every line kept (no write).
pub fn raw_line_pass(
    text: &str,
    unparsable: &LineAction,
    action: impl Fn(&Value) -> LineAction,
) -> Option<(String, usize)> {
    let mut lines = Vec::new();
    let mut changed = 0usize;
    for line in text.split('\n') {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let outcome = serde_json::from_str::<Value>(trimmed)
            .map_or_else(|_| unparsable.clone(), |value| action(&value));
        match outcome {
            LineAction::Keep => lines.push(trimmed.to_string()),
            LineAction::Replace(replacement) => {
                lines.push(replacement);
                changed += 1;
            }
            LineAction::Drop => changed += 1,
        }
    }
    if changed == 0 {
        return None;
    }
    let mut body = lines.join("\n");
    if !lines.is_empty() {
        body.push('\n');
    }
    Some((body, changed))
}

/// One canonical-rewrite repair pass (the schema-validation and
/// stale-records fixes): every line parses, `keep` decides each
/// record, and the survivors rewrite through `rewrite_domain`
/// (`write_records` semantics incl. id backfill). Returns the number
/// of dropped records, `None` when nothing dropped (no write).
///
/// # Errors
///
/// Propagates the store's rewrite error — the pass's only error
/// source (the caller owns the read that produced `text`).
pub fn value_line_pass(
    store: &StoreFiles,
    domain: &str,
    text: &str,
    keep: impl Fn(&Value) -> bool,
) -> Result<Option<usize>> {
    let records = parsed_records(text, store.registry());
    let kept: Vec<Value> = records
        .iter()
        .filter(|record| keep(record))
        .cloned()
        .collect();
    let removed = records.len() - kept.len();
    if removed == 0 {
        return Ok(None);
    }
    store.rewrite_domain(domain, &kept)?;
    Ok(Some(removed))
}

/// The parseable records of a domain file's bytes, in line order —
/// the shared read half of the record-rewriting repair passes
/// (`value_line_pass`, doctor's anchors pass). Unparsable lines drop
/// uncounted (unreachable in practice: the reference's strict checks
/// crash on truly malformed lines before any repair; comment lines
/// were removed by the jsonl-integrity pass above), blank lines skip.
pub fn parsed_records(text: &str, registry: &Registry) -> Vec<Value> {
    let mut records: Vec<Value> = text
        .split('\n')
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    // The repair passes read like the reference's readExpertiseFile
    // (aliases applied) — the keep filters judge the canonical shape
    // and the rewrites land canonical (utils/expertise.ts,
    // mulch-b1cb).
    for record in &mut records {
        crate::apply_aliases_for(registry, record);
    }
    records
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A throwaway store with one live domain `d` for the pass tests.
    fn temp_store(tag: &str) -> (std::path::PathBuf, StoreFiles) {
        let dir = std::env::temp_dir().join(format!("mulch-repair-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(dir.join(".mulch").join("expertise")).expect("temp store");
        std::fs::write(
            dir.join(".mulch").join("mulch.config.yaml"),
            "version: '1'\ndomains:\n  d: {}\n",
        )
        .expect("config");
        let store = match StoreFiles::locate(&dir).expect("store opens") {
            crate::StoreLocation::Open(store) => store,
            other => panic!("store must open, got {other:?}"),
        };
        (dir, store)
    }

    fn read_domain(store: &StoreFiles, domain: &str) -> String {
        std::fs::read_to_string(store.domain_path(domain)).expect("domain file")
    }

    #[test]
    fn raw_line_pass_counts_changes_and_keeps_verbatim_bytes() {
        // dropping policy: unparsable lines (# comments included) drop,
        // survivors keep their trimmed bytes
        let dropped = raw_line_pass(
            "# banner\n{\"a\":1}\nnot json\n  {\"b\":2}  \n\n",
            &LineAction::Drop,
            |_| LineAction::Keep,
        )
        .expect("two lines dropped");
        assert_eq!(dropped, ("{\"a\":1}\n{\"b\":2}\n".to_string(), 2));
        // keeping policy: nothing changes -> None (the caller writes
        // nothing at all)
        assert_eq!(
            raw_line_pass("not json\n{\"a\":1}\n", &LineAction::Keep, |_| {
                LineAction::Keep
            }),
            None
        );
        // replacement: only replaced lines count, unparsable survivors
        // stay byte-identical
        let migrated = raw_line_pass("not json\n{\"a\":1}\n", &LineAction::Keep, |value| {
            value.get("a").map_or(LineAction::Keep, |_| {
                LineAction::Replace("{\"a\":2}".into())
            })
        })
        .expect("one line replaced");
        assert_eq!(migrated, ("not json\n{\"a\":2}\n".to_string(), 1));
    }

    #[test]
    fn value_line_pass_counts_drops_and_rewrites_canonically() {
        let (dir, store) = temp_store("value");
        let text = "{\"type\":\"pattern\",\"name\":\"p\"}\nnot json\n{\"junk\":1}\n";
        // unparsable lines drop uncounted; the typeless record drops
        let removed = value_line_pass(&store, "d", text, |record| record.get("type").is_some())
            .expect("pass runs");
        assert_eq!(removed, Some(1));
        // the canonical rewrite backfills the id (write_records)
        assert_eq!(
            read_domain(&store, "d"),
            format!(
                "{{\"type\":\"pattern\",\"name\":\"p\",\"id\":\"{}\"}}\n",
                crate::record_id("pattern", "p")
            )
        );
        // keep-all: None, and non-canonical spacing survives (no write)
        std::fs::write(store.domain_path("d"), "{\"a\": 1}\n").expect("writable");
        let untouched = value_line_pass(&store, "d", "{\"a\": 1}\n", |_| true).expect("pass runs");
        assert_eq!(untouched, None);
        assert_eq!(read_domain(&store, "d"), "{\"a\": 1}\n");
        std::fs::remove_dir_all(&dir).ok();
    }
}
