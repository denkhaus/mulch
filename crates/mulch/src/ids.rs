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

/// One registry row per record type (the reference `builtins.ts`
/// table): payload fields in canonical order, the id/dedup key, and
/// whether duplicates upsert (named) or skip (anonymous). Every other
/// type list in the codebase derives from this table (mulch-a3de).
pub struct TypeSpec {
    /// The type name (registry order = the reference's fixed order).
    pub name:    &'static str,
    /// Payload fields in canonical write order.
    pub payload: &'static [&'static str],
    /// The id/dedup key field.
    pub id_key:  &'static str,
    /// Whether duplicates upsert (named types) or skip (anonymous).
    pub named:   bool,
}

/// The six built-in types in the reference's fixed order.
pub const REGISTRY: [TypeSpec; 6] = [
    TypeSpec {
        name:    "convention",
        payload: &["content"],
        id_key:  "content",
        named:   false,
    },
    TypeSpec {
        name:    "pattern",
        payload: &["name", "description"],
        id_key:  "name",
        named:   true,
    },
    TypeSpec {
        name:    "failure",
        payload: &["description", "resolution"],
        id_key:  "description",
        named:   false,
    },
    TypeSpec {
        name:    "decision",
        payload: &["title", "rationale"],
        id_key:  "title",
        named:   true,
    },
    TypeSpec {
        name:    "reference",
        payload: &["name", "description"],
        id_key:  "name",
        named:   true,
    },
    TypeSpec {
        name:    "guide",
        payload: &["name", "description"],
        id_key:  "name",
        named:   true,
    },
];

/// The registry row for a type, when it is one of the six built-ins.
#[must_use]
pub fn type_spec(record_type: &str) -> Option<&'static TypeSpec> {
    REGISTRY.iter().find(|spec| spec.name == record_type)
}

/// The id key field per record type (registry row; convention and
/// unknown types key on content).
pub fn id_key_field(record_type: &str) -> &'static str {
    type_spec(record_type).map_or("content", |spec| spec.id_key)
}

/// Whether a type upserts on duplicate instead of skipping (reference
/// `isNamedType`: anonymous `convention`/`failure` skip, the named
/// types upsert; unknown types default to named, though the duplicate
/// detector never matches them).
#[must_use]
pub fn is_named_type(record_type: &str) -> bool {
    type_spec(record_type).is_none_or(|spec| spec.named)
}

/// Payload fields per record type, in canonical write order (registry
/// row; convention and unknown types carry only content).
#[must_use]
pub fn payload_fields(record_type: &str) -> &'static [&'static str] {
    type_spec(record_type).map_or(&["content"], |spec| spec.payload)
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
    fn registry_rows_pin_the_reference_table() {
        // The literals this registry replaced (verified against the
        // reference builtins.ts; mulch-a3de's probe-diff step).
        let expected: [(&str, &[&str], &str, bool); 6] = [
            ("convention", &["content"], "content", false),
            ("pattern", &["name", "description"], "name", true),
            (
                "failure",
                &["description", "resolution"],
                "description",
                false,
            ),
            ("decision", &["title", "rationale"], "title", true),
            ("reference", &["name", "description"], "name", true),
            ("guide", &["name", "description"], "name", true),
        ];
        for (spec, (name, payload, id_key, named)) in REGISTRY.iter().zip(expected) {
            assert_eq!(spec.name, name);
            assert_eq!(spec.payload, payload);
            assert_eq!(spec.id_key, id_key);
            assert_eq!(spec.named, named);
        }
        assert_eq!(
            PAYLOAD_TYPES,
            REGISTRY.map(|spec| spec.name),
            "the type list IS the registry order"
        );
    }

    #[test]
    fn named_types_are_everything_but_convention_and_failure() {
        assert!(!is_named_type("convention"));
        assert!(!is_named_type("failure"));
        for named in ["pattern", "decision", "reference", "guide"] {
            assert!(is_named_type(named));
        }
    }
}
