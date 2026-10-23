//! Record-file semantics shared by the mutating commands: the strict
//! reader (reference `readExpertiseFile`), the compact re-serializing
//! writer (reference `writeExpertiseFile`), identifier resolution
//! (`resolveRecordId`) and the per-type summary
//! (`getRecordSummary` + `truncate`).

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::error::{Error, Result};
use crate::ids::{Registry, Row, SummaryLaw, SummaryPart, record_id};
use crate::jsval::{
    js_dedup_eq, js_key_sorted, js_truncate, parse_non_negative_number, truthy_str, value_text,
};

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
pub fn read_strict(
    registry: &crate::ids::Registry,
    path: &Path,
    allow_unknown: bool,
) -> Result<Vec<LineRecord>> {
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
        if !allow_unknown && !kind.is_empty() && !registry.is_known(kind) {
            return Err(Error::UnknownRecordType {
                path:        path.to_path_buf(),
                line:        index + 1,
                id:          record.get("id").and_then(Value::as_str).map(str::to_string),
                record_type: kind.to_string(),
            });
        }
        apply_aliases_for(registry, &mut record);
        records.push(LineRecord {
            line: index + 1,
            record,
        });
    }
    Ok(records)
}

/// Reference `applyAliases` (utils/expertise.ts): rewrites legacy
/// field names to their canonical names per the type's aliases map,
/// mutating the record. Idempotent. A canonical value that is present
/// and not `null`/`""` WINS and the legacy name just drops; a missing,
/// `null`, or empty-string canonical takes the legacy value. A new
/// canonical key lands at the END of the object (config-declared
/// aliases order); deleting the legacy key preserves the order of the
/// rest (mulch-b1cb).
pub fn apply_aliases(record: &mut Value, aliases: &[(String, Vec<String>)]) {
    let Some(object) = record.as_object_mut() else {
        return;
    };
    for (canonical, legacy_names) in aliases {
        for legacy in legacy_names {
            if !object.contains_key(legacy) {
                continue;
            }
            let canonical_missing_or_empty = match object.get(canonical) {
                None | Some(Value::Null) => true,
                Some(Value::String(text)) if text.is_empty() => true,
                _ => false,
            };
            if canonical_missing_or_empty {
                let value = object
                    .get(legacy)
                    .cloned()
                    .expect("contains_key checked above");
                object.insert(canonical.clone(), value);
            }
            object.shift_remove(legacy);
        }
    }
}

/// Applies a record type's aliases when the type is registered (the
/// read-seam half of the reference's `readExpertiseFile`: builtins and
/// unknown types pass through untouched).
pub fn apply_aliases_for(registry: &crate::ids::Registry, record: &mut Value) {
    let kind = record.get("type").and_then(Value::as_str).unwrap_or("");
    if let Some(row) = registry.row(kind) {
        apply_aliases(record, &row.aliases);
    }
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

pub fn find_duplicate<'a, I>(
    registry: &crate::ids::Registry,
    records: I,
    candidate: &Value,
) -> Option<usize>
where
    I: IntoIterator<Item = &'a Value>,
{
    let record_type = candidate.get("type").and_then(Value::as_str)?;
    let row = registry.row(record_type)?;
    // content_hash never matches; the row's dedup is None then
    // (mulch-b4bd).
    let key = row.dedup.as_deref()?;
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
pub fn upsert_plan(
    registry: &crate::ids::Registry,
    working: &[Value],
    candidate: &Value,
    force: bool,
) -> UpsertPlan {
    if !force && let Some(index) = find_duplicate(registry, working, candidate) {
        let named = registry
            .row(
                candidate
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )
            .is_some_and(|row| row.named);
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
pub fn write_records(
    registry: &crate::ids::Registry,
    path: &Path,
    records: &[Value],
) -> Result<()> {
    let mut body = String::new();
    for record in records {
        let mut record = record.clone();
        assign_missing_id(registry, &mut record);
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
pub fn assign_missing_id(registry: &crate::ids::Registry, record: &mut Value) {
    let Some(object) = record.as_object_mut() else {
        return;
    };
    if object.get("id").and_then(Value::as_str).is_some() {
        return;
    }
    let kind = crate::ids::effective_type(object.get("type").and_then(Value::as_str)).to_string();
    let key_field = registry.id_key_field(&kind);
    let key_value = object
        .get(key_field)
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

/// The truncating summary laws' failure: the reference's TypeError
/// (`text.length` on a missing or non-string required field — JSC
/// renders the same message for `undefined` and for non-strings).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SummaryNotAString;

impl std::fmt::Display for SummaryNotAString {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "undefined is not an object (evaluating 'text.length')"
        )
    }
}

/// The registry summary as the reference's raw JS value
/// (`getRecordSummary` without the string coercion):
/// `Ok(None)` is `undefined` — plain output renders the word, JSON
/// output omits the key — non-string values pass through verbatim,
/// and the truncating laws (`convention`/`failure`) fail with their
/// TypeError when the field is missing or not a string.
///
/// [`record_summary`] stays the string-coercing view for the call
/// sites that render today's shapes.
pub fn record_summary_value(
    registry: &Registry,
    record: &Value,
) -> std::result::Result<Option<Value>, SummaryNotAString> {
    let kind = crate::ids::effective_type(record.get("type").and_then(Value::as_str));
    match registry.row(kind) {
        Some(Row {
            summary: SummaryLaw::Template(parts),
            ..
        }) => {
            let text = parts
                .iter()
                .map(|part| match part {
                    SummaryPart::Literal(text) => text.clone(),
                    SummaryPart::Field(field) => {
                        record
                            .get(field)
                            .map_or(String::new(), |value| match value {
                                Value::String(text) => text.clone(),
                                other => crate::jsval::value_text(other),
                            })
                    }
                })
                .collect();
            Ok(Some(Value::String(text)))
        }
        Some(Row {
            summary:
                SummaryLaw::Field {
                    field,
                    truncate: shorten,
                },
            ..
        }) => match (record.get(field), *shorten) {
            (None, false) => Ok(None),
            (Some(value), false) => Ok(Some(value.clone())),
            (Some(Value::String(text)), true) => Ok(Some(Value::String(js_truncate(text, 60)))),
            (_, true) => Err(SummaryNotAString),
        },
        // Unknown types keep the content fallback string (the
        // throw-on-unknown surface belongs to the commands).
        None => Ok(Some(Value::String(
            record
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        ))),
    }
}

/// Reference per-type summary (`getRecordSummary`).
pub fn record_summary(registry: &Registry, record: &Value) -> String {
    let kind = crate::ids::effective_type(record.get("type").and_then(Value::as_str));
    match registry.row(kind) {
        Some(Row {
            summary: SummaryLaw::Template(parts),
            ..
        }) => parts
            .iter()
            .map(|part| match part {
                SummaryPart::Literal(text) => text.clone(),
                SummaryPart::Field(field) => {
                    record
                        .get(field)
                        .map_or(String::new(), |value| match value {
                            Value::String(text) => text.clone(),
                            other => crate::jsval::value_text(other),
                        })
                }
            })
            .collect(),
        Some(Row {
            summary:
                SummaryLaw::Field {
                    field,
                    truncate: shorten,
                },
            ..
        }) => {
            let value = record
                .get(field)
                .and_then(Value::as_str)
                .unwrap_or_default();
            if *shorten {
                js_truncate(value, 60)
            } else {
                value.to_string()
            }
        }
        // Unknown types key on content like every static fallback —
        // and never truncate: `convention`/`failure` (the only
        // truncate cases) are always registered rows, so the arm's
        // truncate branch was dead (move/delete reach unknown types
        // only under --allow-unknown-types; mulch-b9d4).
        None => record
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    }
}

/// An ordered distribution counter with the reference's JS semantics
/// at the JSON boundary: seeded keys first, extras in first-seen
/// order; a missing/non-string field counts under the literal key
/// `"undefined"` (moved from the status command, mulch-8ba4 — prime's
/// manifest needs the same law; `computeTypeCounts`).
#[derive(Debug)]
pub struct Counter {
    counts: Vec<(String, Option<u64>)>,
}

impl Counter {
    pub fn seeded(keys: &[&str]) -> Self {
        Self {
            counts: keys.iter().map(|k| ((*k).to_owned(), Some(0))).collect(),
        }
    }

    /// Counts the field under its JS property key (`String(key)`
    /// coercion — `5` becomes `"5"`, `null` becomes `"null"`,
    /// objects `[object Object]`; a missing field is `"undefined"`).
    pub fn bump_field(&mut self, field: Option<&Value>) {
        let key = field.map_or_else(|| "undefined".into(), value_text);
        self.bump_key(&key, false);
    }

    /// Counts the field only when its key is one of `known`; anything
    /// else lands under its key as `None` (the reference's counter
    /// goes NaN there, JSON `null`).
    pub fn bump_known_field(&mut self, field: Option<&Value>, known: &[&str]) {
        let key = field.map_or_else(|| "undefined".into(), value_text);
        self.bump_key(&key, !known.contains(&key.as_str()));
    }

    fn bump_key(&mut self, key: &str, force_null: bool) {
        if let Some(slot) = self.counts.iter_mut().find(|(k, _)| k == key) {
            if force_null {
                slot.1 = None;
            } else {
                slot.1 = Some(slot.1.unwrap_or(0) + 1);
            }
            return;
        }
        let value = if force_null { None } else { Some(1) };
        self.counts.push((key.to_owned(), value));
    }

    /// The JSON object: counts, `null` for the NaN-emulating slots.
    /// Key order follows JS object enumeration: integer-like keys
    /// ascending first, then string keys in insertion order (probe:
    /// `{"type": 5}` yields `"5"` before the seeded names).
    pub fn to_distribution(&self) -> Value {
        // JS object enumeration order is the jsval seam's law
        // (integer-like keys ascending first, then first-seen order).
        let mut map = Map::new();
        for (key, count) in js_key_sorted(&self.counts) {
            let value = match count {
                Some(count) => Value::from(*count),
                None => Value::Null,
            };
            map.insert(key.clone(), value);
        }
        Value::Object(map)
    }
}

#[cfg(test)]
mod tests {

    /// The builtins-only registry for the static-law pins below
    /// (custom-types law pins live in custom_types.rs).
    fn registry() -> crate::ids::Registry {
        crate::ids::Registry::builtins()
    }

    use serde_json::json;

    use super::*;

    #[test]
    fn apply_aliases_moves_legacy_to_canonical_at_object_end() {
        let aliases = vec![
            ("code".to_string(), vec!["snippet_code".to_string()]),
            ("name".to_string(), vec!["title2".to_string()]),
        ];
        let mut record = json!({
            "type": "snippet",
            "recorded_at": "2026-10-10T10:00:00Z",
            "snippet_code": "SC1",
            "title2": "Legacy One"
        });
        apply_aliases(&mut record, &aliases);
        // New canonical keys land at the END, in aliases-entry order;
        // the legacy keys are gone and the rest keeps its order.
        assert_eq!(
            record,
            json!({
                "type": "snippet",
                "recorded_at": "2026-10-10T10:00:00Z",
                "code": "SC1",
                "name": "Legacy One"
            })
        );
        let keys: Vec<&str> = record
            .as_object()
            .expect("canonicalized object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, ["type", "recorded_at", "code", "name"]);
        // Idempotent: a second pass is a no-op.
        let again = record.clone();
        apply_aliases(&mut record, &aliases);
        assert_eq!(record, again);
    }

    #[test]
    fn apply_aliases_canonical_wins_over_legacy() {
        let aliases = vec![("code".to_string(), vec!["snippet_code".to_string()])];
        // Every non-empty canonical value wins — including JS-falsy
        // ones (0, false, [], {}): only missing/null/"" count as
        // empty in the reference (`raw[canonical] === undefined ||
        // === null || === ""`).
        for canonical in [json!("C2"), json!(0), json!(false), json!([]), json!({})] {
            let mut record = json!({
                "code": canonical.clone(),
                "snippet_code": "SHOULD_LOSE"
            });
            apply_aliases(&mut record, &aliases);
            assert_eq!(record.get("code"), Some(&canonical), "{canonical:?}");
            assert!(record.get("snippet_code").is_none(), "{canonical:?}");
        }
    }

    #[test]
    fn apply_aliases_legacy_wins_on_missing_null_or_empty_canonical() {
        let aliases = vec![("code".to_string(), vec!["snippet_code".to_string()])];
        // Missing, null, and "" canonical all take the legacy value.
        let mut missing = json!({"snippet_code": "SC1"});
        apply_aliases(&mut missing, &aliases);
        assert_eq!(missing, json!({"code": "SC1"}));
        let mut null = json!({"code": null, "snippet_code": "SC1"});
        apply_aliases(&mut null, &aliases);
        assert_eq!(null, json!({"code": "SC1"}));
        let mut empty = json!({"code": "", "snippet_code": "SC1"});
        apply_aliases(&mut empty, &aliases);
        assert_eq!(empty, json!({"code": "SC1"}));
        // A null legacy value moves as null.
        let mut null_legacy = json!({"snippet_code": null});
        apply_aliases(&mut null_legacy, &aliases);
        assert_eq!(null_legacy, json!({"code": null}));
        // A missing canonical key present nowhere: absent legacy keys
        // are skipped entirely.
        let mut untouched = json!({"type": "pattern"});
        apply_aliases(&mut untouched, &aliases);
        assert_eq!(untouched, json!({"type": "pattern"}));
    }

    #[test]
    fn apply_aliases_delete_preserves_surrounding_key_order() {
        let aliases = vec![("code".to_string(), vec!["mid".to_string()])];
        let mut record = json!({
            "type": "snippet",
            "mid": "SC1",
            "name": "keeps position",
            "extra": "also keeps"
        });
        apply_aliases(&mut record, &aliases);
        let keys: Vec<&str> = record
            .as_object()
            .expect("canonicalized object")
            .keys()
            .map(String::as_str)
            .collect();
        // "mid" deleted in place (shift_remove), "code" appended at
        // the end — the survivors keep their relative order.
        assert_eq!(keys, ["type", "name", "extra", "code"]);
    }

    #[test]
    fn find_duplicate_matches_on_the_dedup_field_not_the_id() {
        let store = [
            json!({"type": "pattern", "name": "p", "id": "mx-old"}),
            json!({"type": "pattern", "name": "q", "id": "mx-q"}),
        ];
        // same name, different id: still the duplicate
        assert_eq!(
            find_duplicate(
                &registry(),
                store.iter(),
                &json!({"type": "pattern", "name": "p"})
            ),
            Some(0)
        );
        assert_eq!(
            find_duplicate(
                &registry(),
                store.iter(),
                &json!({"type": "pattern", "name": "zz"})
            ),
            None
        );
        // same dedup VALUE on another type never matches
        assert_eq!(
            find_duplicate(
                &registry(),
                store.iter(),
                &json!({"type": "guide", "name": "p"})
            ),
            None
        );
    }

    #[test]
    fn find_duplicate_matches_missing_fields_and_skips_unregistered_types() {
        let bare = [json!({"type": "failure", "resolution": "r"})];
        // both sides missing the dedup field match (undefined === undefined)
        assert_eq!(
            find_duplicate(
                &registry(),
                bare.iter(),
                &json!({"type": "failure", "resolution": "r2"})
            ),
            Some(0)
        );
        let store = [json!({"type": "failure", "description": "d"})];
        // one side missing does NOT match a present value
        assert_eq!(
            find_duplicate(
                &registry(),
                store.iter(),
                &json!({"type": "failure", "resolution": "r"})
            ),
            None
        );
        // unregistered types never duplicate (no registry definition)
        assert_eq!(
            find_duplicate(
                &registry(),
                store.iter(),
                &json!({"type": "custom", "description": "d"})
            ),
            None
        );
        // a record without a type never duplicates
        assert_eq!(
            find_duplicate(&registry(), store.iter(), &json!({"name": "p"})),
            None
        );
    }

    #[test]
    fn find_duplicate_follows_js_strict_equality() {
        // numbers compare as f64: 1 matches 1.0 (same JS number)
        let store = [json!({"type": "pattern", "name": 1})];
        assert_eq!(
            find_duplicate(
                &registry(),
                store.iter(),
                &json!({"type": "pattern", "name": 1.0})
            ),
            Some(0)
        );
        // like-typed only: "1" never matches 1
        assert_eq!(
            find_duplicate(
                &registry(),
                store.iter(),
                &json!({"type": "pattern", "name": "1"})
            ),
            None
        );
        // null only matches null, never a present value
        let nulls = [json!({"type": "pattern", "name": null})];
        assert_eq!(
            find_duplicate(
                &registry(),
                nulls.iter(),
                &json!({"type": "pattern", "name": null})
            ),
            Some(0)
        );
        assert_eq!(
            find_duplicate(&registry(), nulls.iter(), &json!({"type": "pattern"})),
            None
        );
        // objects/arrays never match (JS === compares identity)
        let objects = [json!({"type": "pattern", "name": {"a": 1}})];
        assert_eq!(
            find_duplicate(
                &registry(),
                objects.iter(),
                &json!({"type": "pattern", "name": {"a": 1}})
            ),
            None
        );
        let arrays = [json!({"type": "pattern", "name": [1]})];
        assert_eq!(
            find_duplicate(
                &registry(),
                arrays.iter(),
                &json!({"type": "pattern", "name": [1]})
            ),
            None
        );
    }
}

#[cfg(test)]
mod upsert_plan_tests {

    fn registry() -> crate::ids::Registry {
        crate::ids::Registry::builtins()
    }
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
        match upsert_plan(&registry(), &working, &incoming, false) {
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
            &registry(),
            &working,
            &json!({"type": "convention", "content": "c"}),
            false,
        ) {
            UpsertPlan::Skip { index } => assert_eq!(index, 1),
            other => panic!("expected Skip, got {other:?}"),
        }
        // force appends even on a duplicate
        assert!(matches!(
            upsert_plan(
                &registry(),
                &working,
                &json!({"type": "pattern", "name": "p"}),
                true
            ),
            UpsertPlan::Create
        ));
        // no match: Create
        assert!(matches!(
            upsert_plan(
                &registry(),
                &working,
                &json!({"type": "pattern", "name": "q"}),
                false
            ),
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

    fn registry() -> crate::ids::Registry {
        crate::ids::Registry::builtins()
    }
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
        let records = read_strict(&registry(), &path, false).expect("arrays are records");
        assert_eq!(records.len(), 1);

        std::fs::write(&path, "5\n").expect("writable");
        let error = read_strict(&registry(), &path, false).expect_err("scalars are errors");
        assert!(matches!(error, Error::NotAnObject { line: 1, .. }));

        std::fs::write(&path, "null\n").expect("writable");
        assert!(matches!(
            read_strict(&registry(), &path, false),
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
