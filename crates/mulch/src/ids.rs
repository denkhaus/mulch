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

/// A type's compaction strategy (reference `TypeDefinition.compact`;
/// the six builtins are all `concat`, customs default to `manual`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompactStrategy {
    /// Required fields join with blank lines (`name`/`title` keep the
    /// longest value instead).
    Concat,
    /// The newest record's body, re-stamped and re-superseded.
    KeepLatest,
    /// Like keep_latest, plus every input's `outcomes[]` merged.
    MergeOutcomes,
    /// `--auto` skips the group with a warning; `--apply` only.
    Manual,
}

/// One registry row per record type (the reference `builtins.ts`
/// table): payload fields in canonical order, the id/dedup key, and
/// whether duplicates upsert (named) or skip (anonymous). Every other
/// type list in the codebase derives from — or is test-pinned equal
/// to — this table (mulch-a3de; `PAYLOAD_TYPES` is the pinned list).
pub struct TypeSpec {
    /// The type name (registry order = the reference's fixed order).
    pub name:          &'static str,
    /// The markdown section heading (reference `sectionTitle`; the
    /// parent fallback for `extends` customs).
    pub section_title: &'static str,
    /// Payload fields in canonical write order.
    pub payload:       &'static [&'static str],
    /// The id/dedup key field.
    pub id_key:        &'static str,
    /// Whether duplicates upsert (named types) or skip (anonymous).
    pub named:         bool,
    /// The reference branch's `optional` field list (fields the type
    /// declares but does not require; anything else is additional).
    pub optional:      &'static [&'static str],
    /// The `extracts_files` field (a string array in the schema),
    /// when the builtin has one.
    pub files:         Option<&'static str>,
    /// The compaction strategy (all six builtins: `concat`).
    pub compact:       CompactStrategy,
}

/// How a record's one-line summary renders (the reference's
/// `def.summary`): builtins read the id-key field (convention and
/// failure truncated to 60); custom types interpolate their summary
/// template, or inherit the parent builtin's law.
#[derive(Clone, Debug, PartialEq)]
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
#[derive(Clone, Debug, PartialEq)]
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
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// The type name (builtins in the reference's fixed order first,
    /// then customs in config order).
    pub name:          String,
    /// The markdown section heading: the builtin's title, or the
    /// custom's `section_title` ?? parent's ?? Capitalize(name)+"s"
    /// (reference `custom.ts` `defaultSectionTitle` — markdown only;
    /// compact/xml/plain never read it).
    pub section_title: String,
    /// Required fields in schema/write order (parent-first for
    /// customs).
    pub payload:       Vec<String>,
    /// The optional field list.
    pub optional:      Vec<String>,
    /// The id key field (`content_hash` reads an absent field — the
    /// id hashes the type and an empty value).
    pub id_key:        String,
    /// The dedup field; `None` is the never-matching `content_hash`
    /// mode.
    pub dedup:         Option<String>,
    /// Whether duplicates upsert (named) or skip (anonymous):
    /// builtins per the static table, customs when their dedup key
    /// is a field (reference `isNamedType`).
    pub named:         bool,
    /// A builtin row (doctor's registry counts).
    pub builtin:       bool,
    /// The field that is a string array instead of a plain string
    /// (`extracts_files` resolved with `files_field`).
    pub files_array:   Option<String>,
    /// How the record's summary line renders.
    pub summary:       SummaryLaw,
    /// The compaction strategy (builtins `concat`; customs
    /// `compact ?? parent's ?? manual`).
    pub compact:       CompactStrategy,
    /// Canonical field → legacy names, config-declared order
    /// (custom types only): applied at the read seams so disk
    /// records keep working after a field rename (reference
    /// `applyAliases`, mulch-b1cb).
    pub aliases:       Vec<(String, Vec<String>)>,
}

/// The merged registry: the six builtins plus the config's
/// `custom_types` rows (mulch-b4bd). Built from the store's config
/// at open time; the builtins-only view serves configless contexts.
#[derive(Clone, Debug, PartialEq)]
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
            rows.push(crate::custom_types::build_row(&name, &cfg));
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

    /// The id key field for a type (unknown types key on content).
    #[must_use]
    pub fn id_key_field(&self, record_type: &str) -> &str {
        self.row(record_type)
            .map_or("content", |row| row.id_key.as_str())
    }
}

impl Row {
    fn from_spec(spec: &TypeSpec) -> Self {
        Self {
            name:          spec.name.to_string(),
            section_title: spec.section_title.to_string(),
            payload:       spec.payload.iter().map(|f| (*f).to_string()).collect(),
            optional:      spec.optional.iter().map(|f| (*f).to_string()).collect(),
            id_key:        spec.id_key.to_string(),
            dedup:         Some(spec.id_key.to_string()),
            named:         spec.named,
            builtin:       true,
            files_array:   spec.files.map(str::to_string),
            summary:       SummaryLaw::Field {
                field:    spec.id_key.to_string(),
                truncate: matches!(spec.name, "convention" | "failure"),
            },
            compact:       spec.compact,
            aliases:       Vec::new(),
        }
    }
}

/// The six built-in types in the reference's fixed order.
pub const REGISTRY: [TypeSpec; 6] = [
    TypeSpec {
        name:          "convention",
        section_title: "Conventions",
        payload:       &["content"],
        id_key:        "content",
        named:         false,
        optional:      &[],
        files:         None,
        compact:       CompactStrategy::Concat,
    },
    TypeSpec {
        name:          "pattern",
        section_title: "Patterns",
        payload:       &["name", "description"],
        id_key:        "name",
        named:         true,
        optional:      &["files"],
        files:         Some("files"),
        compact:       CompactStrategy::Concat,
    },
    TypeSpec {
        name:          "failure",
        section_title: "Known Failures",
        payload:       &["description", "resolution"],
        id_key:        "description",
        named:         false,
        optional:      &[],
        files:         None,
        compact:       CompactStrategy::Concat,
    },
    TypeSpec {
        name:          "decision",
        section_title: "Decisions",
        payload:       &["title", "rationale"],
        id_key:        "title",
        named:         true,
        optional:      &["date"],
        files:         None,
        compact:       CompactStrategy::Concat,
    },
    TypeSpec {
        name:          "reference",
        section_title: "References",
        payload:       &["name", "description"],
        id_key:        "name",
        named:         true,
        optional:      &["files"],
        files:         Some("files"),
        compact:       CompactStrategy::Concat,
    },
    TypeSpec {
        name:          "guide",
        section_title: "Guides",
        payload:       &["name", "description"],
        id_key:        "name",
        named:         true,
        optional:      &[],
        files:         None,
        compact:       CompactStrategy::Concat,
    },
];

/// The record's effective type: a missing or non-string `type` means
/// `convention` (the reference default — a format law, not a local
/// choice; mulch-3f08).
#[must_use]
pub fn effective_type(record_type: Option<&str>) -> &str {
    record_type.unwrap_or("convention")
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
}
