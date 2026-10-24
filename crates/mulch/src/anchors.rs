//! The record-anchor law (mulch-5a3d): which fields of a record
//! carry filesystem anchors, how each resolves against the project
//! root, and the validity census `prune --check-anchors` demotes on.
//!
//! One owner for a law the reference implements twice (prune.ts
//! `computeAnchorValidity` and doctor.ts `file-anchors`): the ordered
//! anchor list (`files[]`, `dir_anchors[]`, `evidence.file`), the
//! `resolve(projectRoot, p)` semantics (absolute anchors stay
//! absolute, everything else joins the project root), and existence
//! per anchor. Consumers keep their own rendering: prune demotes on
//! the fraction, doctor reports and strips per anchor.

use std::path::{Path, PathBuf};

use serde_json::Value;

/// The census over one record's anchors.
#[derive(Clone, Debug, PartialEq)]
pub struct AnchorReport {
    /// `valid / total`; meaningless at 0/0 (the consumer exempts a
    /// zero-anchor record before reading it).
    pub valid_fraction: f64,
    /// How many anchors resolve.
    pub valid:          usize,
    /// How many anchors the record carries.
    pub total:          usize,
    /// The broken anchors in record order: `(kind, path)` with the
    /// kind one of `file`, `dir`, `evidence_file`.
    pub broken:         Vec<(&'static str, String)>,
}

impl AnchorReport {
    /// The `--explain`/json row shape.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut row = serde_json::Map::new();
        row.insert(
            "valid_fraction".into(),
            crate::jsval::js_number(self.valid_fraction),
        );
        row.insert("valid".into(), Value::from(self.valid));
        row.insert("total".into(), Value::from(self.total));
        row.insert(
            "broken".into(),
            Value::Array(
                self.broken
                    .iter()
                    .map(|(kind, path)| {
                        let mut entry = serde_json::Map::new();
                        entry.insert("kind".into(), Value::String((*kind).into()));
                        entry.insert("path".into(), Value::String(path.clone()));
                        Value::Object(entry)
                    })
                    .collect(),
            ),
        );
        Value::Object(row)
    }
}

/// A record's anchors in the reference's report order: `files[]`
/// entries, `dir_anchors[]` entries, then `evidence.file`. Non-string
/// entries never participate; empty strings skip (prune's
/// `getRecordAnchors` law — a zero-anchor record is exempt from the
/// decay census).
#[must_use]
pub fn record_anchors(record: &Value) -> Vec<(&'static str, String)> {
    let mut anchors = Vec::new();
    for (field, kind) in [("files", "file"), ("dir_anchors", "dir")] {
        for entry in record
            .get(field)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(path) = entry.as_str()
                && !path.is_empty()
            {
                anchors.push((kind, path.to_string()));
            }
        }
    }
    if let Some(evidence) = record.get("evidence")
        && let Some(path) = evidence.get("file").and_then(Value::as_str)
        && !path.is_empty()
    {
        anchors.push(("evidence_file", path.to_string()));
    }
    anchors
}

/// Resolves an anchor like the reference's `resolve(projectRoot, p)`:
/// absolute anchors stay absolute, everything else joins the project
/// root.
#[must_use]
pub fn resolve(project_root: &Path, anchor: &str) -> PathBuf {
    let path = Path::new(anchor);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_root.join(path)
    }
}

/// `existsSync(resolve(projectRoot, path))`.
#[must_use]
pub fn exists(project_root: &Path, anchor: &str) -> bool {
    resolve(project_root, anchor).exists()
}

/// The anchor census against the project root: existence per anchor,
/// the valid fraction, and the broken list.
#[must_use]
pub fn validity(record: &Value, project_root: &Path) -> AnchorReport {
    let anchors = record_anchors(record);
    let total = anchors.len();
    if total == 0 {
        return AnchorReport {
            valid_fraction: 0.0,
            valid:          0,
            total:          0,
            broken:         Vec::new(),
        };
    }
    let mut valid = 0;
    let mut broken = Vec::new();
    for (kind, path) in anchors {
        if exists(project_root, &path) {
            valid += 1;
        } else {
            broken.push((kind, path));
        }
    }
    // Fraction as f64 division (the JSON carries the shortest float
    // the JS engine prints, e.g. 0.6666666666666666).
    AnchorReport {
        valid_fraction: valid as f64 / total as f64,
        valid,
        total,
        broken,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// A record with one anchor of every kind.
    fn anchored(files: &Value, dirs: &Value, evidence: &Value) -> Value {
        json!({
            "type": "pattern",
            "classification": "foundational",
            "recorded_at": "2026-10-01T00:00:00.000Z",
            "files": files.clone(),
            "dir_anchors": dirs.clone(),
            "evidence": evidence.clone(),
        })
    }

    #[test]
    fn record_anchors_walk_the_reference_order_and_skip_junk() {
        let record = anchored(
            &json!(["src/a.rs", "", 42, "src/b.rs"]),
            &json!(["docs", ""]),
            &json!({"file": "proof.txt"}),
        );
        let anchors = record_anchors(&record);
        let kinds: Vec<&str> = anchors.iter().map(|(kind, _)| *kind).collect();
        assert_eq!(kinds, ["file", "file", "dir", "evidence_file"]);
        let paths: Vec<&str> = anchors.iter().map(|(_, path)| path.as_str()).collect();
        assert_eq!(paths, ["src/a.rs", "src/b.rs", "docs", "proof.txt"]);
    }

    #[test]
    fn validity_counts_every_kind_and_exempt_zero_anchors() {
        let root = std::env::temp_dir();
        let record = anchored(
            &json!(["definitely-missing.rs"]),
            &json!([]),
            &json!({"file": "definitely-missing.txt"}),
        );
        let report = validity(&record, &root);
        assert_eq!(report.total, 2);
        assert_eq!(report.valid, 0);
        assert_eq!(report.broken.len(), 2);
        let kinds: Vec<&str> = report.broken.iter().map(|(kind, _)| *kind).collect();
        assert_eq!(kinds, ["file", "evidence_file"]);

        // a bare record carries no anchors: the census is 0/0 and the
        // consumer exempts it before reading the fraction
        let bare = json!({"type": "pattern"});
        let report = validity(&bare, &root);
        assert_eq!(report.total, 0);
        assert_eq!(report.valid_fraction, 0.0);
    }

    #[test]
    fn resolve_joins_relative_and_keeps_absolute() {
        let root = Path::new("/project");
        assert_eq!(resolve(root, "src/a.rs"), Path::new("/project/src/a.rs"));
        assert_eq!(resolve(root, "/etc/passwd"), Path::new("/etc/passwd"));
        assert!(exists(root, "/etc/passwd"));
    }
}
