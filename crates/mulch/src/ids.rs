//! Deterministic record ids (reference `src/utils/expertise.ts:113`):
//! `mx-` + first 6 hex chars of sha256 over `"<type>:<idKey value>"`.

use sha2::{Digest as _, Sha256};

/// The registered record types (reference registry).
pub const PAYLOAD_TYPES: [&str; 6] = [
    "convention",
    "pattern",
    "failure",
    "decision",
    "reference",
    "guide",
];

/// The id key field per record type (reference registry).
pub fn id_key_field(record_type: &str) -> &'static str {
    match record_type {
        "pattern" | "reference" | "guide" => "name",
        "failure" => "description",
        "decision" => "title",
        // convention and unknown types key on content.
        _ => "content",
    }
}

/// Payload fields per record type, in canonical write order (the
/// reference registry: convention=content, pattern/reference/guide=
/// name+description, failure=description+resolution, decision=
/// title+rationale).
#[must_use]
pub fn payload_fields(record_type: &str) -> &'static [&'static str] {
    match record_type {
        "pattern" | "reference" | "guide" => &["name", "description"],
        "failure" => &["description", "resolution"],
        "decision" => &["title", "rationale"],
        _ => &["content"],
    }
}

/// Computes the record id for a payload.
pub fn record_id(record_type: &str, id_key_value: &str) -> String {
    let key = format!("{record_type}:{id_key_value}");
    let digest = Sha256::digest(key.as_bytes());
    let mut hex = String::with_capacity(6);
    for byte in digest.iter().take(3) {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
    }
    format!("mx-{hex}")
}
