//! The per-type compaction laws (reference `mergeRecords`, compact.ts):
//! the registry NAMES a strategy per type ([`CompactStrategy`]); this
//! module gives every arm a body, printless and channel-free.
//!
//! Why here and not in the command (ADR-0005): the strategy name is part
//! of the store's format contract — a custom type declares `compact: …`
//! in `mulch.config.yaml` — so the meaning of that name belongs beside
//! the registry that carries it, exactly like
//! [`crate::records::record_summary`] owns `SummaryLaw`. The command keeps what
//! is CLI-shaped: candidate discovery over the store, `--auto` vs `--apply`,
//! group warnings, the `payload.replacement` hook law (ADR-0003), and the
//! WORDING of every refusal this verdict can produce.

use serde_json::{Map, Value};

use crate::ids::{CompactStrategy, Registry, Row, record_id};

/// What a mechanical merge produced — a verdict, not a `Result`.
///
/// The two refusals are not errors of the same kind: [`Outcome::Manual`]
/// is a POLICY answer (`--auto` cannot merge such a type at all) and each
/// caller renders its own text, while [`Outcome::Empty`] is only
/// reachable through a direct caller — the CLI never forms a group of
/// fewer than two records.
#[derive(Debug)]
#[must_use = "a dropped verdict silently skips the merge (or its refusal)"]
pub enum Outcome {
    /// The merged record, id stamped (the id is part of this contract).
    Merged(Value),
    /// The type's strategy is `manual`: no mechanical merge exists.
    Manual,
    /// No records to merge — or a group whose newest member is not an
    /// object (contract-only; the CLI's groups are parsed records).
    /// Checked BEFORE the strategy dispatch, so an empty `manual`
    /// group also reports `Empty` (the reference's own order).
    Empty,
}

/// The mechanical merge for one registry row: strategy dispatch, the
/// shared envelope (classification, timestamp, supersedes, tag/file
/// unions), and the deterministic id over the merged body.
///
/// `now` is the caller's ISO timestamp — the law stays deterministic so
/// it can be pinned by a table of inputs and expected records.
///
/// Key order is part of the contract (serde_json preserves it and the
/// parity batteries compare bytes): `concat` builds a fresh object in
/// the reference's order, `keep_latest`/`merge_outcomes` edit the newest
/// record in place, and the id always lands LAST.
pub fn merge(records: &[Value], row: &Row, now: &str, registry: &Registry) -> Outcome {
    if records.is_empty() {
        return Outcome::Empty;
    }
    let supersedes: Vec<String> = records.iter().filter_map(id_of).collect();
    let tags = string_union(records, "tags");
    let files = row
        .files_array
        .as_deref()
        .and_then(|field| string_union(records, field));

    let mut merged = match row.compact {
        CompactStrategy::Manual => return Outcome::Manual,
        CompactStrategy::Concat => concat(records, row, now, &supersedes, tags, files),
        CompactStrategy::KeepLatest | CompactStrategy::MergeOutcomes => {
            // Unreachable after the empty check above (newest_first
            // never drops members); the arm only guards direct callers.
            let Some(mut latest) = newest_first(records).first().map(|value| (*value).clone())
            else {
                return Outcome::Empty;
            };
            let Some(map) = latest.as_object_mut() else {
                // A non-object member cannot be a record; the CLI's
                // groups are parsed records, so this arm is contract
                // only — the caller's `Empty` rendering covers it.
                return Outcome::Empty;
            };
            map.insert(
                "classification".into(),
                Value::String("foundational".into()),
            );
            map.insert("recorded_at".into(), Value::String(now.to_string()));
            map.insert("supersedes".into(), string_array(&supersedes));
            // `tags`: the union when any record carries one, ABSENT
            // otherwise — a surviving `tags` key from the newest record
            // would re-introduce entries no member declared.
            match tags {
                Some(tags) => {
                    map.insert("tags".into(), tags);
                }
                None => {
                    map.shift_remove("tags");
                }
            }
            if let Some(files) = files {
                map.insert(row.files_array.clone().unwrap_or_default(), files);
            }
            if row.compact == CompactStrategy::MergeOutcomes {
                let outcomes = outcomes_union(records);
                if !outcomes.is_empty() {
                    map.insert("outcomes".into(), Value::Array(outcomes));
                }
            }
            // `stamp_id` below removes a surviving id and re-appends it
            // LAST, so the arm never touches the key itself.
            latest
        }
    };
    stamp_id(&mut merged, registry);
    Outcome::Merged(merged)
}

/// `concat`: a FRESH record whose required fields join across the group
/// (blank-line separated), except `name`/`title`, which keep the LONGEST
/// value (ties keep the first).
fn concat(
    records: &[Value],
    row: &Row,
    now: &str,
    supersedes: &[String],
    tags: Option<Value>,
    files: Option<Value>,
) -> Value {
    let mut out = Map::new();
    out.insert("type".into(), Value::String(row.name.clone()));
    out.insert(
        "classification".into(),
        Value::String("foundational".into()),
    );
    out.insert("recorded_at".into(), Value::String(now.to_string()));
    out.insert("supersedes".into(), string_array(supersedes));
    if let Some(tags) = tags {
        out.insert("tags".into(), tags);
    }
    if let Some(files) = files {
        out.insert(row.files_array.clone().unwrap_or_default(), files);
    }
    for field in &row.payload {
        let values: Vec<&str> = records
            .iter()
            .filter_map(|record| record.get(field).and_then(Value::as_str))
            .collect();
        let value = if field == "name" || field == "title" {
            values.iter().fold(String::new(), |longest, next| {
                if next.len() > longest.len() {
                    (*next).to_string()
                } else {
                    longest
                }
            })
        } else {
            values.join("\n\n")
        };
        out.insert(field.clone(), Value::String(value));
    }
    Value::Object(out)
}

/// Records sorted newest-first by JS-Date comparison; an unparsable
/// stamp compares as equal (NaN comparator — order kept).
fn newest_first(records: &[Value]) -> Vec<&Value> {
    let mut sorted: Vec<&Value> = records.iter().collect();
    sorted.sort_by(|a, b| {
        match (
            crate::parse_js_date(&field_text(a, "recorded_at")),
            crate::parse_js_date(&field_text(b, "recorded_at")),
        ) {
            (Some(left), Some(right)) => right
                .partial_cmp(&left)
                .unwrap_or(std::cmp::Ordering::Equal),
            _ => std::cmp::Ordering::Equal,
        }
    });
    sorted
}

/// The union of a string-array field (deduped, insertion order);
/// `None` when no record carries entries.
fn string_union(records: &[Value], field: &str) -> Option<Value> {
    let mut seen: Vec<String> = Vec::new();
    for record in records {
        for entry in record
            .get(field)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(text) = entry.as_str()
                && !seen.iter().any(|existing| existing == text)
            {
                seen.push(text.to_string());
            }
        }
    }
    (!seen.is_empty()).then(|| string_array(&seen))
}

/// Every member's `outcomes[]`, concatenated in record order (the
/// `merge_outcomes` half of the strategy).
///
/// Deliberately NOT [`crate::upsert::merge_outcomes`]: that one merges an
/// existing record with an incoming upsert payload (two sides, one
/// overwrite), this one flattens an N-record group — same field, two
/// laws, different shapes.
fn outcomes_union(records: &[Value]) -> Vec<Value> {
    records
        .iter()
        .filter_map(|record| record.get("outcomes"))
        .filter_map(Value::as_array)
        .flatten()
        .cloned()
        .collect()
}

/// A JSON string array from the id list.
fn string_array(values: &[String]) -> Value {
    Value::Array(values.iter().map(|v| Value::String(v.clone())).collect())
}

/// The deterministic id over a record body (the hash half of the
/// reference `generateRecordId`): the id-key field of the record's OWN
/// registry row, re-hashed, an existing id removed, the fresh one
/// appended LAST (the positional half of the id law; key order is
/// byte-visible).
///
/// DEVIATION, pre-existing and preserved byte-neutral by the move: the
/// reference's `generateRecordId` THROWS for an unregistered type;
/// this port tolerates it with the `content` id-key fallback below.
/// The divergence's live surfaces are tracked in mulch-2809.
///
/// Public beyond `merge` because the two single-record paths stamp the
/// same way: `--apply`'s flag-built replacement and the pre-compact
/// hook's adopted record (ADR-0003 keeps those paths in the command,
/// not the stamp). The compaction arm of the write-time id trio — the
/// other two (`assign_missing_id`, `ensure_write_id`, one shared doc)
/// are re-exported from the private `upsert` module at the crate root.
pub fn stamp_id(record: &mut Value, registry: &Registry) {
    let record_type = record
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let id_key = registry
        .row(&record_type)
        .map_or("content", |row| row.id_key.as_str());
    let id = record_id(&record_type, &field_text(record, id_key));
    if let Some(object) = record.as_object_mut() {
        object.shift_remove("id");
        object.insert("id".into(), Value::String(id));
    }
}

/// `String(value ?? "")` of a field — the reference's `record[field] ??
/// ""` read. Private on purpose: the CLI has its own copy for its own
/// reads, and this one is a two-line accessor, not a law.
fn field_text(record: &Value, field: &str) -> String {
    record
        .get(field)
        .map_or_else(String::new, crate::jsval::value_text)
}

/// `record.id` as an optional string.
fn id_of(record: &Value) -> Option<String> {
    record.get("id").and_then(Value::as_str).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// The registry's row for `pattern` (concat) and one hand-built
    /// custom row, so the table needs no store.
    fn row(strategy: CompactStrategy) -> Row {
        let mut row = Registry::builtins()
            .row("pattern")
            .expect("pattern is a builtin")
            .clone();
        row.compact = strategy;
        row
    }

    fn registry() -> Registry {
        Registry::builtins()
    }

    const NOW: &str = "2026-10-25T10:00:00.000Z";

    fn merged(outcome: Outcome) -> Value {
        match outcome {
            Outcome::Merged(record) => record,
            other => panic!("expected a merged record, got {other:?}"),
        }
    }

    #[test]
    fn manual_and_empty_are_verdicts_not_errors() {
        // Catches: modelling `manual` as an error string inside the
        // lib — the channel owns that text, and --auto vs --apply say
        // different things about the same verdict.
        let group = [json!({"type": "pattern", "name": "a", "description": "d"})];
        assert!(matches!(
            merge(&group, &row(CompactStrategy::Manual), NOW, &registry()),
            Outcome::Manual
        ));
        assert!(matches!(
            merge(&[], &row(CompactStrategy::Concat), NOW, &registry()),
            Outcome::Empty
        ));
    }

    #[test]
    fn concat_joins_fields_and_keeps_the_longest_name() {
        // Catches: (a) joining `name` instead of keeping the longest,
        // (b) a tie picking the SECOND value, (c) a blank-line join
        // turned into a space, (d) the envelope order (type first,
        // supersedes before tags, payload fields last).
        let group = [
            json!({"type": "pattern", "id": "mx-1", "name": "short", "description": "one", "tags": ["t1"]}),
            json!({"type": "pattern", "id": "mx-2", "name": "much longer", "description": "two", "tags": ["t1", "t2"]}),
        ];
        let record = merged(merge(
            &group,
            &row(CompactStrategy::Concat),
            NOW,
            &registry(),
        ));
        assert_eq!(record["name"], json!("much longer"));
        assert_eq!(record["description"], json!("one\n\ntwo"));
        assert_eq!(record["classification"], json!("foundational"));
        assert_eq!(record["recorded_at"], json!(NOW));
        assert_eq!(record["supersedes"], json!(["mx-1", "mx-2"]));
        assert_eq!(record["tags"], json!(["t1", "t2"]));
        let keys: Vec<&str> = record
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "type",
                "classification",
                "recorded_at",
                "supersedes",
                "tags",
                "name",
                "description",
                "id"
            ],
            "envelope order is byte-visible"
        );
    }

    #[test]
    fn concat_ties_keep_the_first_value() {
        // Catches: `>=` where the reference has `>`, which would let
        // the LAST of two equal-length names win.
        let group = [
            json!({"type": "pattern", "name": "aaaa", "description": "x"}),
            json!({"type": "pattern", "name": "bbbb", "description": "y"}),
        ];
        let record = merged(merge(
            &group,
            &row(CompactStrategy::Concat),
            NOW,
            &registry(),
        ));
        assert_eq!(record["name"], json!("aaaa"));
    }

    #[test]
    fn keep_latest_takes_the_newest_body_and_puts_the_id_last() {
        // Catches: (a) taking the FIRST record instead of the newest,
        // (b) leaving the old id where it was (the id must be appended
        // last), (c) not stamping classification/recorded_at/supersedes.
        let group = [
            json!({"type": "pattern", "id": "old", "name": "old", "description": "old", "recorded_at": "2020-01-01T00:00:00.000Z"}),
            json!({"type": "pattern", "id": "new", "name": "new", "description": "new", "recorded_at": "2026-01-01T00:00:00.000Z"}),
        ];
        let record = merged(merge(
            &group,
            &row(CompactStrategy::KeepLatest),
            NOW,
            &registry(),
        ));
        assert_eq!(record["name"], json!("new"));
        assert_eq!(record["supersedes"], json!(["old", "new"]));
        assert_eq!(record["recorded_at"], json!(NOW));
        let keys: Vec<&str> = record
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys.last(), Some(&"id"), "the id must be LAST: {keys:?}");
        assert_ne!(record["id"], json!("new"), "the id is recomputed");
    }

    #[test]
    fn keep_latest_drops_tags_no_member_carries() {
        // Catches: leaving the newest record's `tags` in place when no
        // record contributes a union — the key must disappear.
        let group = [
            json!({"type": "pattern", "name": "n", "description": "d"}),
            json!({"type": "pattern", "name": "n2", "description": "d2"}),
        ];
        let record = merged(merge(
            &group,
            &row(CompactStrategy::KeepLatest),
            NOW,
            &registry(),
        ));
        assert!(!record.as_object().unwrap().contains_key("tags"));
    }

    #[test]
    fn merge_outcomes_flattens_every_members_outcomes_in_order() {
        // Catches: (a) keeping only the newest record's outcomes,
        // (b) reversing the union order, (c) writing an empty array
        // where the reference omits the key.
        let group = [
            json!({"type": "pattern", "name": "n", "description": "d",
                   "outcomes": [{"status": "success", "duration_ms": 1}]}),
            json!({"type": "pattern", "name": "n", "description": "d",
                   "outcomes": [{"status": "failure", "duration_ms": 2}]}),
        ];
        let record = merged(merge(
            &group,
            &row(CompactStrategy::MergeOutcomes),
            NOW,
            &registry(),
        ));
        assert_eq!(
            record["outcomes"],
            json!([
                {"status": "success", "duration_ms": 1},
                {"status": "failure", "duration_ms": 2}
            ])
        );

        let without = [
            json!({"type": "pattern", "name": "n", "description": "d"}),
            json!({"type": "pattern", "name": "n", "description": "d"}),
        ];
        let record = merged(merge(
            &without,
            &row(CompactStrategy::MergeOutcomes),
            NOW,
            &registry(),
        ));
        assert!(!record.as_object().unwrap().contains_key("outcomes"));
    }

    #[test]
    fn files_union_and_a_present_tags_union_survive_both_paths() {
        // Catches: (a) losing the `files` array union (a second
        // string-array field besides tags), (b) its envelope position
        // (after `tags` in concat), (c) keep_latest DROPPING a union
        // some member actually carries (only the no-union drop path
        // is pinned by the other table row).
        let group = [
            json!({"type": "pattern", "id": "mx-1", "name": "n", "description": "d",
                   "tags": ["t1"], "files": ["a.rs"]}),
            json!({"type": "pattern", "id": "mx-2", "name": "n2", "description": "d2",
                   "tags": ["t2"], "files": ["a.rs", "b.rs"]}),
        ];
        let concat = merged(merge(
            &group,
            &row(CompactStrategy::Concat),
            NOW,
            &registry(),
        ));
        assert_eq!(concat["files"], json!(["a.rs", "b.rs"]));
        let keys: Vec<&str> = concat
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let tags_at = keys
            .iter()
            .position(|k| *k == "tags")
            .expect("tags present");
        let files_at = keys
            .iter()
            .position(|k| *k == "files")
            .expect("files present");
        assert!(files_at > tags_at, "files lands after tags: {keys:?}");

        let latest = merged(merge(
            &group,
            &row(CompactStrategy::KeepLatest),
            NOW,
            &registry(),
        ));
        assert_eq!(latest["tags"], json!(["t1", "t2"]));
        assert_eq!(latest["files"], json!(["a.rs", "b.rs"]));
    }

    #[test]
    fn an_unparsable_stamp_keeps_the_group_order() {
        // Catches: a comparator that reorders on NaN (the reference's
        // JS sort treats an invalid date as "equal", so order is kept).
        let group = [
            json!({"type": "pattern", "id": "first", "name": "a", "description": "d", "recorded_at": "not a date"}),
            json!({"type": "pattern", "id": "second", "name": "b", "description": "d", "recorded_at": "also not"}),
        ];
        let record = merged(merge(
            &group,
            &row(CompactStrategy::KeepLatest),
            NOW,
            &registry(),
        ));
        assert_eq!(record["name"], json!("a"), "the first record stays newest");
    }

    #[test]
    fn the_cli_delegates_the_merge_law() {
        // Structural pin: the law has ONE body. Catches a later fix
        // re-growing a copy in the command (the drift this sprint
        // removed) and catches a call site that stops using it.
        const CLI: &str = include_str!("commands/compact.rs");
        for copy in [
            "fn merge_records",
            "fn newest_first",
            "fn string_union",
            "fn outcomes_union",
            "fn string_array",
            "fn stamp_id",
        ] {
            assert!(
                !CLI.contains(copy),
                "the command re-grew a merge helper: {copy}"
            );
        }
        // Substring, not full path: a `use mulch::compact::…` rewrite
        // of the call site must not trip the pin. (Names, not semantics:
        // a re-grown copy under a NEW name slips past the bans above —
        // the delegate assertions are the airtight half.)
        assert!(
            CLI.contains("compact::merge"),
            "the command must call the lib law"
        );
        assert!(
            CLI.contains("compact::stamp_id"),
            "the two single-record stamps must delegate too"
        );
    }

    #[test]
    fn stamp_id_falls_back_to_content_and_appends_last() {
        // Catches: (a) losing the `content` id-key fallback for
        // unregistered types (an unwrap or a wrong default key), (b)
        // leaving a pre-existing id in its old POSITION instead of
        // re-appending it last.
        let mut record = json!({"type": "not-a-type", "id": "old", "content": "body"});
        stamp_id(&mut record, &registry());
        assert_eq!(record["id"], json!(record_id("not-a-type", "body")));
        let keys: Vec<&str> = record
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys.last(), Some(&"id"), "the id must be LAST: {keys:?}");
    }
}
