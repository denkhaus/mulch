//! Deterministic record ids (reference `src/utils/expertise.ts:113`):
//! `mx-` + first 6 hex chars of sha256 over `"<type>:<idKey value>"`.

use sha2::{Digest as _, Sha256};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_reference_ids() {
        assert_eq!(record_id("convention", "c1"), "mx-1d1926");
        assert_eq!(record_id("reference", "N1"), "mx-b9079b");
        assert_eq!(record_id("pattern", "N1"), "mx-a6adb8");
        assert_eq!(record_id("guide", "N1"), "mx-ab7ce3");
    }
}
