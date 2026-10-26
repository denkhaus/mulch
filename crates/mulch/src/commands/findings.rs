//! The shared findings-walk vocabulary (mulch-41cf): the
//! `domain:line`-addressed [`Finding`] triple, its parity-pinned
//! JSON shape, the malformed-line message, and the lenient store
//! walk. A skeleton only — policy (what counts as an error, escape
//! hatches, domain rules) stays with the commands that own it.

use serde_json::{Map, Value};

/// One lenient-walk finding (`domain:line` addressed).
pub(crate) struct Finding {
    pub(crate) domain:  String,
    pub(crate) line:    usize,
    pub(crate) message: String,
}

/// The finding triple's JSON shape — key order `domain`, `line`,
/// `message` (parity-pinned bytes; serde preserve_order).
pub(crate) fn finding_json(finding: &Finding) -> Value {
    let mut item = Map::new();
    item.insert("domain".into(), Value::String(finding.domain.clone()));
    item.insert(
        "line".into(),
        Value::from(u64::try_from(finding.line).unwrap_or(0)),
    );
    item.insert("message".into(), Value::String(finding.message.clone()));
    Value::Object(item)
}

/// The malformed-line message (reference validate/sync wording).
pub(crate) const MALFORMED_LINE: &str = "Invalid JSON: failed to parse";

/// Walks every configured domain leniently (config order, per-file
/// line order), visiting each line's parse outcome with its domain
/// name; returns the total line count (records + malformed — the
/// validate/sync `totalRecords` law).
pub(crate) fn walk_lenient(
    store: &mulch::StoreFiles,
    visit: &mut dyn FnMut(&str, mulch::LenientLine),
) -> usize {
    let mut total_records = 0;
    for domain in store.domains() {
        for finding in store.read_lenient(&domain) {
            total_records += 1;
            visit(&domain, finding);
        }
    }
    total_records
}

#[cfg(test)]
mod tests {
    use mulch::LenientLine;

    use super::*;
    use crate::commands::testscratch::store_with;

    /// The JSON triple's key order and values are pinned; a line
    /// number beyond u64 saturates to 0 like the command folds did.
    #[test]
    fn finding_json_pins_the_triple_order() {
        let finding = Finding {
            domain:  "dev".into(),
            line:    3,
            message: MALFORMED_LINE.into(),
        };
        let json = finding_json(&finding).to_string();
        assert_eq!(
            json,
            "{\"domain\":\"dev\",\"line\":3,\"message\":\"Invalid JSON: failed to parse\"}"
        );
    }

    /// The walker's skeleton law: config-order domains, per-file
    /// line order, records AND malformed lines visited, total lines
    /// returned.
    #[test]
    fn walk_lenient_visits_every_line_in_config_order() {
        let (root, store) = store_with(
            "walk",
            "version: '1'\ndomains:\n  dev: {}\n  ops: {}\n",
            true,
        );
        std::fs::write(
            root.join(".mulch/expertise/dev.jsonl"),
            "{\"id\":1}\nnot json\n",
        )
        .expect("dev file");
        std::fs::write(root.join(".mulch/expertise/ops.jsonl"), "{\"id\":2}\n").expect("ops file");
        let mut seen: Vec<(String, usize, bool)> = Vec::new();
        let total = walk_lenient(&store, &mut |domain, line| match line {
            LenientLine::Record { line, .. } => seen.push((domain.into(), line, true)),
            LenientLine::Malformed { line, .. } => seen.push((domain.into(), line, false)),
        });
        assert_eq!(total, 3);
        assert_eq!(seen, vec![
            ("dev".to_string(), 1, true),
            ("dev".to_string(), 2, false),
            ("ops".to_string(), 1, true),
        ]);
        std::fs::remove_dir_all(&root).ok();
    }
}
