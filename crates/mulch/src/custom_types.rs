//! The `custom_types` config block: entries parsing, the reference
//! validation law (message-exact, `registry/custom.ts`), and the
//! merged registry rows (mulch-b4bd).
//!
//! Validation errors surface through the CLI's global config-error
//! channel (every store command fails before doing work); the row
//! builder resolves `extends` inheritance parent-first.

use serde_yaml::Value as Yaml;

use crate::ids::{REGISTRY, TypeSpec};

/// Fields owned by the base record: custom types must not redeclare
/// them (reference `BASE_FIELDS`).
const BASE_FIELDS: [&str; 9] = [
    "id",
    "type",
    "classification",
    "recorded_at",
    "evidence",
    "tags",
    "relates_to",
    "supersedes",
    "outcomes",
];

/// The custom-type naming rule (reference `NAMING_RULE`).
const NAMING_RULE: &str =
    "lowercase letters, digits, and underscores; must start with a letter; no hyphens";

/// The valid compact strategies (reference `VALID_COMPACT_STRATEGIES`).
const COMPACT_STRATEGIES: [&str; 4] = ["concat", "merge_outcomes", "keep_latest", "manual"];

/// One custom-type config block, parsed leniently: validation happens
/// in [`validate`], not at parse time (the reference reads the raw
/// object too — wrong shapes become validation errors or JS quirks).
#[derive(Debug, Default)]
pub(crate) struct CustomTypeCfg {
    /// The raw `extends` value: `Some(String)` when a string was
    /// given (empty included), the raw yaml otherwise (the
    /// "must be a non-empty string" law needs the distinction).
    pub(crate) extends_raw:    Option<Yaml>,
    /// Entry list; a bare string is its CHARACTERS (the reference's
    /// `for (const f of "str")` quirk — probe: `required: notanarray`
    /// reports a duplicate `n`).
    pub(crate) required:       Vec<String>,
    pub(crate) optional:       Vec<String>,
    pub(crate) dedup_key:      Option<String>,
    pub(crate) id_key:         Option<String>,
    pub(crate) summary:        Option<String>,
    pub(crate) compact:        Option<String>,
    pub(crate) extracts_files: Option<bool>,
    pub(crate) files_field:    Option<String>,
}

/// A validated custom type's merged registry row (parent inheritance
/// resolved).
#[derive(Debug, PartialEq)]
pub(crate) struct CustomRow {
    pub(crate) name:        String,
    /// Required fields, parent-first then the child's own (write and
    /// schema order).
    pub(crate) required:    Vec<String>,
    /// Optional fields, parent-first minus promoted-to-required.
    pub(crate) optional:    Vec<String>,
    /// The id key field (or `content_hash`).
    pub(crate) id_key:      String,
    /// The dedup field; `None` is the never-matching `content_hash`
    /// mode.
    pub(crate) dedup:       Option<String>,
    /// The field that is a string array instead of a plain string
    /// (`extracts_files` resolved with `files_field`).
    pub(crate) files_array: Option<String>,
}

/// The `custom_types` entries in config order. The reference iterates
/// `Object.entries(customTypes)`: a mapping yields its pairs, a STRING
/// or SEQUENCE yields `(index, element)` pairs (probe:
/// `custom_types: oops` names the type `"0"`), and `null`/absence
/// yields nothing.
pub(crate) fn entries(custom_types: Option<&Yaml>) -> Vec<(String, CustomTypeCfg)> {
    let Some(value) = custom_types else {
        return Vec::new();
    };
    let mut out = Vec::new();
    match value {
        Yaml::Mapping(mapping) => {
            for (key, cfg) in mapping {
                let name = yaml_key_string(key);
                out.push((name, cfg_from_yaml(cfg)));
            }
        }
        Yaml::String(text) => {
            for (index, ch) in text.chars().enumerate() {
                out.push((index.to_string(), string_cfg(&ch.to_string())));
            }
        }
        Yaml::Sequence(items) => {
            for (index, item) in items.iter().enumerate() {
                out.push((index.to_string(), cfg_from_yaml(item)));
            }
        }
        _ => {}
    }
    out
}

/// A scalar entry's config: every field lookup on a primitive is
/// `undefined` in JS — the default config.
fn string_cfg(_scalar: &str) -> CustomTypeCfg {
    CustomTypeCfg::default()
}

/// The config block for one entry; unknown key types read as the
/// reference's `??` defaults.
fn cfg_from_yaml(value: &Yaml) -> CustomTypeCfg {
    let Yaml::Mapping(mapping) = value else {
        return CustomTypeCfg::default();
    };
    let get = |key: &str| mapping.get(Yaml::String(key.to_string()));
    let field_list = |key: &str| -> Vec<String> {
        match get(key) {
            // The JS for..of quirk: a bare string iterates its chars.
            Some(Yaml::String(text)) => text.chars().map(|ch| ch.to_string()).collect(),
            Some(Yaml::Sequence(items)) => items
                .iter()
                .map(|item| match item {
                    Yaml::String(text) => text.clone(),
                    other => yaml_display(other),
                })
                .collect(),
            _ => Vec::new(),
        }
    };
    CustomTypeCfg {
        extends_raw:    get("extends").cloned(),
        required:       field_list("required"),
        optional:       field_list("optional"),
        dedup_key:      get("dedup_key").and_then(yaml_string),
        id_key:         get("id_key").and_then(yaml_string),
        summary:        get("summary").and_then(yaml_string),
        compact:        get("compact").and_then(yaml_string),
        extracts_files: get("extracts_files").and_then(|value| match value {
            Yaml::Bool(flag) => Some(*flag),
            _ => None,
        }),
        files_field:    get("files_field").and_then(yaml_string),
    }
}

/// The reference's validation law for one custom type
/// (`validateCustomTypeConfig`) — error messages byte-exact.
pub(crate) fn validate(name: &str, cfg: &CustomTypeCfg) -> Result<(), String> {
    if !valid_custom_name(name) {
        return Err(format!(
            "Invalid custom_types name \"{name}\" ({}). Allowed: {NAMING_RULE}.",
            describe_name_violation(name)
        ));
    }
    if REGISTRY.iter().any(|spec| spec.name == name) {
        return Err(format!(
            "Custom type \"{name}\" shadows a built-in record type. Reserved names: {}.",
            REGISTRY.map(|spec| spec.name).join(", ")
        ));
    }

    let parent: Option<&'static TypeSpec> = match &cfg.extends_raw {
        None => None,
        Some(Yaml::String(extends)) if !extends.is_empty() => {
            match REGISTRY.iter().find(|spec| spec.name == extends) {
                Some(spec) => Some(spec),
                None => {
                    return Err(format!(
                        "Custom type \"{name}\" extends \"{extends}\" must be a built-in type. Built-ins: {}. Custom-from-custom inheritance is not supported in v1.",
                        REGISTRY.map(|spec| spec.name).join(", ")
                    ));
                }
            }
        }
        Some(_) => {
            return Err(format!(
                "Custom type \"{name}\" extends must be a non-empty string."
            ));
        }
    };

    if parent.is_none() && cfg.required.is_empty() {
        return Err(format!(
            "Custom type \"{name}\" must declare a non-empty \"required\" array (or set \"extends\" to inherit from a built-in)."
        ));
    }

    // childSeen tracks the child's own declarations; mergedSeen adds
    // the parent's fields.
    let mut child_seen: Vec<&str> = Vec::new();
    let mut merged_seen: Vec<&str> = parent.map_or_else(Vec::new, |spec| {
        spec.payload
            .iter()
            .chain(spec.optional.iter())
            .copied()
            .collect()
    });

    for field in &cfg.required {
        if field.is_empty() {
            return Err(format!(
                "Custom type \"{name}\" has an invalid required field: {field:?}."
            ));
        }
        if !valid_custom_name(field) {
            return Err(format!(
                "Custom type \"{name}\" required field \"{field}\" ({}). Allowed: {NAMING_RULE}.",
                describe_name_violation(field)
            ));
        }
        if BASE_FIELDS.contains(&field.as_str()) {
            return Err(format!(
                "Custom type \"{name}\" cannot declare base field \"{field}\" as required (base fields: {}).",
                BASE_FIELDS.join(", ")
            ));
        }
        if child_seen.contains(&field.as_str()) {
            return Err(format!(
                "Custom type \"{name}\" has duplicate required field \"{field}\"."
            ));
        }
        child_seen.push(field);
        if !merged_seen.contains(&field.as_str()) {
            merged_seen.push(field);
        }
    }
    for field in &cfg.optional {
        if field.is_empty() {
            return Err(format!(
                "Custom type \"{name}\" has an invalid optional field: {field:?}."
            ));
        }
        if !valid_custom_name(field) {
            return Err(format!(
                "Custom type \"{name}\" optional field \"{field}\" ({}). Allowed: {NAMING_RULE}.",
                describe_name_violation(field)
            ));
        }
        if BASE_FIELDS.contains(&field.as_str()) {
            return Err(format!(
                "Custom type \"{name}\" cannot declare base field \"{field}\" as optional."
            ));
        }
        if child_seen.contains(&field.as_str()) {
            return Err(format!(
                "Custom type \"{name}\" has field \"{field}\" in both required and optional."
            ));
        }
        child_seen.push(field);
        if !merged_seen.contains(&field.as_str()) {
            merged_seen.push(field);
        }
    }

    let effective_dedup: Option<String> = cfg
        .dedup_key
        .clone()
        .or_else(|| parent.map(|spec| spec.id_key.to_string()));
    let Some(effective_dedup) = effective_dedup else {
        return Err(format!(
            "Custom type \"{name}\" must set \"dedup_key\" (field name or \"content_hash\"), or extend a built-in that defines one."
        ));
    };
    if effective_dedup != "content_hash" && !merged_seen.contains(&effective_dedup.as_str()) {
        return Err(format!(
            "Custom type \"{name}\" dedup_key \"{effective_dedup}\" must be declared in required/optional or be \"content_hash\"."
        ));
    }
    if let Some(id_key) = &cfg.id_key
        && id_key != "content_hash"
        && !merged_seen.contains(&id_key.as_str())
    {
        return Err(format!(
            "Custom type \"{name}\" id_key \"{id_key}\" must be declared in required/optional or be \"content_hash\"."
        ));
    }

    if cfg.summary.is_none() && parent.is_none() {
        return Err(format!(
            "Custom type \"{name}\" must set \"summary\" template string."
        ));
    }
    if let Some(summary) = &cfg.summary
        && summary.is_empty()
    {
        return Err(format!(
            "Custom type \"{name}\" \"summary\" must be a non-empty string."
        ));
    }
    if let Some(summary) = &cfg.summary {
        let mut valid_tokens: Vec<&str> = BASE_FIELDS.to_vec();
        valid_tokens.extend(merged_seen.iter().copied());
        for token in template_tokens(summary) {
            if !valid_tokens.contains(&token.as_str()) {
                let mut known = valid_tokens.clone();
                known.sort_unstable();
                return Err(format!(
                    "Custom type \"{name}\" summary references unknown field \"{{{token}}}\". Declared fields: {}.",
                    known.join(", ")
                ));
            }
        }
    }

    if let Some(compact) = &cfg.compact
        && !COMPACT_STRATEGIES.contains(&compact.as_str())
    {
        return Err(format!(
            "Custom type \"{name}\" compact strategy \"{compact}\" is invalid. Use one of: {}.",
            COMPACT_STRATEGIES.join(", ")
        ));
    }
    if cfg.extracts_files == Some(true)
        && let Some(files_field) = &cfg.files_field
        && !merged_seen.contains(&files_field.as_str())
    {
        return Err(format!(
            "Custom type \"{name}\" files_field \"{files_field}\" must be declared in required/optional when extracts_files: true."
        ));
    }

    Ok(())
}

/// Builds the merged row for a VALIDATED config entry.
pub(crate) fn build_row(name: &str, cfg: &CustomTypeCfg) -> CustomRow {
    let parent = cfg
        .extends_raw
        .as_ref()
        .and_then(|raw| raw.as_str())
        .and_then(|extends| REGISTRY.iter().find(|spec| spec.name == extends));

    let mut required: Vec<String> = parent.map_or_else(Vec::new, |spec| {
        spec.payload.iter().map(|f| (*f).to_string()).collect()
    });
    for field in &cfg.required {
        if !required.contains(field) {
            required.push(field.clone());
        }
    }
    let mut optional: Vec<String> = parent.map_or_else(Vec::new, |spec| {
        spec.optional.iter().map(|f| (*f).to_string()).collect()
    });
    for field in &cfg.optional {
        if !optional.contains(field) {
            optional.push(field.clone());
        }
    }
    optional.retain(|field| !required.contains(field));

    let dedup: Option<String> = cfg
        .dedup_key
        .clone()
        .or_else(|| parent.map(|spec| spec.id_key.to_string()))
        .filter(|key| key != "content_hash");
    let id_key = cfg
        .id_key
        .clone()
        .or_else(|| cfg.dedup_key.clone())
        .or_else(|| parent.map(|spec| spec.id_key.to_string()))
        .unwrap_or_else(|| "content_hash".to_string());

    let extracts = cfg
        .extracts_files
        .or(parent.map(|spec| spec.files.is_some()));
    let files_field = cfg
        .files_field
        .clone()
        .or_else(|| parent.and_then(|spec| spec.files.map(str::to_string)))
        .unwrap_or_else(|| "files".to_string());
    let files_array = extracts.filter(|extracts| *extracts).map(|_| files_field);

    CustomRow {
        name: name.to_string(),
        required,
        optional,
        id_key,
        dedup,
        files_array,
    }
}

/// The custom-name rule `^[a-z][a-z0-9_]*$`.
fn valid_custom_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// The reference's `describeNameViolation` order.
fn describe_name_violation(value: &str) -> &'static str {
    if value.is_empty() {
        "must not be empty"
    } else if value.contains('-') {
        "contains a hyphen"
    } else if value.starts_with(|c: char| c.is_ascii_digit()) {
        "starts with a digit"
    } else if !value.starts_with(|c: char| c.is_ascii_lowercase()) {
        "must start with a lowercase letter"
    } else if value.contains(|c: char| c.is_ascii_uppercase()) {
        "contains uppercase letters"
    } else if !value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        "contains a character outside [a-z0-9_]"
    } else {
        "violates the naming rule"
    }
}

/// The reference's template tokens (`{{field}}` preferred over
/// `{field}`, token charset `[a-z_][a-z0-9_]*`).
fn template_tokens(template: &str) -> Vec<String> {
    let bytes = template.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{'
            && i + 1 < bytes.len()
            && bytes[i + 1] == b'{'
            && let Some((token, next)) = read_token(template, i + 2, "}}")
        {
            tokens.push(token);
            i = next;
            continue;
        }
        if bytes[i] == b'{'
            && let Some((token, next)) = read_token(template, i + 1, "}")
        {
            tokens.push(token);
            i = next;
            continue;
        }
        i += 1;
    }
    tokens
}

/// Reads one `[a-z_][a-z0-9_]*` token starting at `start` and
/// requiring `end` after it.
fn read_token(template: &str, start: usize, end: &str) -> Option<(String, usize)> {
    let mut chars = template[start..].char_indices();
    let mut token = String::new();
    if let Some((_, first)) = chars.next() {
        if !(first.is_ascii_lowercase() || first == '_') {
            return None;
        }
        token.push(first);
    } else {
        return None;
    }
    for (offset, ch) in chars {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' {
            token.push(ch);
        } else {
            let stop = start + offset;
            return template[stop..].starts_with(end).then(|| {
                let next = stop + end.len();
                (token, next)
            });
        }
    }
    None
}

/// A mapping key as a string (non-string keys render YAML-ish).
fn yaml_key_string(key: &Yaml) -> String {
    match key {
        Yaml::String(text) => text.clone(),
        other => yaml_display(other),
    }
}

fn yaml_string(value: &Yaml) -> Option<String> {
    match value {
        Yaml::String(text) => Some(text.clone()),
        _ => None,
    }
}

/// Scalars stringify like JSON-ish JS; complex keys do not occur in
/// practice (a bare scalar name fails the naming rule anyway).
fn yaml_display(value: &Yaml) -> String {
    match value {
        Yaml::Null => "null".into(),
        Yaml::Bool(flag) => flag.to_string(),
        Yaml::Number(number) => number.to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(yaml: &str) -> CustomTypeCfg {
        let value: Yaml = serde_yaml::from_str(yaml).expect("yaml");
        cfg_from_yaml(&value)
    }

    fn validate_err(name: &str, yaml: &str) -> String {
        validate(name, &cfg(yaml)).expect_err("must fail")
    }

    #[test]
    fn name_violations_carry_the_reference_wording() {
        assert_eq!(
            validate_err("0bad", "required: [x]\ndedup_key: x\nsummary: \"{x}\""),
            "Invalid custom_types name \"0bad\" (starts with a digit). Allowed: lowercase letters, digits, and underscores; must start with a letter; no hyphens."
        );
        assert!(
            validate_err("Bad", "required: [x]\ndedup_key: x\nsummary: \"{x}\"")
                .contains("must start with a lowercase letter")
        );
        assert!(
            validate_err("a-b", "required: [x]\ndedup_key: x\nsummary: \"{x}\"")
                .contains("contains a hyphen")
        );
    }

    #[test]
    fn shadowing_and_extends_laws_match() {
        assert_eq!(
            validate_err("pattern", "required: [x]\ndedup_key: x\nsummary: \"{x}\""),
            "Custom type \"pattern\" shadows a built-in record type. Reserved names: convention, pattern, failure, decision, reference, guide."
        );
        assert_eq!(
            validate_err("cc", "extends: cc\nrequired: []"),
            "Custom type \"cc\" extends \"cc\" must be a built-in type. Built-ins: convention, pattern, failure, decision, reference, guide. Custom-from-custom inheritance is not supported in v1."
        );
        assert_eq!(
            validate_err("cc", "extends: \"\"\nrequired: []"),
            "Custom type \"cc\" extends must be a non-empty string."
        );
    }

    #[test]
    fn field_rules_match() {
        assert!(
            validate_err("t", "required: [type]\ndedup_key: x\nsummary: \"{x}\"")
                .contains("cannot declare base field \"type\" as required")
        );
        assert!(
            validate_err("t", "required: [x, x]\ndedup_key: x\nsummary: \"{x}\"")
                .contains("duplicate required field \"x\"")
        );
        assert!(
            validate_err(
                "t",
                "required: [x]\noptional: [x]\ndedup_key: x\nsummary: \"{x}\""
            )
            .contains("in both required and optional")
        );
    }

    #[test]
    fn dedup_and_id_key_laws_match() {
        assert_eq!(
            validate_err("t", "required: [x]\nsummary: \"{x}\""),
            "Custom type \"t\" must set \"dedup_key\" (field name or \"content_hash\"), or extend a built-in that defines one."
        );
        assert!(
            validate_err("t", "required: [x]\ndedup_key: zz\nsummary: \"{x}\"")
                .contains("dedup_key \"zz\" must be declared")
        );
        assert!(
            validate_err(
                "t",
                "required: [x]\ndedup_key: x\nid_key: q\nsummary: \"{x}\""
            )
            .contains("id_key \"q\" must be declared")
        );
    }

    #[test]
    fn summary_token_law_matches() {
        // The declared-fields list is sorted lexicographically and
        // includes the base fields (probe-pinned).
        assert_eq!(
            validate_err("t", "required: [x]\ndedup_key: x\nsummary: \"{nope}\""),
            "Custom type \"t\" summary references unknown field \"{nope}\". Declared fields: classification, evidence, id, outcomes, recorded_at, relates_to, supersedes, tags, type, x."
        );
        // extends replaces the summary requirement; the parent's
        // fields stay valid tokens.
        assert!(validate("t", &cfg("extends: pattern\nrequired: []")).is_ok());
    }

    #[test]
    fn a_string_required_iterates_its_characters() {
        // The JS for..of quirk: `required: notanarray` reports the
        // duplicate `n` (probe-pinned).
        assert!(
            validate_err("t", "required: notanarray")
                .contains("has duplicate required field \"n\"")
        );
    }

    #[test]
    fn entries_treat_a_string_custom_types_as_indexed_chars() {
        let value: Yaml = serde_yaml::from_str("oops").expect("yaml");
        let entries = entries(Some(&value));
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].0, "0");
        // every entry is the default config (field lookups on a char
        // are undefined)
        assert_eq!(entries[0].1.required, Vec::<String>::new());
    }

    #[test]
    fn build_row_merges_parent_first_and_resolves_keys() {
        let row = build_row(
            "t",
            &cfg("extends: pattern\nrequired: [extra]\noptional: [files]\ndedup_key: extra"),
        );
        assert_eq!(row.required, ["name", "description", "extra"]);
        // `files` promoted to required by inheritance order? No — it
        // stays optional (parent optional, child optional, not
        // required); the promoted case is required listing.
        assert_eq!(row.optional, ["files"]);
        assert_eq!(row.id_key, "extra");
        assert_eq!(row.dedup.as_deref(), Some("extra"));
        // parent extracts-files semantics inherit
        assert_eq!(row.files_array.as_deref(), Some("files"));

        let content = build_row(
            "t",
            &cfg("required: [body]\ndedup_key: content_hash\nsummary: \"{body}\""),
        );
        assert_eq!(content.dedup, None);
        assert_eq!(content.id_key, "content_hash");
    }

    #[test]
    fn template_tokens_prefer_double_braces() {
        assert_eq!(template_tokens("{{a}} {b} {no_} {9x} {}"), vec![
            "a", "b", "no_"
        ]);
    }
}
