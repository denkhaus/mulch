//! The reference record schema: a oneOf over the six record types.
//!
//! Each branch = the registry row's ajv schema (reference
//! `builtins.ts`): required keys, `additionalProperties: false`, and
//! per-property subschemas in the reference's declaration order (base
//! keys first, then the type const, then the payload fields, then the
//! branch's `optional` fields). A branch reports its FIRST failing
//! property in evaluation order: required → additionalProperties →
//! properties in declaration order; a property with several failing
//! keywords reports all of them (ajv's per-branch errors are composite
//! — probe-fitted 2026-10-04 vs ml 0.10.7; mulch-b8ca). Sub-errors are
//! structured ([`SubError`]) and render with their ajv path — a
//! leading space for the empty path, `/{path}` otherwise — joined
//! with [`SUB_SEP`]. Records with a present-but-unregistered `type`
//! fail with ``Unknown record `X` `` instead of the oneOf blob.

use crate::ids::{effective_type, hint_fields};

/// The separator between oneOf sub-errors (reference join).
pub const SUB_SEP: &str = "; ";

/// The validate prefix before the joined sub-errors.
pub const VALIDATION_PREFIX: &str = "Schema validation failed: ";

/// The reference id pattern (`baseSchemaProps.id`).
pub const ID_PATTERN: &str = "^mx-[0-9a-f]{4,8}$";

/// The reference link pattern (`relates_to`/`supersedes` items).
pub const REF_PATTERN: &str = "^([a-z0-9-]+:)?mx-[0-9a-f]{4,8}$";

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
pub enum SubError {
    /// The instance is not an object at all.
    MustBeObject,
    /// A required key is absent (key presence — a `null` value counts
    /// as present and fails the property type check instead).
    Missing(&'static str),
    /// A key outside the branch's declared properties.
    Additional,
    /// A key outside a NESTED object's declared properties
    /// (`/evidence`, `/outcomes/0`).
    AdditionalAt { path: String },
    /// A required key absent inside a nested object.
    MissingAt { path: String, field: &'static str },
    /// A nested property value outside its enum.
    EnumAt { path: String },
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
    /// The oneOf summary line — every surface carries it (validate
    /// plain/json, doctor, batch entries, `move`).
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
            SubError::AdditionalAt { path } => {
                format!("/{path} must NOT have additional properties")
            }
            SubError::MissingAt { path, field } => {
                format!("/{path} must have required property '{field}'")
            }
            SubError::EnumAt { path } => {
                format!("/{path} must be equal to one of the allowed values")
            }
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

/// Renders the sub-error list (validate, doctor, batch and `move`
/// share this).
pub fn render_subs(subs: &[SubError]) -> Vec<String> {
    subs.iter().map(SubError::render).collect()
}

/// A record's schema verdict.
#[derive(Debug)]
pub enum Verdict {
    /// The record matches a branch.
    Valid,
    /// Unregistered `type` value; carries the offending type.
    Unknown(String),
    /// No branch matched; carries the per-branch sub-errors.
    OneOf(Vec<SubError>),
}

/// The oneOf verdict for one parsed record.
pub fn verdict(record: &serde_json::Value) -> Verdict {
    let Some(object) = record.as_object() else {
        // A non-object line is not a record; ajv's wrapper type fires.
        return Verdict::OneOf(vec![SubError::MustBeObject]);
    };
    let record_type = object.get("type").and_then(serde_json::Value::as_str);

    if let Some(kind) = record_type.filter(|kind| crate::ids::type_spec(kind).is_none()) {
        return Verdict::Unknown(kind.into());
    }

    match one_of_subs(object) {
        None => Verdict::Valid,
        Some(subs) => Verdict::OneOf(subs),
    }
}

/// The per-branch sub-errors plus the oneOf summary line — `None` when
/// a branch matched. Every surface carries the tail.
fn one_of_subs(object: &serde_json::Map<String, serde_json::Value>) -> Option<Vec<SubError>> {
    let mut subs = Vec::new();
    for spec in crate::ids::REGISTRY {
        subs.extend(branch_error(&spec, object)?);
    }
    subs.push(SubError::OneOfTail);
    Some(subs)
}

/// The branch's required keys in reference order.
fn required_keys(spec: &crate::ids::TypeSpec) -> Vec<&'static str> {
    std::iter::once("type")
        .chain(spec.payload.iter().copied())
        .chain(["classification", "recorded_at"])
        .collect()
}

/// The branch's declared property set (the additionalProperties
/// guard): the base keys, `type`, the payload fields and the branch's
/// optional fields.
fn declared_properties(spec: &crate::ids::TypeSpec) -> Vec<&'static str> {
    BASE_PROPERTIES
        .iter()
        .copied()
        .chain(["type"])
        .chain(spec.payload.iter().copied())
        .chain(spec.optional.iter().copied())
        .collect()
}

/// One branch's failing sub-errors, `None` when the branch matches:
/// the FIRST failing property contributes ALL its failing keywords.
fn branch_error(
    spec: &crate::ids::TypeSpec,
    object: &serde_json::Map<String, serde_json::Value>,
) -> Option<Vec<SubError>> {
    // 1. required — key presence, in required-array order.
    let required = required_keys(spec);
    if let Some(field) = required.iter().find(|field| !object.contains_key(**field)) {
        return Some(vec![SubError::Missing(field)]);
    }

    // 2. additionalProperties — only the branch's declared keys.
    let declared = declared_properties(spec);
    let has_additional = object.keys().any(|key| !declared.contains(&key.as_str()));
    if has_additional {
        return Some(vec![SubError::Additional]);
    }

    // 3. properties in declaration order: base keys first…
    for key in BASE_PROPERTIES {
        if let Some(value) = object.get(key) {
            let errors = base_property_errors(key, value);
            if !errors.is_empty() {
                return Some(errors);
            }
        }
    }
    // …then the type const (both keywords when the value is not even a
    // string)…
    match object.get("type") {
        Some(serde_json::Value::String(kind)) if kind == spec.name => {}
        Some(serde_json::Value::String(_)) => return Some(vec![SubError::TypeConst]),
        _ => {
            return Some(vec![
                SubError::Type {
                    path:     "type".into(),
                    expected: "string",
                },
                SubError::TypeConst,
            ]);
        }
    }
    // …then the payload fields (strings)…
    for field in spec.payload {
        if let Some(value) = object.get(*field)
            && !value.is_string()
        {
            return Some(vec![SubError::Type {
                path:     (*field).into(),
                expected: "string",
            }]);
        }
    }
    // …then the branch's optional fields (`files` is a string array,
    // `date` a plain string).
    for field in spec.optional {
        let Some(value) = object.get(*field) else {
            continue;
        };
        let error = if *field == "files" {
            string_array_error(field, value)
        } else {
            (!value.is_string()).then(|| SubError::Type {
                path:     (*field).into(),
                expected: "string",
            })
        };
        if let Some(error) = error {
            return Some(vec![error]);
        }
    }
    None
}

/// The base property's failing subschema checks. A property with two
/// failing keywords reports BOTH (probe: `"type": true` yields
/// `must be string` AND `must be equal to constant`; a non-string
/// `classification`/`status` yields the type error AND the enum one);
/// sibling properties still stop the branch at the first failing one.
fn base_property_errors(key: &str, value: &serde_json::Value) -> Vec<SubError> {
    match key {
        "id" => match value {
            serde_json::Value::String(text) if !id_matches(text) => {
                vec![SubError::Pattern { path: "id".into() }]
            }
            serde_json::Value::String(_) => Vec::new(),
            _ => vec![SubError::Type {
                path:     "id".into(),
                expected: "string",
            }],
        },
        "classification" => match value {
            serde_json::Value::String(text) if CLASSIFICATIONS.contains(&text.as_str()) => {
                Vec::new()
            }
            serde_json::Value::String(_) => vec![SubError::Enum {
                path: "classification",
            }],
            _ => vec![
                SubError::Type {
                    path:     "classification".into(),
                    expected: "string",
                },
                SubError::Enum {
                    path: "classification",
                },
            ],
        },
        "status" => match value {
            serde_json::Value::String(text) if STATUSES.contains(&text.as_str()) => Vec::new(),
            serde_json::Value::String(_) => vec![SubError::Enum { path: "status" }],
            _ => vec![
                SubError::Type {
                    path:     "status".into(),
                    expected: "string",
                },
                SubError::Enum { path: "status" },
            ],
        },
        "evidence" => evidence_errors(value),
        "outcomes" => outcomes_errors(value),
        "tags" | "dir_anchors" => string_array_error(key, value).into_iter().collect(),
        "relates_to" | "supersedes" => link_array_error(key, value).into_iter().collect(),
        // The four plain-string base keys, named so a new
        // BASE_PROPERTIES entry cannot slip through unchecked (the
        // coverage test pins the pairing).
        "recorded_at" | "supersession_demoted_at" | "anchor_decay_demoted_at" | "owner" => (!value
            .is_string())
        .then(|| SubError::Type {
            path:     key.into(),
            expected: "string",
        })
        .into_iter()
        .collect(),
        // Not a base property: the payload/type/optional checks own
        // those keys.
        _ => Vec::new(),
    }
}

/// The evidence fields in the reference's declaration order
/// (`definitions.evidence.properties` — the evaluation order).
const EVIDENCE_FIELDS: [&str; 8] = [
    "commit", "date", "issue", "file", "bead", "seeds", "gh", "linear",
];

/// The outcome-item fields in the reference's declaration order
/// (`definitions.outcome.properties`).
const OUTCOME_FIELDS: [&str; 6] = [
    "status",
    "duration",
    "test_results",
    "agent",
    "notes",
    "recorded_at",
];

/// The outcome status enum (`definitions.outcome.properties.status`).
const OUTCOME_STATUSES: [&str; 3] = ["success", "failure", "partial"];

/// The outcome item's plain-string fields, in declaration order
/// (after `status` and `duration`).
const OUTCOME_STRING_FIELDS: [&str; 4] = ["test_results", "agent", "notes", "recorded_at"];

/// The evidence subschema (`definitions.evidence`): an object of the
/// eight declared string fields with `additionalProperties: false` and
/// no required keys — first failure only (additional, then fields in
/// declaration order; a failing field reports just its type, one
/// keyword per field).
fn evidence_errors(value: &serde_json::Value) -> Vec<SubError> {
    let Some(object) = value.as_object() else {
        return vec![SubError::Type {
            path:     "evidence".into(),
            expected: "object",
        }];
    };
    if object
        .keys()
        .any(|key| !EVIDENCE_FIELDS.contains(&key.as_str()))
    {
        return vec![SubError::AdditionalAt {
            path: "evidence".into(),
        }];
    }
    for field in EVIDENCE_FIELDS {
        if let Some(field_value) = object.get(field)
            && !field_value.is_string()
        {
            return vec![SubError::Type {
                path:     format!("evidence/{field}"),
                expected: "string",
            }];
        }
    }
    Vec::new()
}

/// The outcomes subschema (`definitions.outcome`): an array of outcome
/// items — `status` required with its enum, `duration` a number, four
/// string fields, `additionalProperties: false`. The FIRST failing
/// item wins; within an item: required, then additional, then the
/// first failing property (with all its failing keywords — a non-string
/// status reports type AND enum, like the top-level enums).
fn outcomes_errors(value: &serde_json::Value) -> Vec<SubError> {
    let Some(items) = value.as_array() else {
        return vec![SubError::Type {
            path:     "outcomes".into(),
            expected: "array",
        }];
    };
    for (index, item) in items.iter().enumerate() {
        let Some(object) = item.as_object() else {
            return vec![SubError::Type {
                path:     format!("outcomes/{index}"),
                expected: "object",
            }];
        };
        if !object.contains_key("status") {
            return vec![SubError::MissingAt {
                path:  format!("outcomes/{index}"),
                field: "status",
            }];
        }
        if object
            .keys()
            .any(|key| !OUTCOME_FIELDS.contains(&key.as_str()))
        {
            return vec![SubError::AdditionalAt {
                path: format!("outcomes/{index}"),
            }];
        }
        match object.get("status") {
            Some(serde_json::Value::String(text)) if OUTCOME_STATUSES.contains(&text.as_str()) => {}
            Some(serde_json::Value::String(_)) => {
                return vec![SubError::EnumAt {
                    path: format!("outcomes/{index}/status"),
                }];
            }
            _ => {
                return vec![
                    SubError::Type {
                        path:     format!("outcomes/{index}/status"),
                        expected: "string",
                    },
                    SubError::EnumAt {
                        path: format!("outcomes/{index}/status"),
                    },
                ];
            }
        }
        if let Some(duration) = object.get("duration")
            && !duration.is_number()
        {
            return vec![SubError::Type {
                path:     format!("outcomes/{index}/duration"),
                expected: "number",
            }];
        }
        for field in OUTCOME_STRING_FIELDS {
            if let Some(field_value) = object.get(field)
                && !field_value.is_string()
            {
                return vec![SubError::Type {
                    path:     format!("outcomes/{index}/{field}"),
                    expected: "string",
                }];
            }
        }
    }
    Vec::new()
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
        let Some(text) = item.as_str() else {
            return Some(SubError::Type {
                path:     format!("{key}/{index}"),
                expected: "string",
            });
        };
        if !ref_matches(text) {
            return Some(SubError::Pattern {
                path: format!("{key}/{index}"),
            });
        }
    }
    None
}

/// Lowercase-hex tail check of the id pattern.
fn id_matches(text: &str) -> bool {
    text.strip_prefix("mx-").is_some_and(id_hex_ok)
}

/// 4-8 lowercase hex digits.
fn id_hex_ok(hex: &str) -> bool {
    (4..=8).contains(&hex.len())
        && hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Lowercase-hex tail check of the link pattern.
pub fn ref_matches(text: &str) -> bool {
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
pub enum FullVerdict {
    /// The record matches a branch.
    Valid,
    /// Validation failed; carries the sub-error list and the
    /// per-type hint line.
    Invalid { subs: Vec<SubError>, hint: String },
}

pub fn full_verdict(record: &serde_json::Value) -> FullVerdict {
    let Some(object) = record.as_object() else {
        // A non-object line is not a record; ajv's wrapper type fires
        // (mirrors `verdict` — the CLI only ever passes built records,
        // embedders may not).
        let kind = effective_type(None);
        return FullVerdict::Invalid {
            subs: vec![SubError::MustBeObject],
            hint: format!("Hint: {kind} records require: {}", hint_fields(kind)),
        };
    };
    let record_type = object.get("type").and_then(serde_json::Value::as_str);
    let Some(subs) = one_of_subs(object) else {
        return FullVerdict::Valid;
    };
    let kind = effective_type(record_type);
    FullVerdict::Invalid {
        subs,
        hint: format!("Hint: {kind} records require: {}", hint_fields(kind)),
    }
}

/// The validate finding message for a non-valid record.
pub fn validate_message(record: &serde_json::Value) -> Option<String> {
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
pub fn doctor_detail(record: &serde_json::Value) -> Option<String> {
    match verdict(record) {
        Verdict::Valid => None,
        Verdict::Unknown(kind) => Some(format!("Unknown record `{kind}`")),
        Verdict::OneOf(subs) => Some(render_subs(&subs).join(SUB_SEP)),
    }
}

/// The plain-mode stderr rendering of a validate finding.
pub fn plain_detail_lines(message: &str) -> Vec<String> {
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
        let subs = subs_for(r#"{"type":"guide","name":"g","description":"d","recorded_at":"x"}"#);
        assert_eq!(subs[0], " must have required property 'content'");
        let subs = subs_for(
            r#"{"type":"guide","name":"g","description":"d","classification":"weird","recorded_at":"x","bogus":1}"#,
        );
        assert_eq!(subs[1], " must NOT have additional properties");
        let subs = subs_for(
            r#"{"type":"guide","name":"g","description":123,"classification":"weird","recorded_at":"x","id":"XX-bad"}"#,
        );
        assert_eq!(subs[1], "/id must match pattern \"^mx-[0-9a-f]{4,8}$\"");
        let subs = subs_for(
            r#"{"type":"guide","name":"g","description":"d","classification":"weird","recorded_at":"x"}"#,
        );
        assert_eq!(
            subs[1],
            "/classification must be equal to one of the allowed values"
        );
        let subs = subs_for(
            r#"{"type":"guide","name":"g","description":123,"classification":"tactical","recorded_at":"x"}"#,
        );
        assert_eq!(subs[1], "/type must be equal to constant");
    }

    #[test]
    fn a_property_with_two_failing_keywords_reports_both() {
        // type: true -> must be string AND must be equal to constant
        let subs = subs_for(
            r#"{"type":true,"name":"g","description":"d","classification":"tactical","recorded_at":"x"}"#,
        );
        // entry 0 is the convention branch's missing content; the
        // pattern branch reports the type property's two keywords
        assert_eq!(subs[1], "/type must be string");
        assert_eq!(subs[2], "/type must be equal to constant");
        // classification: 5 -> type error AND enum error
        let subs = subs_for(
            r#"{"type":"guide","name":"g","description":"d","classification":5,"recorded_at":"x"}"#,
        );
        assert_eq!(subs[1], "/classification must be string");
        assert_eq!(
            subs[2],
            "/classification must be equal to one of the allowed values"
        );
    }

    #[test]
    fn every_base_property_is_checked() {
        let wrong: [(&str, &str); 13] = [
            ("id", "\"XX-bad\""),
            ("classification", "\"weird\""),
            ("recorded_at", "123"),
            ("evidence", "\"nope\""),
            ("tags", "\"nope\""),
            ("relates_to", "\"nope\""),
            ("supersedes", "\"nope\""),
            ("outcomes", "\"nope\""),
            ("dir_anchors", "\"nope\""),
            ("supersession_demoted_at", "123"),
            ("anchor_decay_demoted_at", "123"),
            ("owner", "123"),
            ("status", "\"weird\""),
        ];
        assert_eq!(wrong.len(), BASE_PROPERTIES.len());
        for (key, value) in wrong {
            let line = format!(
                r#"{{"type":"guide","name":"g","description":"d","classification":"tactical","recorded_at":"x","{key}":{value}}}"#
            );
            let subs = subs_for(&line);
            let guide_branch = &subs[5];
            assert!(
                guide_branch.contains("must be")
                    || guide_branch.contains("must match")
                    || guide_branch.contains("allowed values"),
                "base key {key} produced no check: {guide_branch}"
            );
        }
    }

    #[test]
    fn null_counts_as_present_and_fails_the_type_check() {
        let subs = subs_for(
            r#"{"type":"guide","name":null,"description":"d","classification":"tactical","recorded_at":"x"}"#,
        );
        assert_eq!(subs[5], "/name must be string");
    }

    #[test]
    fn optional_fields_are_declared_where_the_reference_declares_them() {
        // files on a guide record: additional for guide's own branch
        let subs = subs_for(
            r#"{"type":"guide","name":"g","description":"d","classification":"tactical","recorded_at":"x","files":[]}"#,
        );
        assert_eq!(subs[5], " must NOT have additional properties");
        // …declared on pattern, and type-checked there
        let subs = subs_for(
            r#"{"type":"pattern","name":"g","description":"d","classification":"tactical","recorded_at":"x","files":"oops"}"#,
        );
        assert_eq!(subs[1], "/files must be array");
        // decision's `date` is optional: a valid record with it passes…
        let record: serde_json::Value = serde_json::from_str(
            r#"{"type":"decision","title":"t","rationale":"r","classification":"tactical","recorded_at":"x","date":"2026-01-01"}"#,
        )
        .expect("json");
        assert!(matches!(verdict(&record), Verdict::Valid));
        // …a wrong-typed date fails on the decision branch (index 3 in
        // registry order)…
        let subs = subs_for(
            r#"{"type":"decision","title":"t","rationale":"r","classification":"tactical","recorded_at":"x","date":5}"#,
        );
        assert_eq!(subs[3], "/date must be string");
        // …and a wrong-typed date on a guide is an ADDITIONAL property
        let subs = subs_for(
            r#"{"type":"guide","name":"g","description":"d","classification":"tactical","recorded_at":"x","date":"2026-01-01"}"#,
        );
        assert_eq!(subs[5], " must NOT have additional properties");
    }

    #[test]
    fn link_arrays_check_items_for_strings_and_patterns() {
        let subs = subs_for(
            r#"{"type":"pattern","name":"g","description":"d","classification":"tactical","recorded_at":"x","relates_to":["ok:mx-abcd12","bad"]}"#,
        );
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
