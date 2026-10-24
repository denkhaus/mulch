//! The `custom_types` config block: entries parsing, the reference
//! validation law (message-exact, `registry/custom.ts`), and the
//! merged registry rows (mulch-b4bd).
//!
//! Validation errors surface through the CLI's global config-error
//! channel (every store command fails before doing work); the row
//! builder resolves `extends` inheritance parent-first.

use serde_yaml::Value as Yaml;

use crate::ids::{REGISTRY, SummaryLaw, SummaryPart, TypeSpec};

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
    /// Raw `dedup_key`/`id_key`/`summary`/`compact` values: the
    /// validation law distinguishes present-but-wrong-typed from
    /// absent (probe: `dedup_key: ''` reads "must set", `summary: 123`
    /// reads "must be a non-empty string").
    pub(crate) dedup_key:      Option<Yaml>,
    pub(crate) id_key:         Option<Yaml>,
    pub(crate) summary:        Option<Yaml>,
    pub(crate) compact:        Option<Yaml>,
    pub(crate) extracts_files: Option<bool>,
    pub(crate) files_field:    Option<String>,
    /// Raw `section_title` value: present-not-nullish wins as its
    /// display string (JS `??` — probe: `section_title: 123` renders
    /// `### 123`, `""` renders `### `), absent falls to the parent's
    /// builtin title, then `Capitalize(name)+"s"`.
    pub(crate) section_title:  Option<Yaml>,
    /// Raw `aliases` value: the map shape, the entry arrays, and the
    /// legacy-name rules are validation law (probe matrix
    /// 2026-10-17, mulch-b1cb).
    pub(crate) aliases:        Option<Yaml>,
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
            for (index, _) in text.chars().enumerate() {
                // every field lookup on a primitive is `undefined` in
                // JS — the default config
                out.push((index.to_string(), CustomTypeCfg::default()));
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
        section_title:  get("section_title").cloned(),
        required:       field_list("required"),
        optional:       field_list("optional"),
        dedup_key:      get("dedup_key").cloned(),
        id_key:         get("id_key").cloned(),
        summary:        get("summary").cloned(),
        compact:        get("compact").cloned(),
        extracts_files: get("extracts_files").and_then(|value| match value {
            Yaml::Bool(flag) => Some(*flag),
            _ => None,
        }),
        files_field:    get("files_field").and_then(yaml_string),
        aliases:        get("aliases").cloned(),
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

    // The parent lookup's own error laws live here (the shared
    // `parent_spec` helper is the silent variant for build_row).
    if let Some(raw) = &cfg.extends_raw {
        match raw {
            Yaml::String(extends) if !extends.is_empty() => {
                if parent_spec(cfg).is_none() {
                    return Err(format!(
                        "Custom type \"{name}\" extends \"{extends}\" must be a built-in type. Built-ins: {}. Custom-from-custom inheritance is not supported in v1.",
                        REGISTRY.map(|spec| spec.name).join(", ")
                    ));
                }
            }
            _ => {
                return Err(format!(
                    "Custom type \"{name}\" extends must be a non-empty string."
                ));
            }
        }
    }
    let parent = parent_spec(cfg);

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

    // The dedup law distinguishes absent (parent fallback), and
    // present-but-empty-or-wrong-typed: both read "must set"
    // (the reference's typeof/length gate).
    let effective_dedup = effective_dedup(cfg, parent);
    if !yaml_nonempty_string(effective_dedup.as_ref()) {
        return Err(format!(
            "Custom type \"{name}\" must set \"dedup_key\" (field name or \"content_hash\"), or extend a built-in that defines one."
        ));
    }
    let dedup_key = effective_dedup
        .as_ref()
        .map_or_else(String::new, yaml_display);
    if dedup_key != "content_hash" && !merged_seen.contains(&dedup_key.as_str()) {
        return Err(format!(
            "Custom type \"{name}\" dedup_key \"{dedup_key}\" must be declared in required/optional or be \"content_hash\"."
        ));
    }
    if let Some(id_key) = &cfg.id_key {
        let id_key = yaml_display(id_key);
        if id_key != "content_hash" && !merged_seen.contains(&id_key.as_str()) {
            return Err(format!(
                "Custom type \"{name}\" id_key \"{id_key}\" must be declared in required/optional or be \"content_hash\"."
            ));
        }
    }

    if cfg.summary.is_none() && parent.is_none() {
        return Err(format!(
            "Custom type \"{name}\" must set \"summary\" template string."
        ));
    }
    if let Some(summary) = &cfg.summary {
        if !yaml_nonempty_string(Some(summary)) {
            return Err(format!(
                "Custom type \"{name}\" \"summary\" must be a non-empty string."
            ));
        }
        let summary = yaml_display(summary);
        {
            let mut valid_tokens: Vec<&str> = BASE_FIELDS.to_vec();
            valid_tokens.extend(merged_seen.iter().copied());
            for token in template_tokens(&summary) {
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
    }

    if let Some(compact) = &cfg.compact {
        let compact = yaml_display(compact);
        if !COMPACT_STRATEGIES.contains(&compact.as_str()) {
            return Err(format!(
                "Custom type \"{name}\" compact strategy \"{compact}\" is invalid. Use one of: {}.",
                COMPACT_STRATEGIES.join(", ")
            ));
        }
    }
    if cfg.extracts_files == Some(true)
        && let Some(files_field) = &cfg.files_field
        && !merged_seen.contains(&files_field.as_str())
    {
        return Err(format!(
            "Custom type \"{name}\" files_field \"{files_field}\" must be declared in required/optional when extracts_files: true."
        ));
    }

    if let Some(aliases) = &cfg.aliases {
        validate_aliases(name, aliases, &merged_seen)?;
    }

    Ok(())
}

/// The `aliases` block law (reference `validateCustomTypeConfig`'s
/// aliases section): canonical keys must be declared, entries are
/// non-empty arrays of legacy names obeying the custom-name rule, and
/// a legacy name collides with nothing — base fields, foreign
/// builtin-type fields (unless declared here), current fields, or a
/// second canonical owner. Errors byte-exact (probe matrix
/// 2026-10-17).
fn validate_aliases(name: &str, aliases: &Yaml, merged_seen: &[&str]) -> Result<(), String> {
    let Yaml::Mapping(mapping) = aliases else {
        return Err(format!(
            "Custom type \"{name}\" aliases must be a map of canonical field name → legacy alias names."
        ));
    };
    let builtins = builtin_fields();
    let mut seen: Vec<String> = Vec::new();
    for (key, entry) in mapping {
        let canonical = yaml_key_string(key);
        if !merged_seen.contains(&canonical.as_str()) {
            return Err(format!(
                "Custom type \"{name}\" aliases key \"{canonical}\" must be declared in required/optional."
            ));
        }
        let empty_entry = || {
            format!(
                "Custom type \"{name}\" aliases entry \"{canonical}\" must be a non-empty array of legacy field names."
            )
        };
        let Yaml::Sequence(legacy_names) = entry else {
            return Err(empty_entry());
        };
        if legacy_names.is_empty() {
            return Err(empty_entry());
        }
        for legacy in legacy_names {
            let legacy_name = match legacy {
                Yaml::String(text) if !text.is_empty() => text.clone(),
                other => {
                    return Err(format!(
                        "Custom type \"{name}\" aliases entry \"{canonical}\" has an invalid legacy name: {}.",
                        json_stringify(other)
                    ));
                }
            };
            if !valid_custom_name(&legacy_name) {
                return Err(format!(
                    "Custom type \"{name}\" aliases legacy name \"{legacy_name}\" ({}). Allowed: {NAMING_RULE}.",
                    describe_name_violation(&legacy_name)
                ));
            }
            if BASE_FIELDS.contains(&legacy_name.as_str()) {
                return Err(format!(
                    "Custom type \"{name}\" aliases legacy name \"{legacy_name}\" collides with a base field."
                ));
            }
            if builtins.contains(&legacy_name.as_str())
                && !merged_seen.contains(&legacy_name.as_str())
            {
                return Err(format!(
                    "Custom type \"{name}\" aliases legacy name \"{legacy_name}\" collides with a built-in type's field. Use a name that isn't already owned by another type, or set it as required/optional on this type if the field really belongs here."
                ));
            }
            if merged_seen.contains(&legacy_name.as_str()) {
                return Err(format!(
                    "Custom type \"{name}\" aliases legacy name \"{legacy_name}\" is already declared as a current field; aliases are for retired names only."
                ));
            }
            if seen.contains(&legacy_name) {
                return Err(format!(
                    "Custom type \"{name}\" aliases legacy name \"{legacy_name}\" appears under multiple canonical fields; each alias maps to one canonical name."
                ));
            }
            seen.push(legacy_name);
        }
    }
    Ok(())
}

/// The fields owned by built-in types (reference `BUILTIN_FIELDS`):
/// an alias legacy name must not swallow a foreign type's field.
fn builtin_fields() -> Vec<&'static str> {
    let mut fields: Vec<&'static str> = REGISTRY
        .iter()
        .flat_map(|spec| spec.payload.iter().chain(spec.optional.iter()))
        .copied()
        .collect();
    fields.sort_unstable();
    fields.dedup();
    fields
}

/// JS `JSON.stringify` for yaml values (the invalid-legacy-name
/// rendering): strings quoted via [`crate::js_string`], scalars
/// bare, containers in JSON shape.
fn json_stringify(value: &Yaml) -> String {
    match value {
        Yaml::Null => "null".into(),
        Yaml::Bool(flag) => flag.to_string(),
        Yaml::Number(number) => number.to_string(),
        Yaml::String(text) => crate::js_string(text),
        Yaml::Sequence(items) => {
            let parts: Vec<String> = items.iter().map(json_stringify).collect();
            format!("[{}]", parts.join(","))
        }
        Yaml::Mapping(mapping) => {
            let parts: Vec<String> = mapping
                .iter()
                .map(|(key, value)| format!("{}:{}", json_stringify(key), json_stringify(value)))
                .collect();
            format!("{{{}}}", parts.join(","))
        }
        Yaml::Tagged(tagged) => json_stringify(&tagged.value),
    }
}

/// Builds the merged registry row for a VALIDATED config entry
/// (`ids::Row` directly — `builtin: false`, named iff a field dedup
/// key exists; the parallel `CustomRow` shape folded away,
/// mulch-b9d4).
pub(crate) fn build_row(name: &str, cfg: &CustomTypeCfg) -> crate::ids::Row {
    let parent = parent_spec(cfg);

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

    let dedup = match effective_dedup(cfg, parent)
        .as_ref()
        .map_or_else(String::new, yaml_display)
    {
        dedup if dedup == "content_hash" || dedup.is_empty() => None,
        field => Some(field),
    };
    let id_key = cfg
        .id_key
        .clone()
        .or_else(|| effective_dedup(cfg, parent))
        .as_ref()
        .map_or_else(|| "content_hash".to_string(), yaml_display);

    let extracts = cfg
        .extracts_files
        .or(parent.map(|spec| spec.files.is_some()));
    let files_field = cfg
        .files_field
        .clone()
        .or_else(|| parent.and_then(|spec| spec.files.map(str::to_string)))
        .unwrap_or_else(|| "files".to_string());
    let files_array = extracts.filter(|extracts| *extracts).map(|_| files_field);

    let summary = match &cfg.summary {
        Some(raw) if yaml_nonempty_string(Some(raw)) => {
            SummaryLaw::Template(compile_summary(&yaml_display(raw)))
        }
        // extends without an own summary: the parent builtin's field
        // law (its summary fn over its own field).
        _ => parent.map_or(
            SummaryLaw::Field {
                field:    "content".into(),
                truncate: false,
            },
            |spec| SummaryLaw::Field {
                field:    spec.id_key.to_string(),
                truncate: matches!(spec.name, "convention" | "failure"),
            },
        ),
    };

    // Config-declared order preserved (the read-time canonicalization
    // appends canonical keys in this order when a record only carries
    // legacy names).
    let aliases = cfg.aliases.as_ref().map_or_else(Vec::new, |value| {
        let Yaml::Mapping(mapping) = value else {
            return Vec::new();
        };
        mapping
            .iter()
            .map(|(key, entry)| {
                let names = match entry {
                    Yaml::Sequence(items) => items
                        .iter()
                        .map(|item| match item {
                            Yaml::String(text) => text.clone(),
                            other => yaml_display(other),
                        })
                        .collect(),
                    _ => Vec::new(),
                };
                (yaml_key_string(key), names)
            })
            .collect()
    });

    // `cfg.section_title ?? parent?.sectionTitle ?? Capitalize(name)+"s"`
    // (custom.ts `defaultSectionTitle`: only char 0 uppercased,
    // underscores kept, naive +s — probe: `status` → `Statuss`).
    let mut chars = name.chars();
    let default_title = match chars.next() {
        Some(first) => format!("{}{}s", first.to_uppercase(), chars.as_str()),
        None => "s".to_string(),
    };
    let section_title = cfg
        .section_title
        .as_ref()
        .filter(|raw| !matches!(raw, Yaml::Null))
        .map(yaml_display)
        .or_else(|| parent.map(|spec| spec.section_title.to_string()))
        .unwrap_or(default_title);

    // `compact ?? parent?.compact ?? "manual"` (custom.ts); the
    // value was validated in [`validate`], so a non-string raw is
    // unreachable here.
    let compact = cfg
        .compact
        .as_ref()
        .map(|raw| match yaml_display(raw).as_str() {
            "concat" => crate::ids::CompactStrategy::Concat,
            "keep_latest" => crate::ids::CompactStrategy::KeepLatest,
            "merge_outcomes" => crate::ids::CompactStrategy::MergeOutcomes,
            _ => crate::ids::CompactStrategy::Manual,
        })
        .or(parent.map(|spec| spec.compact))
        .unwrap_or(crate::ids::CompactStrategy::Manual);

    crate::ids::Row {
        name: name.to_string(),
        section_title,
        payload: required,
        optional,
        id_key,
        dedup: dedup.clone(),
        named: dedup.is_some(),
        builtin: false,
        files_array,
        aliases,
        summary,
        compact,
    }
}

/// The parent builtin spec for `extends` (validated configs only —
/// `None` for absent extends).
fn parent_spec(cfg: &CustomTypeCfg) -> Option<&'static TypeSpec> {
    cfg.extends_raw
        .as_ref()
        .and_then(|raw| raw.as_str())
        .filter(|extends| !extends.is_empty())
        .and_then(|extends| REGISTRY.iter().find(|spec| spec.name == extends))
}

/// The effective dedup key: the config's raw value, falling back to
/// the parent's (builtins key on their id key).
fn effective_dedup(cfg: &CustomTypeCfg, parent: Option<&'static TypeSpec>) -> Option<Yaml> {
    cfg.dedup_key
        .clone()
        .or_else(|| parent.map(|spec| Yaml::String(spec.id_key.to_string())))
}

/// Compiles a summary template into literal/field parts
/// (`{{field}}` preferred over `{field}`).
fn compile_summary(template: &str) -> Vec<SummaryPart> {
    let bytes = template.as_bytes();
    let mut parts = Vec::new();
    let mut literal = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let mut matched = None;
        if bytes[i] == b'{'
            && i + 1 < bytes.len()
            && bytes[i + 1] == b'{'
            && let Some((token, next)) = read_token(template, i + 2, "}}")
        {
            matched = Some((token, next));
        }
        if matched.is_none()
            && bytes[i] == b'{'
            && let Some((token, next)) = read_token(template, i + 1, "}")
        {
            matched = Some((token, next));
        }
        if let Some((token, next)) = matched {
            if !literal.is_empty() {
                parts.push(SummaryPart::Literal(std::mem::take(&mut literal)));
            }
            parts.push(SummaryPart::Field(token));
            i = next;
        } else {
            let ch = template[i..].chars().next().expect("in bounds");
            literal.push(ch);
            i += ch.len_utf8();
        }
    }
    if !literal.is_empty() {
        parts.push(SummaryPart::Literal(literal));
    }
    parts
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

/// Reads one `[a-zA-Z_][a-zA-Z0-9_]*` token starting at `start` and
/// requiring `end` after it (the reference TOKEN_RE carries the `gi`
/// flag — case-insensitive extraction; the DECLARED-FIELDS check then
/// rejects unknown casings with the reference message).
fn read_token(template: &str, start: usize, end: &str) -> Option<(String, usize)> {
    let mut chars = template[start..].char_indices();
    let mut token = String::new();
    if let Some((_, first)) = chars.next() {
        if !(first.is_ascii_alphabetic() || first == '_') {
            return None;
        }
        token.push(first);
    } else {
        return None;
    }
    for (offset, ch) in chars {
        if ch.is_ascii_alphabetic() || ch.is_ascii_digit() || ch == '_' {
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

/// Whether the value is a non-empty string (the reference's
/// `typeof x === "string" && x.length > 0` gates).
fn yaml_nonempty_string(value: Option<&Yaml>) -> bool {
    matches!(value, Some(Yaml::String(text)) if !text.is_empty())
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
        Yaml::String(text) => text.clone(),
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
    fn json_stringify_renders_like_the_reference() {
        // JSON.stringify shapes for the invalid-legacy-name message
        // (probed vs ml 0.10.7, 2026-10-17).
        assert_eq!(
            json_stringify(&serde_yaml::from_str::<Yaml>("123").expect("yaml")),
            "123"
        );
        assert_eq!(
            json_stringify(&serde_yaml::from_str::<Yaml>("1.5").expect("yaml")),
            "1.5"
        );
        assert_eq!(
            json_stringify(&serde_yaml::from_str::<Yaml>("null").expect("yaml")),
            "null"
        );
        assert_eq!(
            json_stringify(&serde_yaml::from_str::<Yaml>("true").expect("yaml")),
            "true"
        );
        assert_eq!(json_stringify(&Yaml::String(String::new())), "\"\"");
        assert_eq!(
            json_stringify(&serde_yaml::from_str::<Yaml>("[x, 2]").expect("yaml")),
            "[\"x\",2]"
        );
        assert_eq!(
            json_stringify(&serde_yaml::from_str::<Yaml>("{a: 1}").expect("yaml")),
            "{\"a\":1}"
        );
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
        assert_eq!(row.payload, ["name", "description", "extra"]);
        // the child redeclares `files` optional — it stays optional
        // (parent optional, child optional)
        assert_eq!(row.optional, ["files"]);
        assert_eq!(row.id_key, "extra");
        assert_eq!(row.dedup.as_deref(), Some("extra"));
        // parent extracts-files semantics inherit
        assert_eq!(row.files_array.as_deref(), Some("files"));
        assert!(row.named);
        assert!(!row.builtin);

        let content = build_row(
            "t",
            &cfg("required: [body]\ndedup_key: content_hash\nsummary: \"{body}\""),
        );
        assert_eq!(content.dedup, None);
        assert_eq!(content.id_key, "content_hash");
        assert!(!content.named);
    }

    #[test]
    fn template_tokens_prefer_double_braces() {
        assert_eq!(template_tokens("{{a}} {b} {no_} {9x} {}"), vec![
            "a", "b", "no_"
        ]);
        // the `gi` flag: uppercase extracts too (the declared-fields
        // check rejects it with the reference message)
        assert_eq!(template_tokens("{Name}"), vec!["Name"]);
    }

    #[test]
    fn wrong_typed_scalars_read_the_reference_branches() {
        // `dedup_key: ''` is present-but-empty: "must set" (the
        // typeof/length gate), not "must be declared"
        assert!(
            validate_err("t", "required: [x]\ndedup_key: ''").contains("must set \"dedup_key\"")
        );
        // `summary: 123` is present-but-wrong-typed
        assert_eq!(
            validate_err("t", "required: [x]\ndedup_key: x\nsummary: 123"),
            "Custom type \"t\" \"summary\" must be a non-empty string."
        );
        // a non-string dedup_key with extends still reads "must set"
        assert!(
            validate_err("t", "extends: pattern\nrequired: []\ndedup_key: 123")
                .contains("must set \"dedup_key\"")
        );
        // a non-string id_key interpolates its display form
        assert!(
            validate_err("t", "required: [x]\ndedup_key: x\nid_key: 123")
                .contains("id_key \"123\" must be declared")
        );
    }

    #[test]
    fn summary_law_templates_and_inheritance() {
        let row = build_row(
            "t",
            &cfg("required: [body]\ndedup_key: body\nsummary: \"{body}!\""),
        );
        assert_eq!(
            row.summary,
            SummaryLaw::Template(vec![
                SummaryPart::Field("body".into()),
                SummaryPart::Literal("!".into())
            ])
        );
        // extends without an own summary: the parent's field law
        let inherited = build_row("t", &cfg("extends: convention\nrequired: [extra]"));
        assert_eq!(inherited.summary, SummaryLaw::Field {
            field:    "content".into(),
            truncate: true,
        });
        let pattern_law = build_row("t", &cfg("extends: pattern\nrequired: [extra]"));
        assert_eq!(pattern_law.summary, SummaryLaw::Field {
            field:    "name".into(),
            truncate: false,
        });
    }

    #[test]
    fn child_required_promotes_a_parent_optional_field() {
        let row = build_row(
            "t",
            &cfg("extends: pattern\nrequired: [files]\ndedup_key: name"),
        );
        assert!(row.payload.contains(&"files".to_string()));
        assert!(
            !row.optional.iter().any(|field| field == "files"),
            "promoted fields leave the optional list"
        );
    }
}
