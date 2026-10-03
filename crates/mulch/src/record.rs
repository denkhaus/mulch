//! Expertise records — the JSONL line format of `expertise/<domain>.jsonl`
//! and `archive/<domain>.jsonl`.
//!
//! A record is an ordered field list: the reference CLI (0.10.7) writes
//! JSON objects whose key order follows its per-type template, and the
//! ADR-0023 extension mechanism is *additive fields* — so the Rust core
//! keeps every field in file order, exposes typed accessors for the
//! known ones, and inserts new known fields at their canonical slot.

use serde_json::Value;

use crate::error::{Error, Result};

/// Field write order pinned from the reference writer (probe of
/// `ml record`/`ml outcome`/`ml archive` on 0.10.7, 2026-10-03).
///
/// New known fields are inserted before the first present field that
/// ranks later; fields that exist on disk keep their position.
const CANONICAL_ORDER: &[&str] = &[
    "type",
    "classification",
    "recorded_at",
    "name",
    "description",
    "title",
    "rationale",
    "evidence",
    "tags",
    "dir_anchors",
    "files",
    "content",
    "resolution",
    "id",
    "outcomes",
    "status",
    "archived_at",
    "archive_reason",
];

/// Fields every record must carry to be valid for the reference reader.
const REQUIRED_FIELDS: &[&str] = &["type", "classification", "recorded_at", "id"];

/// A validated record id (`mx-` + 6 lowercase hex digits).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RecordId([u8; 3]);

impl RecordId {
    /// Parse and validate a record id.
    pub fn parse(value: &str) -> Result<Self> {
        let hex = value
            .strip_prefix("mx-")
            .ok_or_else(|| Error::InvalidRecordId {
                value: value.to_owned(),
            })?;
        if hex.len() != 3 * 2
            || !hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(Error::InvalidRecordId {
                value: value.to_owned(),
            });
        }
        let mut bytes = [0u8; 3];
        for (slot, chunk) in bytes.iter_mut().zip(hex.as_bytes().chunks(2)) {
            *slot = u8::from_str_radix(
                std::str::from_utf8(chunk).expect("hex digits are ASCII"),
                16,
            )
            .expect("validated hex pair");
        }
        Ok(Self(bytes))
    }

    /// The canonical `mx-xxxxxx` string.
    pub fn as_str(&self) -> String {
        format!("mx-{:02x}{:02x}{:02x}", self.0[0], self.0[1], self.0[2])
    }
}

impl std::fmt::Display for RecordId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.as_str())
    }
}

/// One expertise record: ordered fields plus typed accessors.
///
/// Unknown fields (additive extensions, per ADR-0023) are first-class:
/// they are preserved verbatim through read-modify-write cycles.
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    fields: Vec<(String, Value)>,
}

impl Record {
    /// A new record with the three leading canonical fields.
    ///
    /// The `id` is added by [`Record::set_id`] (or stays absent while a
    /// caller generates one) so type-specific fields land before it.
    pub fn new(record_type: &str, classification: &str, recorded_at: &str) -> Self {
        let mut record = Self { fields: Vec::new() };
        record.set("type", Value::String(record_type.to_owned()));
        record.set("classification", Value::String(classification.to_owned()));
        record.set("recorded_at", Value::String(recorded_at.to_owned()));
        record
    }

    /// Parse one JSONL line (a compact JSON object).
    pub fn parse(line: &str) -> Result<Self> {
        let object: serde_json::Map<String, Value> =
            serde_json::from_str(line).map_err(|source| Error::RecordParse {
                path: std::path::PathBuf::from("expertise"),
                line: 0,
                source,
            })?;
        Ok(Self {
            fields: object.into_iter().collect(),
        })
    }

    /// Serialize as the compact JSONL line (reference `JSON.stringify`
    /// shape: `,`/`:` separators, no spaces).
    pub fn to_json_line(&self) -> String {
        let object: serde_json::Map<String, Value> = self
            .fields
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        serde_json::to_string(&Value::Object(object)).expect("JSON serialization is infallible")
    }

    /// The record type (`convention`, `pattern`, `failure`, `decision`,
    /// `reference`, `guide`, or a config-declared custom type).
    pub fn record_type(&self) -> Option<&str> {
        self.get("type").and_then(Value::as_str)
    }

    /// The classification tier (`foundational`, `tactical`, `observational`).
    pub fn classification(&self) -> Option<&str> {
        self.get("classification").and_then(Value::as_str)
    }

    /// The `recorded_at` timestamp.
    pub fn recorded_at(&self) -> Option<&str> {
        self.get("recorded_at").and_then(Value::as_str)
    }

    /// The validated record id.
    pub fn id(&self) -> Result<Option<RecordId>> {
        self.get("id")
            .and_then(Value::as_str)
            .map(RecordId::parse)
            .transpose()
    }

    /// Set the record id (canonical slot: after type-specific fields).
    pub fn set_id(&mut self, id: RecordId) {
        self.set("id", Value::String(id.as_str()));
    }

    /// A field's raw JSON value, if present.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.fields
            .iter()
            .find(|(field, _)| field == key)
            .map(|(_, value)| value)
    }

    /// Set a field: replaced in place when it exists, inserted at its
    /// canonical slot otherwise.
    pub fn set(&mut self, key: &str, value: Value) {
        if let Some(slot) = self.fields.iter_mut().find(|(field, _)| field == key) {
            slot.1 = value;
            return;
        }
        let rank = canonical_rank(key);
        let position = self
            .fields
            .iter()
            .position(|(field, _)| canonical_rank(field) > rank)
            .unwrap_or(self.fields.len());
        self.fields.insert(position, (key.to_owned(), value));
    }

    /// Append a field verbatim at the end (additive-field escape hatch
    /// for keys the core does not know).
    pub fn append(&mut self, key: &str, value: Value) {
        self.fields.retain(|(field, _)| field != key);
        self.fields.push((key.to_owned(), value));
    }

    /// Remove a field, if present.
    pub fn remove(&mut self, key: &str) {
        self.fields.retain(|(field, _)| field != key);
    }

    /// Field names in write order.
    pub fn field_names(&self) -> impl Iterator<Item = &str> {
        self.fields.iter().map(|(key, _)| key.as_str())
    }

    /// Validate the fields every reference-readable record must carry.
    pub fn ensure_required(&self) -> Result<()> {
        for field in REQUIRED_FIELDS {
            if self.get(field).is_none() {
                return Err(Error::MissingField {
                    path: std::path::PathBuf::from("expertise"),
                    line: 0,
                    field,
                });
            }
        }
        self.id()?.map(|_| ()).ok_or_else(|| Error::MissingField {
            path:  std::path::PathBuf::from("expertise"),
            line:  0,
            field: "id",
        })
    }
}

fn canonical_rank(key: &str) -> usize {
    CANONICAL_ORDER
        .iter()
        .position(|candidate| *candidate == key)
        .unwrap_or(CANONICAL_ORDER.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    const REFERENCE_FAILURE: &str = "{\"type\":\"failure\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T10:12:14.018Z\",\"description\":\"fail desc\",\"resolution\":\"fixed it\",\"id\":\"mx-7b33dd\"}";
    const REFERENCE_CONVENTION: &str = "{\"type\":\"convention\",\"classification\":\"foundational\",\"recorded_at\":\"2026-10-03T10:12:24.565Z\",\"evidence\":{\"commit\":\"abc123\"},\"tags\":[\"a\",\"b\"],\"dir_anchors\":[\"crates/x\"],\"content\":\"body text\",\"id\":\"mx-e4f59f\"}";

    #[test]
    fn new_records_match_reference_write_order() {
        let mut failure = Record::new("failure", "tactical", "2026-10-03T10:12:14.018Z");
        failure.set("description", Value::String("fail desc".into()));
        failure.set("resolution", Value::String("fixed it".into()));
        failure.set_id(RecordId::parse("mx-7b33dd").unwrap());
        assert_eq!(failure.to_json_line(), REFERENCE_FAILURE);

        let mut convention = Record::new("convention", "foundational", "2026-10-03T10:12:24.565Z");
        convention.set("evidence", serde_json::json!({ "commit": "abc123" }));
        convention.set("tags", serde_json::json!(["a", "b"]));
        convention.set("dir_anchors", serde_json::json!(["crates/x"]));
        convention.set("content", Value::String("body text".into()));
        convention.set_id(RecordId::parse("mx-e4f59f").unwrap());
        assert_eq!(convention.to_json_line(), REFERENCE_CONVENTION);
    }

    #[test]
    fn parses_reference_lines_and_preserves_field_order() {
        let record = Record::parse(REFERENCE_CONVENTION).expect("parses");
        assert_eq!(record.record_type(), Some("convention"));
        assert_eq!(
            record.id().unwrap().map(|id| id.as_str()),
            Some("mx-e4f59f".to_owned())
        );
        assert_eq!(record.field_names().collect::<Vec<_>>(), vec![
            "type",
            "classification",
            "recorded_at",
            "evidence",
            "tags",
            "dir_anchors",
            "content",
            "id"
        ]);
        assert_eq!(record.to_json_line(), REFERENCE_CONVENTION);
    }

    #[test]
    fn unknown_fields_survive_round_trip_verbatim() {
        let line = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"t\",\"name\":\"n\",\"description\":\"d\",\"id\":\"mx-41bad1\",\"fabricated_by\":\"mulch-rs\",\"deep\":{\"k\":[1,2]}}";
        let mut record = Record::parse(line).expect("parses");
        assert_eq!(record.to_json_line(), line);
        record.set("name", Value::String("renamed".into()));
        assert!(record.to_json_line().contains("\"name\":\"renamed\""));
        assert!(
            record
                .to_json_line()
                .contains("\"fabricated_by\":\"mulch-rs\"")
        );
    }

    #[test]
    fn outcomes_append_after_id_like_the_reference() {
        let mut record = Record::parse(REFERENCE_CONVENTION).expect("parses");
        record.set(
            "outcomes",
            serde_json::json!([{ "status": "success", "recorded_at": "t2", "duration": 1200 }]),
        );
        let rewritten = record.to_json_line();
        assert!(rewritten.contains("\"id\":\"mx-e4f59f\",\"outcomes\":["));
    }

    #[test]
    fn record_id_round_trips_and_rejects_bad_values() {
        assert_eq!(RecordId::parse("mx-7b33dd").unwrap().as_str(), "mx-7b33dd");
        for bad in ["7b33dd", "mx-7b33dd0", "mx-7B33DD", "mx-zz33dd", ""] {
            assert!(RecordId::parse(bad).is_err(), "{bad} should be rejected");
        }
    }
}
