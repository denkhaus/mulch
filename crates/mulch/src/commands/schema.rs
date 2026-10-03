//! The reference record schema: a oneOf over the six record types.
//!
//! Empirical rule (probe-fitted, mulch-da8b): a record matches a branch
//! when its `type` equals the branch name and every required field is
//! present and non-null. When no branch matches, EVERY branch
//! contributes ` must have required property '<first required>'` —
//! including branches whose required fields are present but whose type
//! differs — followed by ` must match exactly one schema in oneOf`;
//! sub-errors join with `;  ` (semicolon + two spaces).

/// Branch names with their required fields (pinned from the reference).
pub(crate) const BRANCHES: [(&str, &[&str]); 6] = [
    ("convention", &["content"]),
    ("pattern", &["name", "description"]),
    ("failure", &["description", "resolution"]),
    ("decision", &["title", "rationale"]),
    ("reference", &["name", "description"]),
    ("guide", &["name", "description"]),
];

/// The oneOf verdict for one parsed record: `None` when valid.
pub(crate) fn schema_error(record: &serde_json::Value) -> Option<String> {
    let object = record.as_object()?;
    let record_type = object.get("type").and_then(serde_json::Value::as_str);

    let matches = BRANCHES.iter().any(|(name, required)| {
        record_type == Some(*name)
            && required
                .iter()
                .all(|field| object.get(*field).is_some_and(|v| !v.is_null()))
    });
    if matches {
        return None;
    }

    let mut subs: Vec<String> = BRANCHES
        .iter()
        .map(|(_, required)| format!(" must have required property '{}'", required[0]))
        .collect();
    subs.push(" must match exactly one schema in oneOf".into());
    Some(format!("Schema validation failed: {}", subs.join(";  ")))
}
