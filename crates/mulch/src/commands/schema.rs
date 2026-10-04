//! The reference record schema: a oneOf over the six record types.
//!
//! Each branch = the registry row's ajv schema (reference
//! `builtins.ts`): required keys, `additionalProperties: false`, and
//! per-property subschemas in the reference's declaration order (base
//! keys first, then the type const, then the payload fields, then
//! `files` where the branch declares it). A branch reports its FIRST
//! failure in evaluation order: required → additionalProperties →
//! properties in declaration order (probe-fitted 2026-10-04 vs
//! ml 0.10.7; mulch-b8ca). Sub-errors are structured ([`SubError`])
//! and render with their ajv path — a leading space for the empty
//! path, `/{path}` otherwise — joined with [`SUB_SEP`]. Records with a
//! present-but-unregistered `type` fail with `Unknown record \`X\``
//! instead of the oneOf blob.

/// The separator between oneOf sub-errors (reference join).
pub(crate) const SUB_SEP: &str = "; ";

/// The validate prefix before the joined sub-errors.
pub(crate) const VALIDATION_PREFIX: &str = "Schema validation failed: ";

/// The reference id pattern (`baseSchemaProps.id`).
pub(crate) const ID_PATTERN: &str = "^mx-[0-9a-f]{4,8}$";

/// The reference link pattern (`relates_to`/`supersedes` items).
pub(crate) const REF_PATTERN: &str = "^([a-z0-9-]+:)?mx-[0-9a-f]{4,8}$";

/// The base properties every branch declares, in the reference's
/// declaration order (`baseSchemaProps` key order — the evaluation
/// order for property errors).
const BASE_PROPERTIES: [&str; 13] = [
    "id",
    "classification",
    "recorded_at",
    "evidence",
    "tags",
    "relates_to",
    "supersedes",
    "outcomes",
    "dir_anchors",
    "supersession_demoted_at",
    "anchor_decay_demoted_at",
    "owner",
    "status",
];

/// The three classification values (reference enum).
const CLASSIFICATIONS: [&str; 3] = ["foundational", "tactical", "observational"];

/// The three record-status values (reference enum; `archived` is
/// intentionally absent — soft-archived records bypass the schema).
const STATUSES: [&str; 3] = ["draft", "active", "deprecated"];

/// One structured sub-error; rendering adds the ajv path.
#[derive(Debug, PartialEq)]
pub(crate) enum SubError {
    /// The instance is not an object at all.
    MustBeObject,
    /// A required key is absent (key presence — a `null` value counts
    /// as present and fails the property type check instead).
    Missing(&'static str),
    /// A key outside the branch's declared properties.
    Additional,
    /// A property (or item) is not of the expected kind.
    Type {
        path:     String,
        expected: &'static str,
    },
    /// A property value is outside its enum.
    Enum { path: &'static str },
    /// A property value violates its pattern (`/id`, link items).
    Pattern { path: String },
    /// The record's `type` differs from the branch's const.
    TypeConst,
    /// The oneOf summary line (validate/doctor omit it; the built/
    /// batch surfaces and `move` carry it).
    OneOfTail,
}

impl SubError {
    /// The rendered ajv entry: `{path} {message}` (empty path → the
    /// leading space).
    fn render(&self) -> String {
        match self {
            SubError::MustBeObject => " must be object".into(),
            SubError::Missing(field) => format!(" must have required property '{field}'"),
            SubError::Additional => " must NOT have additional properties".into(),
            SubError::Type { path, expected } => {
                if path.is_empty() {
                    format!(" must be {expected}")
                } else {
                    format!("/{path} must be {expected}")
                }
            }
            SubError::Enum { path } => {
                format!("/{path} must be equal to one of the allowed values")
            }
            SubError::Pattern { path } => {
                if path == "id" {
                    format!("/id must match pattern \"{ID_PATTERN}\"")
                } else {
                    format!("/{path} must match pattern \"{REF_PATTERN}\"")
                }
            }
            SubError::TypeConst => "/type must be equal to constant".into(),
            SubError::OneOfTail => " must match exactly one schema in oneOf".into(),
        }
    }
}

/// Renders the sub-error list (validate, doctor and the batch entries
/// share this).
pub(crate) fn render_subs(subs: &[SubError]) -> Vec<String> {
    subs.iter().map(SubError::render).collect()
}

/// A record's schema verdict.
#[derive(Debug)]
pub(crate) enum Verdict {
    /// The record matches a branch.
    Valid,
    /// Unregistered `type` value; carries the offending type.
    Unknown(String),
    /// No branch matched; carries the per-branch sub-errors.
    OneOf(Vec<SubError>),
}

/// The oneOf verdict for one parsed record.
pub(crate) fn verdict(record: &serde_json::Value) -> Verdict {
    let Some(object) = record.as_object() else {
        // A non-object line is not a record; ajv's wrapper type fires.
        return Verdict::OneOf(vec![SubError::MustBeObject]);
    };
    let record_type = object.get("type").and_then(serde_json::Value::as_str);

    if let Some(kind) = record_type.filter(|kind| mulch::type_spec(kind).is_none()) {
        return Verdict::Unknown(kind.into());
    }

    match one_of_subs(object) {
        None => Verdict::Valid,
        Some(subs) => Verdict::OneOf(subs),
    }
}

/// The per-branch sub-errors plus the oneOf summary line — `None` when
/// a branch matched. Every surface (validate, doctor, batch, move)
/// carries the tail.
fn one_of_subs(object: &serde_json::Map<String, serde_json::Value>) -> Option<Vec<SubError>> {
    let mut subs = mulch::REGISTRY
        .iter()
        .map(|spec| branch_error(spec, object))
        .collect::<Option<Vec<SubError>>>()?;
    subs.push(SubError::OneOfTail);
    Some(subs)
}

/// The branch's required keys in reference order.
fn required_keys(spec: &mulch::TypeSpec) -> Vec<&'static str> {
    std::iter::once("type")
        .chain(spec.payload.iter().copied())
        .chain(["classification", "recorded_at"])
        .collect()
}

/// The branch's declared property set (the additionalProperties
/// guard).
fn declared_properties(spec: &mulch::TypeSpec) -> Vec<&'static str> {
    let mut declared: Vec<&'static str> = BASE_PROPERTIES
        .iter()
        .copied()
        .chain(["type"])
        .chain(spec.payload.iter().copied())
        .collect();
    // `files` is declared only where the branch schema declares it
    // (pattern, reference); elsewhere it is an additional property.
    if spec.declares_files {
        declared.push("files");
    }
    declared
}

/// One branch's first failing check, `None` when the branch matches.
fn branch_error(
    spec: &mulch::TypeSpec,
    object: &serde_json::Map<String, serde_json::Value>,
) -> Option<SubError> {
    // 1. required — key presence, in required-array order.
    let required = required_keys(spec);
    if let Some(field) = required.iter().find(|field| !object.contains_key(**field)) {
        return Some(SubError::Missing(field));
    }

    // 2. additionalProperties — only the branch's declared keys.
    let declared = declared_properties(spec);
    let has_additional = object.keys().any(|key| !declared.contains(&key.as_str()));
    if has_additional {
        return Some(SubError::Additional);
    }

    // 3. properties in declaration order: base keys first…
    for key in BASE_PROPERTIES {
        if let Some(value) = object.get(key)
            && let Some(error) = base_property_error(key, value)
        {
            return Some(error);
        }
    }
    // …then the type const…
    if object.get("type").and_then(serde_json::Value::as_str) != Some(spec.name) {
        return Some(SubError::TypeConst);
    }
    // …then the payload fields (strings)…
    for field in spec.payload {
        if let Some(value) = object.get(*field)
            && !value.is_string()
        {
            return Some(SubError::Type {
                path:     (*field).into(),
                expected: "string",
            });
        }
    }
    // …then `files` where the branch declares it (pattern, reference).
    if spec.declares_files
        && let Some(value) = object.get("files")
        && let Some(error) = string_array_error("files", value)
    {
        return Some(error);
    }
    None
}

/// The base property's first failing subschema check.
fn base_property_error(key: &str, value: &serde_json::Value) -> Option<SubError> {
    match key {
        "id" => match value {
            serde_json::Value::String(text) if !id_matches(text) => {
                Some(SubError::Pattern { path: "id".into() })
            }
            serde_json::Value::String(_) => None,
            _ => Some(SubError::Type {
                path:     "id".into(),
                expected: "string",
            }),
        },
        "classification" => match value {
            serde_json::Value::String(text) => {
                if CLASSIFICATIONS.contains(&text.as_str()) {
                    None
                } else {
                    Some(SubError::Enum {
                        path: "classification",
                    })
                }
            }
            _ => Some(SubError::Type {
                path:     "classification".into(),
                expected: "string",
            }),
        },
        "status" => match value {
            serde_json::Value::String(text) => {
                if STATUSES.contains(&text.as_str()) {
                    None
                } else {
                    Some(SubError::Enum { path: "status" })
                }
            }
            _ => Some(SubError::Type {
                path:     "status".into(),
                expected: "string",
            }),
        },
        "evidence" => (!value.is_object()).then(|| SubError::Type {
            path:     "evidence".into(),
            expected: "object",
        }),
        "outcomes" => (!value.is_array()).then(|| SubError::Type {
            path:     "outcomes".into(),
            expected: "array",
        }),
        "tags" | "dir_anchors" => string_array_error(key, value),
        "relates_to" | "supersedes" => link_array_error(key, value),
        // recorded_at, supersession_demoted_at, anchor_decay_demoted_at,
        // owner: plain strings.
        _ => (!value.is_string()).then(|| SubError::Type {
            path:     key.into(),
            expected: "string",
        }),
    }
}

/// An array-of-strings property's first failure.
fn string_array_error(key: &str, value: &serde_json::Value) -> Option<SubError> {
    let Some(items) = value.as_array() else {
        return Some(SubError::Type {
            path:     key.into(),
            expected: "array",
        });
    };
    for (index, item) in items.iter().enumerate() {
        if !item.is_string() {
            return Some(SubError::Type {
                path:     format!("{key}/{index}"),
                expected: "string",
            });
        }
    }
    None
}

/// A link-array property's first failure (array of pattern strings).
fn link_array_error(key: &str, value: &serde_json::Value) -> Option<SubError> {
    let Some(items) = value.as_array() else {
        return Some(SubError::Type {
            path:     key.into(),
            expected: "array",
        });
    };
    for (index, item) in items.iter().enumerate() {
        let text = item.as_str();
        if text.is_none() {
            return Some(SubError::Type {
                path:     format!("{key}/{index}"),
                expected: "string",
            });
        }
        if text.is_some_and(|text| !ref_matches(text)) {
            return Some(SubError::Pattern {
                path: format!("{key}/{index}"),
            });
        }
    }
    None
}

/// Lowercase-hex tail check of the id pattern.
fn id_matches(text: &str) -> bool {
    match text.strip_prefix("mx-") {
        Some(hex) => id_hex_ok(hex),
        None => false,
    }
}

/// 4-8 lowercase hex digits.
fn id_hex_ok(hex: &str) -> bool {
    (4..=8).contains(&hex.len())
        && hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Lowercase-hex tail check of the link pattern.
pub(crate) fn ref_matches(text: &str) -> bool {
    match text.strip_prefix("mx-") {
        Some(hex) => id_hex_ok(hex),
        None => match text.split_once(":mx-") {
            Some((prefix, hex)) => {
                !prefix.is_empty()
                    && prefix
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
                    && id_hex_ok(hex)
            }
            None => false,
        },
    }
}

/// A full-record verdict for BUILT/batch records (validate's verdict
/// minus the unknown-type short-circuit — an unregistered type is one
/// more failing branch there, matching the batch error surface).
pub(crate) enum FullVerdict {
    /// The record matches a branch.
    Valid,
    /// Validation failed; carries the sub-error list and the
    /// per-type hint line.
    Invalid { subs: Vec<SubError>, hint: String },
}

pub(crate) fn full_verdict(record: &serde_json::Value) -> FullVerdict {
    let object = record.as_object().expect("built records are objects");
    let record_type = object.get("type").and_then(serde_json::Value::as_str);
    let Some(subs) = one_of_subs(object) else {
        return FullVerdict::Valid;
    };
    let fields = crate::commands::hint_fields(record_type.unwrap_or("convention"));
    FullVerdict::Invalid {
        subs,
        hint: format!(
            "Hint: {} records require: {}",
            record_type.unwrap_or("convention"),
            fields
        ),
    }
}

/// The validate finding message for a non-valid record.
pub(crate) fn validate_message(record: &serde_json::Value) -> Option<String> {
    match verdict(record) {
        Verdict::Valid => None,
        Verdict::Unknown(kind) => Some(format!("Unknown record `{kind}`")),
        Verdict::OneOf(subs) => Some(format!(
            "{VALIDATION_PREFIX}{}",
            render_subs(&subs).join(SUB_SEP)
        )),
    }
}

/// The doctor detail line for a non-valid record: no wrapper; the
/// caller prefixes `domain:line - ` and the sub-errors' own path
/// spaces provide the reference's extra gap.
pub(crate) fn doctor_detail(record: &serde_json::Value) -> Option<String> {
    match verdict(record) {
        Verdict::Valid => None,
        Verdict::Unknown(kind) => Some(format!("Unknown record `{kind}`")),
        Verdict::OneOf(subs) => Some(render_subs(&subs).join(SUB_SEP)),
    }
}

/// The plain-mode stderr rendering of a validate finding.
pub(crate) fn plain_detail_lines(message: &str) -> Vec<String> {
    if let Some(payload) = message.strip_prefix(VALIDATION_PREFIX) {
        let mut lines = vec!["Schema validation failed:".into()];
        // Each entry carries its own ajv path prefix: "  " + path +
        // " " + message (the empty path yields the reference's extra
        // gap; /type yields a single one).
        lines.extend(payload.split(SUB_SEP).map(|sub| format!("  {sub}")));
        lines
    } else {
        vec![message.into()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subs_for(line: &str) -> Vec<String> {
        let record: serde_json::Value = serde_json::from_str(line).expect("json");
        match verdict(&record) {
            Verdict::OneOf(subs) => render_subs(&subs),
            other => panic!("expected OneOf, got {other:?}"),
        }
    }

    #[test]
    fn evaluation_order_is_required_then_additional_then_properties() {
        // required wins over everything (missing content + enum +
        // const all failing on the convention branch)
        let subs = subs_for(r#"{"type":"guide","name":"g","description":"d","recorded_at":"x"}"#);
        assert_eq!(subs[0], " must have required property 'content'");
        // additionalProperties before property errors (bogus + weird
        // classification: pattern branch reports the additional)
        let subs = subs_for(
            r#"{"type":"guide","name":"g","description":"d","classification":"weird","recorded_at":"x","bogus":1}"#,
        );
        assert_eq!(subs[1], " must NOT have additional properties");
        // declaration order: id before classification before type const
        let subs = subs_for(
            r#"{"type":"guide","name":"g","description":123,"classification":"weird","recorded_at":"x","id":"XX-bad"}"#,
        );
        assert_eq!(subs[1], "/id must match pattern \"^mx-[0-9a-f]{4,8}$\"");
        // …classification enum before the const…
        let subs = subs_for(
            r#"{"type":"guide","name":"g","description":"d","classification":"weird","recorded_at":"x"}"#,
        );
        assert_eq!(
            subs[1],
            "/classification must be equal to one of the allowed values"
        );
        // …const before the payload field type.
        let subs = subs_for(
            r#"{"type":"guide","name":"g","description":123,"classification":"tactical","recorded_at":"x"}"#,
        );
        assert_eq!(subs[1], "/type must be equal to constant");
    }

    #[test]
    fn null_counts_as_present_and_fails_the_type_check() {
        let subs = subs_for(
            r#"{"type":"guide","name":null,"description":"d","classification":"tactical","recorded_at":"x"}"#,
        );
        // the guide branch (index 5; the tail is last) reports the
        // payload type error
        assert_eq!(subs[5], "/name must be string");
    }

    #[test]
    fn files_is_declared_only_where_the_reference_declares_it() {
        // files on a guide record: additional for guide's own branch
        // (index 5 of 6; the tail is last)
        let subs = subs_for(
            r#"{"type":"guide","name":"g","description":"d","classification":"tactical","recorded_at":"x","files":[]}"#,
        );
        assert_eq!(subs[5], " must NOT have additional properties");
        // …but declared (and type-checked) on pattern: the pattern
        // branch (index 1) reports before the const
        let subs = subs_for(
            r#"{"type":"pattern","name":"g","description":"d","classification":"tactical","recorded_at":"x","files":"oops"}"#,
        );
        assert_eq!(subs[1], "/files must be array");
    }

    #[test]
    fn link_arrays_check_items_for_strings_and_patterns() {
        let subs = subs_for(
            r#"{"type":"pattern","name":"g","description":"d","classification":"tactical","recorded_at":"x","relates_to":["ok:mx-abcd12","bad"]}"#,
        );
        // the pattern branch (index 1) reports its first failing property
        assert_eq!(
            subs[1],
            "/relates_to/1 must match pattern \"^([a-z0-9-]+:)?mx-[0-9a-f]{4,8}$\""
        );
    }

    #[test]
    fn non_objects_report_must_be_object() {
        let record: serde_json::Value = serde_json::json!(5);
        match verdict(&record) {
            Verdict::OneOf(subs) => {
                assert_eq!(render_subs(&subs), vec![" must be object"]);
            }
            other => panic!("expected OneOf, got {other:?}"),
        }
    }
}
