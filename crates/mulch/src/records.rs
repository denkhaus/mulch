//! Record-file semantics shared by the mutating commands: the strict
//! reader (reference `readExpertiseFile`), the compact re-serializing
//! writer (reference `writeExpertiseFile`), identifier resolution
//! (`resolveRecordId`) and the per-type summary
//! (`getRecordSummary` + `truncate`).

use std::path::{Path, PathBuf};

use serde_json::Value;

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
        /// How many matched.
        count: usize,
        /// The matching ids.
        ids:   Vec<String>,
    },
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
            count: many.len(),
            ids:   many
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
