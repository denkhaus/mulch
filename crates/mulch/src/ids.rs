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
/// type list in the codebase derives from — or is test-pinned equal
/// to — this table (mulch-a3de; `PAYLOAD_TYPES` is the pinned list).
pub struct TypeSpec {
    /// The type name (registry order = the reference's fixed order).
    pub name:     &'static str,
    /// Payload fields in canonical write order.
    pub payload:  &'static [&'static str],
    /// The id/dedup key field.
    pub id_key:   &'static str,
    /// Whether duplicates upsert (named types) or skip (anonymous).
    pub named:    bool,
    /// The reference branch's `optional` field list (fields the type
    /// declares but does not require; anything else is additional).
    pub optional: &'static [&'static str],
    /// The `extracts_files` field (a string array in the schema),
    /// when the builtin has one.
    pub files:    Option<&'static str>,
}

/// How a record's one-line summary renders (the reference's
/// `def.summary`): builtins read the id-key field (convention and
/// failure truncated to 60); custom types interpolate their summary
/// template, or inherit the parent builtin's law.
#[derive(Debug, PartialEq)]
pub enum SummaryLaw {
    /// The named field's value, optionally truncated.
    Field {
        /// The field to read.
        field:    String,
        /// Truncate to 60 chars (the sentence-end law).
        truncate: bool,
    },
    /// Interpolated template parts (`{field}` → `String(value)`),
    /// missing fields render empty.
    Template(Vec<SummaryPart>),
}

/// One part of a custom summary template.
#[derive(Debug, PartialEq)]
pub enum SummaryPart {
    /// Literal text.
    Literal(String),
    /// A `{field}` interpolation.
    Field(String),
}

/// One merged registry row — a builtin or a validated custom type.
/// The config-aware view every registry consumer works over
/// (mulch-b4bd): schema branches, duplicate detection, id
/// generation, and the doctor registry surfaces.
#[derive(Debug, PartialEq)]
pub struct Row {
    /// The type name (builtins in the reference's fixed order first,
    /// then customs in config order).
    pub name:        String,
    /// Required fields in schema/write order (parent-first for
    /// customs).
    pub payload:     Vec<String>,
    /// The optional field list.
    pub optional:    Vec<String>,
    /// The id key field (`content_hash` reads an absent field — the
    /// id hashes the type and an empty value).
    pub id_key:      String,
    /// The dedup field; `None` is the never-matching `content_hash`
    /// mode.
    pub dedup:       Option<String>,
    /// Whether duplicates upsert (named) or skip (anonymous):
    /// builtins per the static table, customs when their dedup key
    /// is a field (reference `isNamedType`).
    pub named:       bool,
    /// A builtin row (doctor's registry counts).
    pub builtin:     bool,
    /// The field that is a string array instead of a plain string
    /// (`extracts_files` resolved with `files_field`).
    pub files_array: Option<String>,
    /// How the record's summary line renders.
    pub summary:     SummaryLaw,
}

/// The merged registry: the six builtins plus the config's
/// `custom_types` rows (mulch-b4bd). Built from the store's config
/// at open time; the builtins-only view serves configless contexts.
#[derive(Debug, PartialEq)]
pub struct Registry {
    rows: Vec<Row>,
}

impl Registry {
    /// The builtins-only view (no store / no custom_types).
    #[must_use]
    pub fn builtins() -> Self {
        Self {
            rows: REGISTRY.iter().map(Row::from_spec).collect(),
        }
    }

    /// The merged view over a config's `custom_types` block; every
    /// entry is validated (the reference `validateCustomTypeConfig`
    /// law) before its row is built.
    ///
    /// # Errors
    ///
    /// The first validation failure's message, verbatim (the CLI
    /// renders it through the global config-error surface).
    pub fn from_config(config: &crate::config::Config) -> Result<Self, crate::error::Error> {
        let mut rows: Vec<Row> = REGISTRY.iter().map(Row::from_spec).collect();
        for (name, cfg) in crate::custom_types::entries(config.custom_types_value()) {
            crate::custom_types::validate(&name, &cfg)
                .map_err(|message| crate::error::Error::CustomTypes { message })?;
            let row = crate::custom_types::build_row(&name, &cfg);
            let named = row.dedup.is_some();
            rows.push(Row {
                name: row.name,
                payload: row.required,
                optional: row.optional,
                id_key: row.id_key,
                dedup: row.dedup,
                named,
                builtin: false,
                files_array: row.files_array,
                summary: row.summary,
            });
        }
        Ok(Self { rows })
    }

    /// All rows: builtins first (fixed order), then customs in
    /// config order (the oneOf and doctor walk order).
    #[must_use]
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// Whether a type is registered (builtin or custom).
    #[must_use]
    pub fn is_known(&self, record_type: &str) -> bool {
        self.rows.iter().any(|row| row.name == record_type)
    }

    /// The row for a type.
    #[must_use]
    pub fn row(&self, record_type: &str) -> Option<&Row> {
        self.rows.iter().find(|row| row.name == record_type)
    }

    /// The id key field for a type (unknown types key on content,
    /// like the static builtin helper).
    #[must_use]
    pub fn id_key_field(&self, record_type: &str) -> &str {
        self.row(record_type)
            .map_or("content", |row| row.id_key.as_str())
    }
}

impl Row {
    fn from_spec(spec: &TypeSpec) -> Self {
        Self {
            name:        spec.name.to_string(),
            payload:     spec.payload.iter().map(|f| (*f).to_string()).collect(),
            optional:    spec.optional.iter().map(|f| (*f).to_string()).collect(),
            id_key:      spec.id_key.to_string(),
            dedup:       Some(spec.id_key.to_string()),
            named:       spec.named,
            builtin:     true,
            files_array: spec.files.map(str::to_string),
            summary:     SummaryLaw::Field {
                field:    spec.id_key.to_string(),
                truncate: matches!(spec.name, "convention" | "failure"),
            },
        }
    }
}

/// The six built-in types in the reference's fixed order.
pub const REGISTRY: [TypeSpec; 6] = [
    TypeSpec {
        name:     "convention",
        payload:  &["content"],
        id_key:   "content",
        named:    false,
        optional: &[],
        files:    None,
    },
    TypeSpec {
        name:     "pattern",
        payload:  &["name", "description"],
        id_key:   "name",
        named:    true,
        optional: &["files"],
        files:    Some("files"),
    },
    TypeSpec {
        name:     "failure",
        payload:  &["description", "resolution"],
        id_key:   "description",
        named:    false,
        optional: &[],
        files:    None,
    },
    TypeSpec {
        name:     "decision",
        payload:  &["title", "rationale"],
        id_key:   "title",
        named:    true,
        optional: &["date"],
        files:    None,
    },
    TypeSpec {
        name:     "reference",
        payload:  &["name", "description"],
        id_key:   "name",
        named:    true,
        optional: &["files"],
        files:    Some("files"),
    },
    TypeSpec {
        name:     "guide",
        payload:  &["name", "description"],
        id_key:   "name",
        named:    true,
        optional: &[],
        files:    None,
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

/// The required-fields hint line content for a record type (registry
/// knowledge: the payload fields joined like the reference hint).
#[must_use]
pub fn hint_fields(record_type: &str) -> String {
    payload_fields(record_type).join(", ")
}

/// The record's effective type: a missing or non-string `type` means
/// `convention` (the reference default — a format law, not a local
/// choice; mulch-3f08).
#[must_use]
pub fn effective_type(record_type: Option<&str>) -> &str {
    record_type.unwrap_or("convention")
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
        for spec in REGISTRY {
            let expected = match spec.name {
                "convention" => (&["content"][..], "content", false, None),
                "pattern" | "reference" => {
                    (&["name", "description"][..], "name", true, Some("files"))
                }
                "failure" => (
                    &["description", "resolution"][..],
                    "description",
                    false,
                    None,
                ),
                "decision" => (&["title", "rationale"][..], "title", true, None),
                "guide" => (&["name", "description"][..], "name", true, None),
                other => panic!("unexpected registry row {other}"),
            };
            let (payload, id_key, named, files) = expected;
            assert_eq!(spec.payload, payload);
            assert_eq!(spec.id_key, id_key);
            assert_eq!(spec.named, named);
            assert_eq!(spec.files, files);
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
