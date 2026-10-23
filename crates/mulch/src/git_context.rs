//! The pure file-matching laws behind git-context scoping — the
//! reference's `utils/dir-anchors.ts` (`normalizeDirAnchor`,
//! `fileLivesUnderDir`), `utils/git.ts` (`fileMatchesAny`,
//! `filterByContext`) and `commands/learn.ts`
//! (`matchFilesToDomains`). The git PROCESS side (running `git
//! diff`) stays CLI-side; the lib owns the matching law so
//! `record`'s auto-files, `prime --context` and `learn` share one
//! seam.
//!
//! JS semantics pinned here:
//! - Matching is suffix-based BOTH directions: `a === b || a.endsWith(b) ||
//!   b.endsWith(a)`.
//! - `normalizeDirAnchor` trims, maps backslashes to `/`, strips trailing
//!   slashes and a leading `./`; `""` and `"."` mean the root (everything lives
//!   under it).
//! - `filterByContext`: records with NO `files` and NO `dir_anchors` are always
//!   relevant; otherwise any file match or any dir anchor containing a changed
//!   file wins. Both fields are read literally (not through the registry).
//! - `matchFilesToDomains` iterates domains in CONFIG order, reads each
//!   strictly (`readExpertiseFile`: malformed lines and unregistered types are
//!   errors unless bypassed), collects files only from types whose registry row
//!   extracts files, and reports matched changed files in CHANGED order; a
//!   changed file matches the first domain file that satisfies the suffix law.

use serde_json::Value;

use crate::ids::Registry;
use crate::store_files::StoreFiles;

/// One domain's matched changed files.
#[derive(Clone, Debug, PartialEq)]
pub struct DomainMatch {
    pub domain:        String,
    pub matched_files: Vec<String>,
}

/// `matchFilesToDomains`' result: per-domain matches (config
/// order) and the changed files no domain claimed (changed order).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DomainFileMatches {
    pub matches:   Vec<DomainMatch>,
    pub unmatched: Vec<String>,
}

/// `normalizeDirAnchor`: the canonical dir-anchor form.
pub fn normalize_dir_anchor(input: &str) -> String {
    let value = input.trim();
    if value.is_empty() || value == "." {
        return String::new();
    }
    let normalized = value.replace('\\', "/");
    let mut normalized = normalized.trim_end_matches('/').to_string();
    if let Some(stripped) = normalized.strip_prefix("./") {
        normalized = stripped.to_string();
    }
    let normalized = normalized.trim_end_matches('/');
    if normalized.is_empty() || normalized == "." {
        String::new()
    } else {
        normalized.to_string()
    }
}

/// `fileLivesUnderDir`: root anchors match everything.
pub fn file_lives_under_dir(file: &str, dir: &str) -> bool {
    let file = file.replace('\\', "/");
    let dir = normalize_dir_anchor(dir);
    if dir.is_empty() {
        return true;
    }
    file == dir || file.starts_with(&format!("{dir}/"))
}

/// `fileMatchesAny`: exact or either-direction suffix match.
pub fn file_matches_any(file: &str, changed_files: &[String]) -> bool {
    changed_files
        .iter()
        .any(|changed| changed == file || changed.ends_with(file) || file.ends_with(changed))
}

/// `filterByContext`: anchorless records stay; anchored records
/// need one match.
pub fn filter_by_context(records: &[Value], changed_files: &[String]) -> Vec<Value> {
    records
        .iter()
        .filter(|record| {
            let files = record.get("files").and_then(Value::as_array);
            let anchors = record.get("dir_anchors").and_then(Value::as_array);
            let has_files = files.is_some_and(|f| !f.is_empty());
            let has_anchors = anchors.is_some_and(|a| !a.is_empty());
            if !has_files && !has_anchors {
                return true;
            }
            if has_files
                && files.is_some_and(|files| {
                    files
                        .iter()
                        .filter_map(Value::as_str)
                        .any(|file| file_matches_any(file, changed_files))
                })
            {
                return true;
            }
            if has_anchors
                && anchors.is_some_and(|anchors| {
                    anchors.iter().filter_map(Value::as_str).any(|anchor| {
                        changed_files
                            .iter()
                            .any(|changed| file_lives_under_dir(changed, anchor))
                    })
                })
            {
                return true;
            }
            false
        })
        .cloned()
        .collect()
}

/// `matchFilesToDomains` over the store: config-order domains,
/// strict reads (`readExpertiseFile`: malformed lines and
/// unregistered types fail unless `allow_unknown`), registry-driven
/// file extraction.
///
/// # Errors
///
/// The strict read's [`crate::Error`] (malformed line, unknown type).
pub fn match_files_to_domains(
    store: &StoreFiles,
    changed_files: &[String],
    allow_unknown: bool,
) -> Result<DomainFileMatches, crate::Error> {
    let mut out = DomainFileMatches::default();
    let mut matched: std::collections::HashSet<&String> = std::collections::HashSet::new();

    for domain in store.domains() {
        let domain_files = domain_file_set(store, &domain, allow_unknown)?;
        let mut domain_matched = Vec::new();
        for changed in changed_files {
            let hits = domain_files.iter().any(|domain_file| {
                changed == domain_file
                    || changed.ends_with(domain_file)
                    || domain_file.ends_with(changed)
            });
            if hits {
                domain_matched.push(changed.clone());
                matched.insert(changed);
            }
        }
        if !domain_matched.is_empty() {
            out.matches.push(DomainMatch {
                domain,
                matched_files: domain_matched,
            });
        }
    }
    out.unmatched = changed_files
        .iter()
        .filter(|changed| !matched.contains(changed))
        .cloned()
        .collect();
    Ok(out)
}

/// The distinct file paths a domain's records declare, from types
/// whose registry row extracts files (`def.filesField`).
fn domain_file_set(
    store: &StoreFiles,
    domain: &str,
    allow_unknown: bool,
) -> Result<Vec<String>, crate::Error> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut files = Vec::new();
    for line in store.read_records(domain, allow_unknown)? {
        collect_registry_files(store.registry(), &line.record, |file| {
            if seen.insert(file.to_string()) {
                files.push(file.to_string());
            }
        });
    }
    Ok(files)
}

/// Reads a record's files through its registry row: the type must
/// extract files and the field must be an array of strings.
fn collect_registry_files(registry: &Registry, record: &Value, mut sink: impl FnMut(&str)) {
    let Some(type_) = record.get("type").and_then(Value::as_str) else {
        return;
    };
    let Some(row) = registry.row(type_) else {
        return;
    };
    let Some(files_field) = row.files_array.as_deref() else {
        return;
    };
    let Some(items) = record.get(files_field).and_then(Value::as_array) else {
        return;
    };
    for item in items.iter().filter_map(Value::as_str) {
        sink(item);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// Anchor normalization: trim, slashes, trailing, `./`.
    #[test]
    fn normalize_dir_anchor_canonicalizes() {
        assert_eq!(normalize_dir_anchor(""), "");
        assert_eq!(normalize_dir_anchor("."), "");
        assert_eq!(normalize_dir_anchor("src/"), "src");
        assert_eq!(normalize_dir_anchor("./src/deep//"), "src/deep");
        assert_eq!(normalize_dir_anchor(" src\\deep "), "src/deep");
        assert_eq!(normalize_dir_anchor("src/./"), "src/.");
    }

    /// The root anchor matches everything; deeper anchors need the
    /// prefix.
    #[test]
    fn lives_under_dir_law() {
        assert!(file_lives_under_dir("src/a.rs", ""));
        assert!(file_lives_under_dir("src/a.rs", "src"));
        assert!(file_lives_under_dir("src/a.rs", "src/"));
        assert!(file_lives_under_dir("src/a.rs", "./src"));
        assert!(!file_lives_under_dir("src2/a.rs", "src"));
        assert!(file_lives_under_dir("src\\a.rs", "src"));
    }

    /// Suffix matching works in both directions.
    #[test]
    fn matches_any_law() {
        let changed = vec!["crates/src/a.rs".to_string()];
        assert!(file_matches_any("src/a.rs", &changed));
        assert!(file_matches_any("crates/src/a.rs", &changed));
        // "crates/src/a.rs/extra" ends with neither — no match (the
        // suffix law is directional equality, not prefix).
        assert!(!file_matches_any("crates/src/a.rs/extra", &changed));
        assert!(!file_matches_any("other.rs", &changed));
    }

    /// Anchorless records always pass; anchored records need a
    /// match; empty arrays count as absent.
    #[test]
    fn filter_by_context_law() {
        let records = vec![
            json!({"type": "convention", "content": "no anchors"}),
            json!({"type": "pattern", "files": ["src/a.rs"]}),
            json!({"type": "pattern", "files": ["other.rs"]}),
            json!({"type": "pattern", "files": []}),
            json!({"type": "pattern", "dir_anchors": ["src"]}),
        ];
        let changed = vec!["src/a.rs".to_string()];
        let kept: Vec<Value> = filter_by_context(&records, &changed);
        assert_eq!(kept.len(), 4);
        assert!(kept.iter().all(|r| r["files"] != json!(["other.rs"])));
    }
}
