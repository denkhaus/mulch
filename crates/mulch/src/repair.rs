//! The line-preserving repair laws over a domain file's TEXT: the
//! generic passes `doctor --fix` drives, shared with the future
//! prune/archive commands (mulch-7c55). The CALLER owns every byte of
//! IO — reading the domain text (a missing file skips silently),
//! writing the repaired body or the kept records back through the
//! store seam — while this module owns the text-level laws only:
//! survivors keep their trimmed bytes verbatim (raw passes) or the
//! kept records return for a canonical rewrite (value passes,
//! mulch-d343's one-write-owner rule).

use serde_json::Value;

use crate::Registry;

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
/// record. Returns the KEPT records plus the number dropped — the
/// caller rewrites the domain through the store seam
/// (`rewrite_domain`, `write_records` semantics incl. id backfill);
/// `None` when nothing dropped (no write at all).
pub fn value_line_pass(
    text: &str,
    registry: &Registry,
    keep: impl Fn(&Value) -> bool,
) -> Option<(Vec<Value>, usize)> {
    let records = parsed_records(text, registry);
    let kept: Vec<Value> = records
        .iter()
        .filter(|record| keep(record))
        .cloned()
        .collect();
    let removed = records.len() - kept.len();
    (removed != 0).then_some((kept, removed))
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
    use crate::StoreFiles;

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
    fn value_line_pass_returns_kept_records_and_the_caller_writes() {
        let (dir, store) = temp_store("value");
        let text = "{\"type\":\"pattern\",\"name\":\"p\"}\nnot json\n{\"junk\":1}\n";
        std::fs::write(store.domain_path("d"), text).expect("seed the domain file");
        // unparsable lines drop uncounted; the typeless record drops
        let (kept, removed) = value_line_pass(text, store.registry(), |record| {
            record.get("type").is_some()
        })
        .expect("pass drops one");
        assert_eq!(removed, 1);
        // The pass wrote NOTHING — the caller owns the write, so the
        // bytes on disk are still the pre-pass state verbatim.
        assert_eq!(read_domain(&store, "d"), text);
        // the canonical rewrite (caller-side) backfills the id
        store.rewrite_domain("d", &kept).expect("write");
        assert_eq!(
            read_domain(&store, "d"),
            format!(
                "{{\"type\":\"pattern\",\"name\":\"p\",\"id\":\"{}\"}}\n",
                crate::record_id("pattern", "p")
            )
        );
        // keep-all: None, and non-canonical spacing survives (no write)
        std::fs::write(store.domain_path("d"), "{\"a\": 1}\n").expect("writable");
        let untouched = value_line_pass("{\"a\": 1}\n", store.registry(), |_| true);
        assert_eq!(untouched, None);
        assert_eq!(read_domain(&store, "d"), "{\"a\": 1}\n");
        std::fs::remove_dir_all(&dir).ok();
    }
}
