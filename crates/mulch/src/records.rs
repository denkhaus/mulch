//! Record-file semantics shared by the mutating commands: the strict
//! reader (reference `readExpertiseFile`), the compact re-serializing
//! writer (reference `writeExpertiseFile`), identifier resolution
//! (`resolveRecordId`) and the per-type summary
//! (`getRecordSummary` + `truncate`).

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::error::{Error, Result};
use crate::ids::{PAYLOAD_TYPES, id_key_field, is_named_type, record_id};
use crate::jsval::{js_dedup_eq, parse_non_negative_number, truthy_str};

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
        crate::ids::effective_type(self.record.get("type").and_then(Value::as_str)).to_string()
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

/// Reference legacy-outcome detection (validate.ts + doctor's
/// `checkLegacyOutcome`): a singular `outcome` key on a record without
/// an `outcomes` array. The value itself is never inspected — a `null`
/// outcome detects too; only migration skips it.
pub fn legacy_outcome(record: &Value) -> Option<&Value> {
    let object = record.as_object()?;
    if object.contains_key("outcomes") {
        return None;
    }
    object.get("outcome")
}

/// The migrated entry for one legacy `outcome` value (reference
/// `readExpertiseFile` + doctor's legacy fix): `status`, `duration`,
/// `test_results`, `agent` — present keys only, in that order. A
/// non-object value yields the empty entry: the reference reads
/// `.status` & co. as `undefined` on it and JSON serialization drops
/// undefined values, so `"outcome": 5` migrates to `outcomes: [{}]`
/// and its read-only listing renders `undefined` (mulch-1609).
pub fn legacy_entry(value: &Value) -> Map<String, Value> {
    let mut entry = Map::new();
    if let Some(fields) = value.as_object() {
        for key in ["status", "duration", "test_results", "agent"] {
            if let Some(field) = fields.get(key) {
                entry.insert(key.into(), field.clone());
            }
        }
    }
    entry
}

/// Reference legacy normalization (the strict reader's tail): a
/// singular non-null `outcome` (with `outcomes` absent) becomes a
/// one-element `outcomes` array of its [`legacy_entry`]; a `null`
/// outcome stays untouched — the reference's `raw.outcome !== null`
/// guard leaves the key in place for writes to preserve.
fn normalize_legacy_outcome(record: &mut Value) {
    let Some(legacy) = legacy_outcome(record).filter(|value| !value.is_null()) else {
        return;
    };
    let entry = legacy_entry(legacy);
    let Some(object) = record.as_object_mut() else {
        return;
    };
    // shift_remove keeps the remaining keys in place (plain remove
    // swaps the last entry into the freed slot) and the fresh key
    // appends at the end — the reference's `{ ...raw, outcome:
    // undefined, outcomes: [...] }` stringified shape.
    object.shift_remove("outcome");
    object.insert("outcomes".into(), Value::Array(vec![Value::Object(entry)]));
}

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

/// One outcome entry's raw flag strings — the shared owner of the
/// entry key order per command family and the strict duration parse
/// (the three outcome builders in record.ts/edit.ts/outcome.ts plus
/// utils/numeric-flags.ts), and the builders' JS truthiness law
/// (mulch-1209): empty `agent`/`notes`/`test_results` flags are
/// dropped, `duration` is not (`--duration ""` reaches the strict
/// parse and errors). `timestamped` selects the `outcome` command
/// family (status, recorded_at, duration, agent, notes,
/// test_results); the record/edit family writes status, duration,
/// test_results, agent. Plain data: fill the fields, call [`build`].
///
/// [`build`]: OutcomeEntry::build
pub struct OutcomeEntry<'a> {
    /// Whether recorded_at is stamped (the `outcome` command family).
    pub timestamped:   bool,
    /// Outcome verdict (`success` | `failure` | `partial`).
    pub status:        &'a str,
    /// `now` in ISO format — inserted as recorded_at when timestamped.
    pub now:           &'a str,
    /// Raw `--duration`/`--outcome-duration` flag value.
    pub duration:      Option<&'a str>,
    /// The flag name as it appears in the parse error.
    pub duration_flag: &'a str,
    /// Recording agent name.
    pub agent:         Option<&'a str>,
    /// Free-text notes (timestamped family only).
    pub notes:         Option<&'a str>,
    /// Test results summary.
    pub test_results:  Option<&'a str>,
}

impl OutcomeEntry<'_> {
    /// Builds the entry map in the family key order. `Err` carries the
    /// reference's parse-error line WITHOUT the `Error: ` prefix — the
    /// caller renders it through its command-named failure channel.
    pub fn build(&self) -> std::result::Result<Map<String, Value>, String> {
        let mut entry = Map::new();
        entry.insert("status".into(), Value::String(self.status.into()));
        if self.timestamped {
            entry.insert("recorded_at".into(), Value::String(self.now.into()));
        }
        if let Some(raw) = self.duration {
            let Some(value) = parse_non_negative_number(raw) else {
                return Err(format!(
                    "{} must be a non-negative number (got \"{raw}\").",
                    self.duration_flag
                ));
            };
            entry.insert("duration".into(), value);
        }
        // JS truthiness (reference `if (options.agent)` class,
        // mulch-1209): an empty flag value is absent — never stored,
        // never rendered. The jsval seam owns the law.
        let agent = truthy_str(self.agent);
        let notes = truthy_str(self.notes);
        let test_results = truthy_str(self.test_results);
        if self.timestamped {
            if let Some(agent) = agent {
                entry.insert("agent".into(), Value::String(agent.into()));
            }
            if let Some(notes) = notes {
                entry.insert("notes".into(), Value::String(notes.into()));
            }
            if let Some(test_results) = test_results {
                entry.insert("test_results".into(), Value::String(test_results.into()));
            }
        } else {
            if let Some(test_results) = test_results {
                entry.insert("test_results".into(), Value::String(test_results.into()));
            }
            if let Some(agent) = agent {
                entry.insert("agent".into(), Value::String(agent.into()));
            }
        }
        Ok(entry)
    }
}

/// The dedup→update/skip/create decision for one candidate against a
/// working set — the one owner of the registry rule shared by the
/// record flag path and the batch/stdin loop (reference record.ts +
/// `processStdinRecords`): a dedup-field match on a NAMED type upserts
/// with merged outcomes, an anonymous duplicate skips, and `--force`
/// (or no match) appends. The id-placement post-steps stay with the
/// callers (flag: id last; batch: input-id position, generated ids
/// after the merged outcomes).
#[derive(Debug)]
pub enum UpsertPlan {
    /// Append as a new record (no dedup match, or `--force`).
    Create,
    /// Named-type upsert: replace the record at `index` with `merged`
    /// (incoming fields, outcomes merged existing-first).
    Update {
        index:  usize,
        merged: Map<String, Value>,
    },
    /// Anonymous duplicate: keep the existing record at `index`.
    Skip { index: usize },
}

/// Computes the [`UpsertPlan`] for `candidate` against `working`
/// (existing records plus this batch's own appends — within-batch
/// duplicates upsert too; a dry-run pass keeps `working` unmutated so
/// within-batch duplicates count as creates there).
pub fn upsert_plan(working: &[Value], candidate: &Value, force: bool) -> UpsertPlan {
    if !force && let Some(index) = find_duplicate(working, candidate) {
        let named = candidate
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(is_named_type);
        if named {
            let incoming = candidate.as_object().cloned().unwrap_or_default();
            let merged = merge_outcomes(&working[index], incoming);
            return UpsertPlan::Update { index, merged };
        }
        return UpsertPlan::Skip { index };
    }
    UpsertPlan::Create
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
    let kind = crate::ids::effective_type(object.get("type").and_then(Value::as_str)).to_string();
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
    let kind = crate::ids::effective_type(record.get("type").and_then(Value::as_str));
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
mod upsert_plan_tests {
    use serde_json::json;

    use super::{UpsertPlan, upsert_plan};

    #[test]
    fn plan_upserts_named_skips_anonymous_and_forces_creates() {
        let working = [
            json!({"type": "pattern", "name": "p", "outcomes": [{"status": "success"}]}),
            json!({"type": "convention", "content": "c"}),
        ];
        // named duplicate: Update with outcomes merged existing-first
        let incoming = json!({"type": "pattern", "name": "p", "outcomes": [{"status": "failure"}]});
        match upsert_plan(&working, &incoming, false) {
            UpsertPlan::Update { index, merged } => {
                assert_eq!(index, 0);
                assert_eq!(
                    merged.get("outcomes"),
                    Some(&json!([{"status": "success"}, {"status": "failure"}]))
                );
            }
            other => panic!("expected Update, got {other:?}"),
        }
        // anonymous duplicate: Skip
        match upsert_plan(
            &working,
            &json!({"type": "convention", "content": "c"}),
            false,
        ) {
            UpsertPlan::Skip { index } => assert_eq!(index, 1),
            other => panic!("expected Skip, got {other:?}"),
        }
        // force appends even on a duplicate
        assert!(matches!(
            upsert_plan(&working, &json!({"type": "pattern", "name": "p"}), true),
            UpsertPlan::Create
        ));
        // no match: Create
        assert!(matches!(
            upsert_plan(&working, &json!({"type": "pattern", "name": "q"}), false),
            UpsertPlan::Create
        ));
    }
}

#[cfg(test)]
mod outcome_entry_tests {
    use serde_json::{Map, json};

    use super::OutcomeEntry;

    fn key_order(map: &Map<String, serde_json::Value>) -> Vec<&str> {
        map.keys().map(String::as_str).collect()
    }

    #[test]
    fn build_orders_keys_per_family_and_errors_with_flag_name() {
        let timestamped = OutcomeEntry {
            timestamped:   true,
            status:        "success",
            now:           "2026-10-06T19:00:00.000Z",
            duration:      Some("7.25"),
            duration_flag: "--duration",
            agent:         Some("oa"),
            notes:         Some("on"),
            test_results:  Some("ot"),
        }
        .build()
        .expect("valid flags build");
        assert_eq!(key_order(&timestamped), [
            "status",
            "recorded_at",
            "duration",
            "agent",
            "notes",
            "test_results"
        ]);
        assert_eq!(timestamped["duration"], json!(7.25));

        let plain = OutcomeEntry {
            timestamped:   false,
            status:        "success",
            now:           "",
            duration:      Some("42.0"),
            duration_flag: "--outcome-duration",
            agent:         Some("ra"),
            notes:         Some("ignored on this family"),
            test_results:  Some("rt"),
        }
        .build()
        .expect("valid flags build");
        assert_eq!(key_order(&plain), [
            "status",
            "duration",
            "test_results",
            "agent"
        ]);
        // "42.0" canonicalizes to the integer 42
        assert_eq!(plain["duration"], json!(42));

        let error = OutcomeEntry {
            timestamped:   false,
            status:        "success",
            now:           "",
            duration:      Some("abc"),
            duration_flag: "--outcome-duration",
            agent:         None,
            notes:         None,
            test_results:  None,
        }
        .build()
        .expect_err("invalid duration errors");
        assert_eq!(
            error,
            "--outcome-duration must be a non-negative number (got \"abc\")."
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

#[cfg(test)]
mod legacy_tests {
    use serde_json::json;

    use super::{Value, legacy_entry, legacy_outcome};

    fn normalized(line: &str) -> Value {
        let mut record: Value = serde_json::from_str(line).expect("json");
        super::normalize_legacy_outcome(&mut record);
        record
    }

    #[test]
    fn detection_requires_a_singular_outcome_key() {
        // null detects too — only migration skips it
        assert_eq!(
            legacy_outcome(&json!({"outcome": null})),
            Some(&Value::Null)
        );
        assert_eq!(legacy_outcome(&json!({"outcome": 5})), Some(&json!(5)));
        // a coexisting outcomes array wins, and no key means no legacy
        assert_eq!(legacy_outcome(&json!({"outcome": 1, "outcomes": []})), None);
        assert_eq!(legacy_outcome(&json!({"outcomes": []})), None);
        // non-objects never detect (the reference's `in` needs one)
        assert_eq!(legacy_outcome(&json!([1, 2])), None);
        assert_eq!(legacy_outcome(&json!("scalar")), None);
    }

    #[test]
    fn entry_extracts_present_keys_only_in_reference_order() {
        let entry = legacy_entry(&json!({
            "agent": "a", "junk": 9, "status": "s", "test_results": "t", "duration": 1
        }));
        assert_eq!(entry.keys().map(String::as_str).collect::<Vec<_>>(), [
            "status",
            "duration",
            "test_results",
            "agent"
        ]);
        assert_eq!(entry.get("junk"), None);
    }

    #[test]
    fn entry_empties_non_object_values() {
        // mulch-1609: the reference reads `.status` & co. as undefined
        // on primitives; JSON serialization drops undefined values
        for value in [json!(5), json!("x"), json!([1]), json!(true), json!({})] {
            assert!(legacy_entry(&value).is_empty(), "value {value} empties");
        }
    }

    #[test]
    fn normalization_moves_object_outcomes_to_a_trailing_array() {
        assert_eq!(
            normalized(r#"{"name":"p","outcome":{"agent":"a","status":"success"},"extra":1}"#)
                .to_string(),
            r#"{"name":"p","extra":1,"outcomes":[{"status":"success","agent":"a"}]}"#
        );
        // a mid-object outcome: every remaining key keeps its position
        // (plain remove would swap the last key into the freed slot)
        assert_eq!(
            normalized(
                r#"{"type":"pattern","outcome":{"status":"success"},"name":"mid","id":"mx-7"}"#
            )
            .to_string(),
            r#"{"type":"pattern","name":"mid","id":"mx-7","outcomes":[{"status":"success"}]}"#
        );
    }

    #[test]
    fn normalization_coerces_non_object_outcomes_to_an_empty_entry() {
        for line in [
            r#"{"name":"p","outcome":5}"#,
            r#"{"name":"p","outcome":"x"}"#,
            r#"{"name":"p","outcome":[1]}"#,
            r#"{"name":"p","outcome":{}}"#,
        ] {
            assert_eq!(
                normalized(line).to_string(),
                r#"{"name":"p","outcomes":[{}]}"#,
                "line {line}"
            );
        }
    }

    #[test]
    fn normalization_leaves_null_and_coexisting_outcomes_untouched() {
        assert_eq!(
            normalized(r#"{"name":"p","outcome":null}"#).to_string(),
            r#"{"name":"p","outcome":null}"#
        );
        assert_eq!(
            normalized(r#"{"name":"p","outcome":{"status":"s"},"outcomes":[]}"#).to_string(),
            r#"{"name":"p","outcome":{"status":"s"},"outcomes":[]}"#
        );
    }
}
