//! BM25 search over expertise records — the reference's
//! `utils/bm25.ts` (`searchBM25` + `tokenize` +
//! `extractRecordText`), byte-relevant law. Pure: the CLI composes
//! this with [`crate::query::RecordFilter`] and renders; the lib
//! owns the score.
//!
//! JS semantics pinned here:
//! - `tokenize` lowercases (Unicode), replaces `[^\w\s-]` with spaces (`\w` is
//!   ASCII-only in JS regexes without `u`; the whitespace class is the
//!   ECMAScript `\s` set — U+FEFF counts, U+0085 does not), splits on
//!   whitespace runs.
//! - `extractRecordText` switches on the builtin type only — custom types
//!   contribute NOTHING but their `tags` (no case in the reference switch).
//! - Field texts join string array items with `" "`; blank strings (JS `trim`)
//!   never contribute.
//! - `searchBM25` keeps `score > 0` results only, sorts score descending STABLY
//!   (ties keep corpus order), and reports `matched_fields` in the per-type
//!   field order.

use serde_json::Value;

use crate::jsval;

/// Default BM25 parameters (reference `DEFAULT_BM25_PARAMS`).
pub const DEFAULT_BM25_PARAMS: Bm25Params = Bm25Params { k1: 1.5, b: 0.75 };

/// BM25 tuning parameters (`k1`: term-frequency saturation, `b`:
/// length normalization).
#[derive(Clone, Copy, Debug)]
pub struct Bm25Params {
    pub k1: f64,
    pub b:  f64,
}

/// One scored hit: the record, its BM25 score, and the fields whose
/// text contained a query token (reference field order).
#[derive(Clone, Debug)]
pub struct Bm25Result {
    pub record:         Value,
    pub score:          f64,
    pub matched_fields: Vec<String>,
}

/// The searchable text of a record: the per-field texts in the
/// reference's `addField` order plus the joined `all_text`.
#[derive(Clone, Debug, Default)]
pub struct RecordText {
    pub all_text:    String,
    pub field_texts: Vec<(String, String)>,
}

/// `tokenize`: lowercase, strip punctuation to spaces, split.
pub fn tokenize(text: &str) -> Vec<String> {
    let lowered = text.to_lowercase();
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in lowered.chars() {
        if is_word_char(ch) || ch == '-' {
            current.push(ch);
        } else if is_ecmascript_whitespace(ch) {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        } else {
            // `[^\w\s-]` — any other char becomes a separator.
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn is_word_char(ch: char) -> bool {
    // JS `\w` without the `u` flag: `[A-Za-z0-9_]` only.
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn is_ecmascript_whitespace(ch: char) -> bool {
    jsval::is_ecmascript_whitespace(ch)
}

fn trimmed_nonempty(text: &str) -> bool {
    // JS `String.prototype.trim` uses the ECMAScript whitespace set.
    !text.chars().all(is_ecmascript_whitespace)
}

/// `extractRecordText`: builtin-type field extraction in the
/// reference's per-type order, `tags` last; custom types carry
/// their `tags` only.
pub fn extract_record_text(record: &Value) -> RecordText {
    let mut out = RecordText::default();
    let obj = record.as_object();
    let type_ = obj.and_then(|o| o.get("type")).and_then(Value::as_str);
    let get = |name: &str| obj.and_then(|o| o.get(name));

    let add_field = |out: &mut RecordText, name: &str, value: Option<&Value>| {
        let Some(value) = value else { return };
        let text = match value {
            Value::String(s) => {
                if !trimmed_nonempty(s) {
                    return;
                }
                s.clone()
            }
            Value::Array(items) => {
                let joined = items
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" ");
                if !trimmed_nonempty(&joined) {
                    return;
                }
                joined
            }
            _ => return,
        };
        out.field_texts.push((name.to_string(), text.clone()));
        if !out.all_text.is_empty() {
            out.all_text.push(' ');
        }
        out.all_text.push_str(&text);
    };

    match type_ {
        Some("pattern" | "reference") => {
            add_field(&mut out, "name", get("name"));
            add_field(&mut out, "description", get("description"));
            add_field(&mut out, "files", get("files"));
        }
        Some("convention") => {
            add_field(&mut out, "content", get("content"));
        }
        Some("failure") => {
            add_field(&mut out, "description", get("description"));
            add_field(&mut out, "resolution", get("resolution"));
        }
        Some("decision") => {
            add_field(&mut out, "title", get("title"));
            add_field(&mut out, "rationale", get("rationale"));
        }
        Some("guide") => {
            add_field(&mut out, "name", get("name"));
            add_field(&mut out, "description", get("description"));
        }
        // Custom (and unknown) types: no case in the reference
        // switch — only the common `tags` below.
        _ => {}
    }
    add_field(&mut out, "tags", get("tags"));
    out
}

/// `searchBM25`: score every record against the query, keep
/// `score > 0`, sort descending (stable).
pub fn search_bm25(records: &[Value], query: &str, params: Bm25Params) -> Vec<Bm25Result> {
    if records.is_empty() || !trimmed_nonempty(query) {
        return Vec::new();
    }
    let query_tokens = tokenize(query);
    if query_tokens.is_empty() {
        return Vec::new();
    }

    let docs: Vec<(Value, RecordText, Vec<String>)> = records
        .iter()
        .map(|record| {
            let text = extract_record_text(record);
            let tokens = tokenize(&text.all_text);
            (record.clone(), text, tokens)
        })
        .collect();

    let total_length: usize = docs.iter().map(|(_, _, tokens)| tokens.len()).sum();
    let avg_doc_length = total_length as f64 / docs.len() as f64;

    // IDF per term present in the corpus (unique terms per doc).
    let mut doc_freq: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (_, _, tokens) in &docs {
        let mut unique = tokens.clone();
        unique.sort();
        unique.dedup();
        for term in unique {
            *doc_freq.entry(term).or_default() += 1;
        }
    }
    let doc_count = docs.len() as f64;
    let idf: std::collections::HashMap<String, f64> = doc_freq
        .into_iter()
        .map(|(term, freq)| {
            let score = ((doc_count - freq as f64 + 0.5) / (freq as f64 + 0.5) + 1.0).ln();
            (term, score)
        })
        .collect();

    let mut results = Vec::new();
    for (record, text, tokens) in &docs {
        let mut term_freq: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::new();
        for token in tokens {
            *term_freq.entry(token.as_str()).or_default() += 1;
        }
        let doc_length = tokens.len() as f64;
        let mut score = 0.0_f64;
        for query_term in &query_tokens {
            let freq = term_freq.get(query_term.as_str()).copied().unwrap_or(0) as f64;
            let term_idf = idf.get(query_term).copied().unwrap_or(0.0);
            let numerator = freq * (params.k1 + 1.0);
            let denominator =
                freq + params.k1 * (1.0 - params.b + params.b * (doc_length / avg_doc_length));
            score += term_idf * (numerator / denominator);
        }
        if score > 0.0 {
            let matched_fields = text
                .field_texts
                .iter()
                .filter(|(_, field_text)| {
                    let field_tokens = tokenize(field_text);
                    query_tokens.iter().any(|qt| field_tokens.contains(qt))
                })
                .map(|(name, _)| name.clone())
                .collect();
            results.push(Bm25Result {
                record: record.clone(),
                score,
                matched_fields,
            });
        }
    }

    // Stable descending sort: ties keep corpus order (JS sort is
    // stable; sort_by is stable).
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// The JS tokenizer laws: punctuation splits, case folds,
    /// hyphens stay, whitespace (incl. FEFF) splits.
    #[test]
    fn tokenize_follows_the_js_law() {
        assert_eq!(tokenize("Hello, World!"), ["hello", "world"]);
        assert_eq!(tokenize("foo-bar baz"), ["foo-bar", "baz"]);
        assert_eq!(tokenize("  \t\n "), Vec::<String>::new());
        // U+FEFF is ECMAScript whitespace; U+0085 is NOT (it becomes
        // a separator through the punctuation branch).
        assert_eq!(tokenize("a\u{FEFF}b"), ["a", "b"]);
        assert_eq!(tokenize("a\u{85}b"), ["a", "b"]);
        // Non-ASCII letters are punctuation to the JS class.
        assert_eq!(tokenize("café ★"), ["caf"]);
        assert_eq!(tokenize("snake_case kebab-case"), [
            "snake_case",
            "kebab-case"
        ]);
    }

    /// Field extraction: builtin order, tags last, custom types
    /// carry tags only, blanks skip.
    #[test]
    fn extract_record_text_follows_the_field_law() {
        let pattern = json!({
            "type": "pattern",
            "name": "one",
            "description": "   ",
            "files": ["a.rs", "b.rs", 5],
            "tags": ["t1"]
        });
        let text = extract_record_text(&pattern);
        assert_eq!(text.field_texts, [
            ("name".to_string(), "one".to_string()),
            ("files".to_string(), "a.rs b.rs".to_string()),
            ("tags".to_string(), "t1".to_string())
        ]);
        assert_eq!(text.all_text, "one a.rs b.rs t1");

        let custom = json!({"type": "recipe", "name": "x", "tags": []});
        assert_eq!(extract_record_text(&custom).field_texts, []);

        let convention = json!({"type": "convention", "content": "law"});
        assert_eq!(extract_record_text(&convention).field_texts, [(
            "content".to_string(),
            "law".to_string()
        )]);
    }

    /// End-to-end BM25: only positive scores, ranking, matched
    /// fields in reference order.
    #[test]
    fn search_ranks_and_reports_fields() {
        let records = vec![
            json!({"type": "convention", "content": "rust guidelines rust"}),
            json!({"type": "guide", "name": "rust", "description": "unrelated"}),
            json!({"type": "convention", "content": "typescript"}),
        ];
        let results = search_bm25(&records, "rust", DEFAULT_BM25_PARAMS);
        assert_eq!(results.len(), 2);
        // Both matched `rust`; the convention has tf=2 — higher score.
        assert!(results[0].score > results[1].score);
        assert_eq!(results[0].matched_fields, ["content"]);
        assert_eq!(results[1].matched_fields, ["name"]);

        // Blank query and empty corpus return nothing.
        assert!(search_bm25(&records, "  ", DEFAULT_BM25_PARAMS).is_empty());
        assert!(search_bm25(&[], "rust", DEFAULT_BM25_PARAMS).is_empty());
        // Query of only punctuation tokenizes to nothing.
        assert!(search_bm25(&records, "!!", DEFAULT_BM25_PARAMS).is_empty());
    }
}
