//! Record-file semantics shared by the mutating commands: the strict
//! reader (reference `readExpertiseFile`), the compact re-serializing
//! writer (reference `writeExpertiseFile`), identifier resolution
//! (`resolveRecordId`) and the per-type summary
//! (`getRecordSummary` + `truncate`).

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::error::{Error, Result};
use crate::ids::{PAYLOAD_TYPES, id_key_field, record_id};

/// One parsed record with its physical line number.
#[derive(Debug)]
pub struct LineRecord {
    /// 1-based physical line number in the file.
    pub line:   usize,
    /// The parsed record (legacy `outcome` normalized to `outcomes`).
    pub record: Value,
}

impl LineRecord {
    /// The record's `id`, when present.
    pub fn id(&self) -> Option<&str> {
        self.record.get("id").and_then(Value::as_str)
    }

    /// The record's `type`.
    pub fn record_type(&self) -> String {
        self.record
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("convention")
            .to_string()
    }
}

/// Reads a domain file strictly: blank and `#` lines are skipped,
/// malformed lines and unregistered types are errors (nothing is
/// written by callers that use this reader).
///
/// # Errors
///
/// [`Error::MalformedLine`] for unparsable lines,
/// [`Error::UnknownRecordType`] for unregistered types (unless
/// `allow_unknown`), [`Error::Read`] for I/O failures.
pub fn read_strict(path: &Path, allow_unknown: bool) -> Result<Vec<LineRecord>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(source) => {
            return Err(Error::Read {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    let mut records = Vec::new();
    for (index, line) in text.split('\n').enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut record: Value = serde_json::from_str(line).map_err(|source| {
            let preview = if trimmed.len() > 80 {
                format!("{}...", &trimmed[..77])
            } else {
                trimmed.to_string()
            };
            Error::MalformedLine {
                path: path.to_path_buf(),
                line: index + 1,
                preview,
                reason: source.to_string(),
            }
        })?;
        // Scalars and null crash the reference reader (`"outcome" in
        // raw` on a non-object); objects and arrays pass (arrays count
        // as records there). Clean error instead — README DEVIATIONS.
        if !matches!(record, Value::Object(_) | Value::Array(_)) {
            let preview = if trimmed.len() > 80 {
                format!("{}...", &trimmed[..77])
            } else {
                trimmed.to_string()
            };
            return Err(Error::NotAnObject {
                path: path.to_path_buf(),
                line: index + 1,
                preview,
            });
        }
        normalize_legacy_outcome(&mut record);
        let kind = record.get("type").and_then(Value::as_str).unwrap_or("");
        if !allow_unknown && !kind.is_empty() && !PAYLOAD_TYPES.contains(&kind) {
            return Err(Error::UnknownRecordType {
                path:        path.to_path_buf(),
                line:        index + 1,
                id:          record.get("id").and_then(Value::as_str).map(str::to_string),
                record_type: kind.to_string(),
            });
        }
        records.push(LineRecord {
            line: index + 1,
            record,
        });
    }
    Ok(records)
}

/// Reference legacy normalization: a singular `outcome` object becomes
/// a one-element `outcomes` array (only when `outcomes` is absent).
fn normalize_legacy_outcome(record: &mut Value) {
    let Some(object) = record.as_object_mut() else {
        return;
    };
    if object.contains_key("outcomes") {
        return;
    }
    let Some(legacy) = object.get("outcome").and_then(Value::as_object).cloned() else {
        object.remove("outcome");
        return;
    };
    let mut normalized = serde_json::Map::new();
    if let Some(status) = legacy.get("status") {
        normalized.insert("status".into(), status.clone());
    }
    for key in ["duration", "test_results", "agent"] {
        if let Some(value) = legacy.get(key) {
            normalized.insert(key.into(), value.clone());
        }
    }
    object.insert(
        "outcomes".into(),
        Value::Array(vec![Value::Object(normalized)]),
    );
    object.remove("outcome");
}

/// Renders a JSON value the way a JS template literal or property key
/// would stringify it: strings raw, `null` as "null", arrays joined
/// with "," (null items empty), objects as "[object Object]"
/// (probe-pinned 2026-10-04, ml 0.10.7).
pub fn value_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => "null".into(),
        Value::Array(items) => items
            .iter()
            .map(|item| match item {
                Value::Null => String::new(),
                other => value_text(other),
            })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".into(),
        other => other.to_string(),
    }
}

/// The reference's duplicate detector (`findDuplicate`,
/// utils/expertise.ts): the first same-type record whose dedup-field
/// value equals the candidate's. The dedup field is the registry's
/// dedupKey — same mapping as [`id_key_field`] — never the id, so a
/// renamed record still dedupes after `edit --name`. Unregistered
/// types never duplicate (the reference registry has no definition);
/// records missing the field on both sides match, like JS
/// `undefined === undefined`.
pub fn find_duplicate<'a, I>(records: I, candidate: &Value) -> Option<usize>
where
    I: IntoIterator<Item = &'a Value>,
{
    let record_type = candidate.get("type").and_then(Value::as_str)?;
    if !PAYLOAD_TYPES.contains(&record_type) {
        return None;
    }
    let key = id_key_field(record_type);
    let new_value = candidate.get(key);
    records.into_iter().position(|record| {
        record.get("type").and_then(Value::as_str) == Some(record_type)
            && js_dedup_eq(record.get(key), new_value)
    })
}

/// JS strict equality (`===`) for the reference's dedup-field
/// comparison (`findDuplicate`, utils/expertise.ts): missing fields
/// compare equal (`undefined === undefined`), numbers compare as f64
/// (`1` equals `1.0` — both parse to the same JS number), and objects
/// or arrays never match because `===` compares object identity and
/// separately parsed lines never share one.
fn js_dedup_eq(a: Option<&Value>, b: Option<&Value>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(Value::Number(a)), Some(Value::Number(b))) => a.as_f64() == b.as_f64(),
        // objects/arrays never match (JS === compares identity), and
        // undefined never equals a present value (and vice versa) —
        // this arm MUST precede the structural-eq arm or identical
        // object shapes would wrongly match
        (Some(Value::Object(_) | Value::Array(_)), _)
        | (_, Some(Value::Object(_) | Value::Array(_)))
        | (None, Some(_))
        | (Some(_), None) => false,
        (Some(a), Some(b)) => a == b,
    }
}

/// Merges both sides' outcomes into `incoming` (reference
/// `{ ...record, outcomes: merged }`, existing first): replaces the
/// `outcomes` key in place when the incoming record already has one,
/// appends it at the end otherwise. The id stays untouched.
pub fn merge_outcomes(existing: &Value, mut incoming: Map<String, Value>) -> Map<String, Value> {
    let mut merged: Vec<Value> = Vec::new();
    for source in [existing.get("outcomes"), incoming.get("outcomes")] {
        if let Some(outcomes) = source.and_then(Value::as_array) {
            merged.extend(outcomes.iter().cloned());
        }
    }
    if !merged.is_empty() {
        incoming.insert("outcomes".into(), Value::Array(merged));
    }
    incoming
}

/// The flag-path upsert shape: merged outcomes, then the id LAST —
/// the builder pre-assigns the id, so it lifts over the appended
/// outcomes (probe-pinned key order; batch paths keep input-id
/// positions and use [`merge_outcomes`] directly).
pub fn upsert_record(existing: &Value, mut incoming: Map<String, Value>) -> Map<String, Value> {
    let id_value = incoming.remove("id");
    let mut merged = merge_outcomes(existing, incoming);
    if let Some(id_value) = id_value {
        merged.insert("id".into(), id_value);
    }
    merged
}

/// Writes records compactly (reference `writeExpertiseFile`): missing
/// ids are generated, lines are compact JSON, the file ends with `\n`
/// (0 bytes when empty), written through a temp file + rename.
///
/// # Errors
///
/// [`Error::Write`] on I/O failure.
pub fn write_records(path: &Path, records: &[Value]) -> Result<()> {
    let mut body = String::new();
    for record in records {
        let mut record = record.clone();
        assign_missing_id(&mut record);
        body.push_str(&serde_json::to_string(&record).unwrap_or_default());
        body.push('\n');
    }
    if records.is_empty() {
        body.clear();
    }
    // A unique temp name (the reference uses a random suffix) so two
    // writers never share a scratch file, with best-effort cleanup on a
    // failed rename.
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let temp = PathBuf::from(format!("{}.tmp.{unique:x}", path.display()));
    std::fs::write(&temp, body).map_err(|source| Error::Write {
        path: temp.clone(),
        source,
    })?;
    std::fs::rename(&temp, path).map_err(|source| {
        std::fs::remove_file(&temp).ok();
        Error::Write {
            path: path.to_path_buf(),
            source,
        }
    })
}

/// Assigns the deterministic id when a record lacks one.
pub fn assign_missing_id(record: &mut Value) {
    let Some(object) = record.as_object_mut() else {
        return;
    };
    if object.get("id").and_then(Value::as_str).is_some() {
        return;
    }
    let kind = object
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("convention")
        .to_string();
    let key_value = object
        .get(id_key_field(&kind))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    object.insert("id".into(), Value::String(record_id(&kind, &key_value)));
}

/// Why an identifier did not resolve.
#[derive(Debug)]
pub enum ResolveError {
    /// No record matched.
    NotFound(String),
    /// Several records matched the prefix.
    Ambiguous {
        /// The queried identifier.
        identifier: String,
        /// How many matched.
        count:      usize,
        /// The matching ids.
        ids:        Vec<String>,
    },
}

/// One lenient line finding (reference `validate`/`doctor` raw line
/// loops): only blank lines are skipped; every other line carries its
/// parse outcome — `#` comments included, which those loops report as
/// invalid-JSON findings. Line numbers are 1-based and physical.
#[derive(Debug)]
pub enum LenientLine {
    /// A parsed record — any JSON value; shape checks belong to the
    /// consumer.
    Record { line: usize, record: Value },
    /// An unparsable line.
    Malformed { line: usize },
}

/// Reads a record file leniently, per line (reference validate/doctor
/// semantics): parse failures are findings, not errors. A missing file
/// reads as empty; other I/O problems read as empty too (the lenient
/// readers never fail the command).
pub fn read_lenient(path: &Path) -> Vec<LenientLine> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let mut lines = Vec::new();
    for (index, line) in text.split('\n').enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let finding = match serde_json::from_str::<Value>(trimmed) {
            Ok(record) => LenientLine::Record {
                line: index + 1,
                record,
            },
            Err(_) => LenientLine::Malformed { line: index + 1 },
        };
        lines.push(finding);
    }
    lines
}

/// Reference `resolveRecordId`: exact match on `mx-<hash>` or a bare
/// hash, then a unique prefix match.
///
/// # Errors
///
/// [`ResolveError::NotFound`], [`ResolveError::Ambiguous`].
pub fn resolve_record_id(
    records: &[LineRecord],
    identifier: &str,
) -> std::result::Result<usize, ResolveError> {
    let hash = identifier.strip_prefix("mx-").unwrap_or(identifier);
    let target = format!("mx-{hash}");
    if let Some(index) = records
        .iter()
        .position(|line| line.id() == Some(target.as_str()))
    {
        return Ok(index);
    }
    let matches: Vec<usize> = records
        .iter()
        .enumerate()
        .filter(|(_, line)| line.id().is_some_and(|id| id.starts_with(&target)))
        .map(|(index, _)| index)
        .collect();
    match matches.as_slice() {
        [single] => Ok(*single),
        [] => Err(ResolveError::NotFound(identifier.to_string())),
        many => Err(ResolveError::Ambiguous {
            identifier: identifier.to_string(),
            count:      many.len(),
            ids:        many
                .iter()
                .filter_map(|index| records[*index].id().map(str::to_string))
                .collect(),
        }),
    }
}

/// Reference per-type summary (`getRecordSummary`).
pub fn record_summary(record: &Value) -> String {
    let kind = record
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("convention");
    let value = record
        .get(id_key_field(kind))
        .and_then(Value::as_str)
        .unwrap_or_default();
    match kind {
        "convention" | "failure" => truncate(value, 60),
        _ => value.to_string(),
    }
}

/// Reference `truncate(text, maxLen)`: a sentence end inside the window
/// wins, otherwise `slice(0, maxLen) + "..."`.
fn truncate(text: &str, max_len: usize) -> String {
    if text.chars().count() <= max_len {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let window: String = chars[..max_len.min(chars.len())].iter().collect();
    if let Some(position) = sentence_end(&window) {
        return window.chars().take(position + 1).collect();
    }
    format!("{window}...")
}

/// The first `[.!?]` followed by whitespace.
fn sentence_end(window: &str) -> Option<usize> {
    let bytes: Vec<char> = window.chars().collect();
    for (index, ch) in bytes.iter().enumerate() {
        if matches!(ch, '.' | '!' | '?')
            && bytes
                .get(index + 1)
                .is_some_and(|next| next.is_whitespace())
            && index > 0
            && index < window.chars().count()
        {
            return Some(index);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn find_duplicate_matches_on_the_dedup_field_not_the_id() {
        let store = [
            json!({"type": "pattern", "name": "p", "id": "mx-old"}),
            json!({"type": "pattern", "name": "q", "id": "mx-q"}),
        ];
        // same name, different id: still the duplicate
        assert_eq!(
            find_duplicate(store.iter(), &json!({"type": "pattern", "name": "p"})),
            Some(0)
        );
        assert_eq!(
            find_duplicate(store.iter(), &json!({"type": "pattern", "name": "zz"})),
            None
        );
        // same dedup VALUE on another type never matches
        assert_eq!(
            find_duplicate(store.iter(), &json!({"type": "guide", "name": "p"})),
            None
        );
    }

    #[test]
    fn find_duplicate_matches_missing_fields_and_skips_unregistered_types() {
        let bare = [json!({"type": "failure", "resolution": "r"})];
        // both sides missing the dedup field match (undefined === undefined)
        assert_eq!(
            find_duplicate(bare.iter(), &json!({"type": "failure", "resolution": "r2"})),
            Some(0)
        );
        let store = [json!({"type": "failure", "description": "d"})];
        // one side missing does NOT match a present value
        assert_eq!(
            find_duplicate(store.iter(), &json!({"type": "failure", "resolution": "r"})),
            None
        );
        // unregistered types never duplicate (no registry definition)
        assert_eq!(
            find_duplicate(store.iter(), &json!({"type": "custom", "description": "d"})),
            None
        );
        // a record without a type never duplicates
        assert_eq!(find_duplicate(store.iter(), &json!({"name": "p"})), None);
    }

    #[test]
    fn find_duplicate_follows_js_strict_equality() {
        // numbers compare as f64: 1 matches 1.0 (same JS number)
        let store = [json!({"type": "pattern", "name": 1})];
        assert_eq!(
            find_duplicate(store.iter(), &json!({"type": "pattern", "name": 1.0})),
            Some(0)
        );
        // like-typed only: "1" never matches 1
        assert_eq!(
            find_duplicate(store.iter(), &json!({"type": "pattern", "name": "1"})),
            None
        );
        // null only matches null, never a present value
        let nulls = [json!({"type": "pattern", "name": null})];
        assert_eq!(
            find_duplicate(nulls.iter(), &json!({"type": "pattern", "name": null})),
            Some(0)
        );
        assert_eq!(
            find_duplicate(nulls.iter(), &json!({"type": "pattern"})),
            None
        );
        // objects/arrays never match (JS === compares identity)
        let objects = [json!({"type": "pattern", "name": {"a": 1}})];
        assert_eq!(
            find_duplicate(
                objects.iter(),
                &json!({"type": "pattern", "name": {"a": 1}})
            ),
            None
        );
        let arrays = [json!({"type": "pattern", "name": [1]})];
        assert_eq!(
            find_duplicate(arrays.iter(), &json!({"type": "pattern", "name": [1]})),
            None
        );
    }
}

#[cfg(test)]
mod upsert_record_tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn merges_outcomes_existing_first_and_appends_the_id_last() {
        let upserted = upsert_record(
            &json!({"name": "p", "outcomes": [{"status": "success"}], "id": "mx-old"}),
            json!({"name": "p", "id": "mx-new"})
                .as_object()
                .cloned()
                .expect("object"),
        );
        assert_eq!(
            Value::Object(upserted),
            json!({"name": "p", "outcomes": [{"status": "success"}], "id": "mx-new"})
        );
    }

    #[test]
    fn keeps_an_explicit_id_and_skips_the_outcomes_key_when_both_empty() {
        let upserted = upsert_record(
            &json!({"name": "p"}),
            json!({"name": "p", "id": "mx-explicit"})
                .as_object()
                .cloned()
                .expect("object"),
        );
        assert_eq!(
            Value::Object(upserted),
            json!({"name": "p", "id": "mx-explicit"})
        );
    }
}

#[cfg(test)]
mod lenient_tests {
    use super::*;

    #[test]
    fn read_lenient_reports_parse_outcomes_with_line_numbers() {
        let dir = std::env::temp_dir().join(format!("mulch-lenient-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("d.jsonl");
        std::fs::write(
            &path,
            "# banner\n{\"type\":\"convention\"}\n\nnot json\n[1,2]\n",
        )
        .expect("writable");

        let findings = read_lenient(&path);
        assert_eq!(findings.len(), 4, "only the blank line skips");
        // comment lines are findings too (reference validate/doctor
        // raw loops flag them as invalid JSON)
        assert!(matches!(findings[0], LenientLine::Malformed { line: 1 }));
        assert!(matches!(findings[1], LenientLine::Record { line: 2, .. }));
        assert!(matches!(findings[2], LenientLine::Malformed { line: 4 }));
        assert!(matches!(findings[3], LenientLine::Record { line: 5, .. }));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn read_strict_rejects_scalars_but_keeps_arrays() {
        let dir = std::env::temp_dir().join(format!("mulch-strict-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("d.jsonl");

        std::fs::write(&path, "[1,2,3]\n").expect("writable");
        let records = read_strict(&path, false).expect("arrays are records");
        assert_eq!(records.len(), 1);

        std::fs::write(&path, "5\n").expect("writable");
        let error = read_strict(&path, false).expect_err("scalars are errors");
        assert!(matches!(error, Error::NotAnObject { line: 1, .. }));

        std::fs::write(&path, "null\n").expect("writable");
        assert!(matches!(
            read_strict(&path, false),
            Err(Error::NotAnObject { .. })
        ));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn read_lenient_missing_file_reads_empty() {
        let missing = std::path::Path::new("/nonexistent-mulch-probe/d.jsonl");
        assert!(read_lenient(missing).is_empty());
    }
}
