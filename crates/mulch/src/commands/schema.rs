//! The reference record schema: a oneOf over the six record types.
//!
//! Empirical rule (differentially fitted, mulch-da8b): each branch
//! reports its first MISSING required property; when a branch's
//! required fields are all present but the record's `type` differs,
//! the branch reports its first required property instead (the type
//! const mismatch surfaces as a pseudo-missing field). Records with a
//! present-but-unregistered `type` fail with `Unknown record \`X\``
//! instead of the oneOf blob.

/// Branch names with their required fields (pinned from the reference).
pub(crate) const BRANCHES: [(&str, &[&str]); 6] = [
    ("convention", &["content"]),
    ("pattern", &["name", "description"]),
    ("failure", &["description", "resolution"]),
    ("decision", &["title", "rationale"]),
    ("reference", &["name", "description"]),
    ("guide", &["name", "description"]),
];

/// A record's schema verdict.
pub(crate) enum Verdict {
    /// The record matches a branch.
    Valid,
    /// Unregistered `type` value; carries the offending type.
    Unknown(String),
    /// No branch matched; carries the per-branch sub-errors.
    OneOf(Vec<String>),
}

/// The oneOf verdict for one parsed record.
pub(crate) fn verdict(record: &serde_json::Value) -> Verdict {
    let Some(object) = record.as_object() else {
        // A non-object line is not a record; treat it as type-less.
        return Verdict::OneOf(Vec::new());
    };
    let record_type = object.get("type").and_then(serde_json::Value::as_str);

    if let Some(kind) = record_type.filter(|kind| !BRANCHES.iter().any(|(name, _)| name == kind)) {
        return Verdict::Unknown(kind.into());
    }

    let mut subs = Vec::new();
    for (name, required) in BRANCHES {
        let missing: Vec<&str> = required
            .iter()
            .copied()
            .filter(|field| object.get(*field).is_none_or(serde_json::Value::is_null))
            .collect();
        if record_type == Some(name) && missing.is_empty() {
            return Verdict::Valid;
        }
        let reported = if let Some(first) = missing.first() {
            *first
        } else {
            // Type const mismatch with every required field present:
            // the branch surfaces its first required property.
            required[0]
        };
        subs.push(format!("must have required property '{reported}'"));
    }
    subs.push("must match exactly one schema in oneOf".into());
    Verdict::OneOf(subs)
}

/// The validate finding message for a non-valid record.
pub(crate) fn validate_message(record: &serde_json::Value) -> Option<String> {
    match verdict(record) {
        Verdict::Valid => None,
        Verdict::Unknown(kind) => Some(format!("Unknown record `{kind}`")),
        Verdict::OneOf(subs) => Some(format!("Schema validation failed:  {}", subs.join(";  "))),
    }
}

/// The doctor detail line for a non-valid record (no wrapper, two
/// spaces after the dash — pinned from the reference).
pub(crate) fn doctor_detail(record: &serde_json::Value) -> Option<String> {
    match verdict(record) {
        Verdict::Valid => None,
        Verdict::Unknown(kind) => Some(format!("Unknown record `{kind}`")),
        Verdict::OneOf(subs) => Some(subs.join(";  ")),
    }
}

/// The plain-mode stderr rendering of a validate finding.
pub(crate) fn plain_detail_lines(message: &str) -> Vec<String> {
    if let Some(payload) = message.strip_prefix("Schema validation failed:  ") {
        let mut lines = vec!["Schema validation failed:".into()];
        lines.extend(payload.split(";  ").map(|sub| format!("   {sub}")));
        lines
    } else {
        vec![message.into()]
    }
}
