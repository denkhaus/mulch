//! `mulch config` — the read/write surface over
//! `.mulch/mulch.config.yaml` (mulch-176f). Reference:
//! commands/config.ts (ml 0.10.7), probed live 2026-10-26
//! (/tmp/opencode/sprint54A).
//!
//! Channel contract (probe-pinned): `schema` and `show` print their
//! JSON documents on STDOUT unconditionally — `--json` and `--quiet`
//! change nothing (the reference writes through `process.stdout`
//! directly, config.ts:18/40/58); `set`/`unset` succeed SILENTLY.
//! Every handled error is the bare message on STDERR with exit 1 in
//! BOTH plain and json mode — the catch writes `err.message` and
//! never builds an envelope (config.ts:35/75/93), so this module's
//! [`plain_failure`] renders the message itself in json mode.
//!
//! The write path (`set`/`unset`) re-reads, mutates, validates, and
//! atomically rewrites the document as a GENERIC ordered map: the
//! reference's `readConfig` → `setAtPath`/`unsetAtPath` → AJV →
//! `yaml.dump` pipeline over plain JS objects, where key ORDER is
//! file order with backfilled/appended keys (utils/config.ts:252-277,
//! config.ts:99-178). The lib's [`mulch::Config`] is the store's
//! TYPED view (its `to_yaml` re-orders canonically) and is
//! deliberately NOT used here.

// 176f prep: the command lands ahead of its dispatch wiring — the
// host applies the cli.rs/mod.rs patch (see /tmp/sprint54/A).
#![allow(
    dead_code,
    reason = "176f prep: dispatch wiring lands with the host patch"
)]

use std::path::{Path, PathBuf};

use serde_yaml::Mapping;

use crate::cli::GlobalOpts;
use crate::output::{EXIT_ERROR, Failure, print_json};

/// The MulchConfig JSON Schema, pinned byte-for-byte from
/// `ml config schema` (ml 0.10.7, 23144 bytes). `schema` prints it
/// verbatim; every walk over it (`--path` fallbacks, the set/unset
/// gates, the validator's error titles) parses this same fixture so
/// the schema ships once.
const SCHEMA_FIXTURE: &str = include_str!("../../fixtures/config/schema.json");

/// The parsed [`SCHEMA_FIXTURE`], built on first use (parse of a
/// committed fixture — the expect is a build-time invariant).
fn schema() -> &'static serde_json::Value {
    static SCHEMA: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    SCHEMA.get_or_init(|| {
        serde_json::from_str(SCHEMA_FIXTURE).expect("the pinned config schema parses")
    })
}

/// The `config` subcommand surface (reference commander tree,
/// config.ts:11-97). clap wording for help/errors is the documented
/// deviation family (README DEVIATIONS); the flag/argument shapes
/// match the reference.
#[derive(Debug, clap::Subcommand)]
pub(crate) enum ConfigSub {
    /// Emit MulchConfig JSON Schema for warren and other config-UI
    /// consumers.
    Schema,

    /// Emit the effective MulchConfig as JSON. Pass --path to read a
    /// single knob; falls back to the schema default when the knob is
    /// unset.
    Show {
        /// Dot-notation path to a single knob (e.g.
        /// governance.max_entries, search.boost_factor).
        #[arg(long)]
        path: Option<String>,
    },

    /// Set a config knob via dot-notation path. <value> is
    /// YAML-parsed; the resulting config is validated against the
    /// schema before an atomic write under the writer lock.
    Set {
        /// Dot-notation path (e.g. governance.max_entries,
        /// search.boost_factor, domains.warren.allowed_types).
        path: String,

        /// YAML-parsed value to set at <path>.
        // allow_hyphen_values: commander passes `-1`-shaped values
        // through to the YAML parse (probed); clap would abort
        // pre-dispatch.
        #[arg(allow_hyphen_values = true)]
        value: String,
    },

    /// Remove a config knob via dot-notation path so subsequent reads
    /// fall back to the schema default. Idempotent — unsetting a
    /// never-set path is a silent no-op.
    Unset {
        /// Dot-notation path (e.g. search.boost_factor, domains.warren,
        /// hooks.pre-record).
        path: String,
    },
}

/// Runs `config` (all four subcommands).
pub(crate) fn run(opts: &GlobalOpts, sub: &ConfigSub) -> Result<(), Failure> {
    match sub {
        ConfigSub::Schema => {
            // Raw stdout, one trailing newline, no gates (config.ts:18).
            #[allow(clippy::print_stdout, reason = "rendering is this command's job")]
            {
                print!("{SCHEMA_FIXTURE}");
            }
            Ok(())
        }
        ConfigSub::Show { path } => show(opts, path.as_deref()),
        ConfigSub::Set { path, value } => set(opts, path, value),
        ConfigSub::Unset { path } => unset(opts, path),
    }
}

// ---- the shared no-envelope error law ----

/// The config error shape: the bare message on stderr, exit 1, in
/// BOTH modes (the reference's catch prints `err.message`; json mode
/// gets no envelope, config.ts:35/75/93). In json mode the message is
/// rendered here and `rendered` silences the dispatcher.
fn plain_failure(opts: &GlobalOpts, message: impl Into<String>) -> Failure {
    let message = message.into();
    let mut failure = Failure {
        message:            message.clone(),
        code:               EXIT_ERROR,
        envelope:           serde_json::Value::Null,
        envelope_to_stderr: true,
        rendered:           false,
    };
    if opts.json {
        #[allow(clippy::print_stderr, reason = "error rendering is the CLI boundary")]
        {
            eprintln!("{message}");
        }
        failure.rendered = true;
    }
    failure
}

// ---- config show ----

/// `config show`: the effective config as JSON, or one knob with the
/// schema-default fallback (config.ts:21-59).
fn show(opts: &GlobalOpts, path: Option<&str>) -> Result<(), Failure> {
    let effective = read_effective(&config_path()?)
        .map_err(|source| plain_failure(opts, render_read_error(&source, false)))?;
    let json = yaml_to_json(&serde_yaml::Value::Mapping(effective));
    let Some(path) = path else {
        print_json(&json, false);
        return Ok(());
    };
    let segments = split_path(path);
    if segments.is_empty() {
        return Err(plain_failure(opts, "--path must not be empty."));
    }
    let value = match walk_config(&json, &segments) {
        Some(found) => Some(found.clone()),
        None => walk_schema_default(&segments),
    };
    match value {
        Some(value) => {
            print_json(&value, false);
            Ok(())
        }
        None => Err(plain_failure(
            opts,
            format!("Path '{path}' not found in config and has no schema default."),
        )),
    }
}

// ---- config set / config unset ----

/// `config set` (config.ts:61-78, 99-140): empty-path gate, store
/// gate, schema-path gate, value YAML parse, then the locked
/// read→mutate→validate→write cycle.
fn set(opts: &GlobalOpts, path: &str, value: &str) -> Result<(), Failure> {
    let segments = split_path(path);
    if segments.is_empty() {
        return Err(plain_failure(opts, "<path> must not be empty."));
    }
    let config_path = store_gate(opts, "set")?;
    validate_path_in_schema(&segments).map_err(|message| plain_failure(opts, message))?;
    let parsed: serde_yaml::Value = serde_yaml::from_str(value)
        .map_err(|source| plain_failure(opts, format!("Invalid YAML for <value>: {source}")))?;
    let parsed = normalize_yaml_numbers(parsed);

    let _writer_lock = lock_store(opts, &config_path)?;
    let mut doc = read_effective(&config_path)
        .map_err(|source| plain_failure(opts, render_read_error(&source, false)))?;
    set_at_path(&mut doc, &segments, parsed);
    validate_mutation(opts, &doc, "set")?;
    write_atomic(&config_path, &dump_yaml(&doc))
        .map_err(|source| plain_failure(opts, format!("writing config: {source}")))?;
    Ok(())
}

/// `config unset` (config.ts:80-96, 142-178): the same gates, then the
/// locked read→remove→validate→write cycle. A never-set path is a
/// SILENT no-op — the file is not rewritten (probe-pinned).
fn unset(opts: &GlobalOpts, path: &str) -> Result<(), Failure> {
    let segments = split_path(path);
    if segments.is_empty() {
        return Err(plain_failure(opts, "<path> must not be empty."));
    }
    let config_path = store_gate(opts, "unset")?;
    validate_path_in_schema(&segments).map_err(|message| plain_failure(opts, message))?;

    let _writer_lock = lock_store(opts, &config_path)?;
    let mut doc = read_effective(&config_path)
        .map_err(|source| plain_failure(opts, render_read_error(&source, false)))?;
    if !unset_at_path(&mut doc, &segments) {
        return Ok(());
    }
    validate_mutation(opts, &doc, "unset")?;
    write_atomic(&config_path, &dump_yaml(&doc))
        .map_err(|source| plain_failure(opts, format!("writing config: {source}")))?;
    Ok(())
}

/// The set/unset store gate: the config FILE must exist (the
/// reference checks `existsSync(configPath)`, config.ts:106/149) — a
/// `.mulch/` without a config fails the same way as no `.mulch/` at
/// all. The trailing clause promotes this binary (the reference
/// itself already says `mulch init` here; its `ml config set` becomes
/// `mulch config set` under the promote law).
fn store_gate(opts: &GlobalOpts, verb: &str) -> Result<PathBuf, Failure> {
    let config_path = current_dir_config(opts)?;
    if config_path.is_file() {
        return Ok(config_path);
    }
    Err(plain_failure(
        opts,
        format!(
            "No .mulch/ directory found. Run `mulch init` to set up this project before `mulch config {verb}`."
        ),
    ))
}

/// The config path under the process cwd (the port's store law — no
/// worktree redirection, like every other command here).
fn current_dir_config(opts: &GlobalOpts) -> Result<PathBuf, Failure> {
    let cwd = std::env::current_dir()
        .map_err(|source| plain_failure(opts, format!("resolving cwd: {source}")))?;
    Ok(cwd.join(".mulch").join("mulch.config.yaml"))
}

/// `config show`'s config path (same law, the show error wording).
fn config_path() -> Result<PathBuf, Failure> {
    let cwd = std::env::current_dir()
        .map_err(|source| Failure::handled("config", format!("resolving cwd: {source}")))?;
    Ok(cwd.join(".mulch").join("mulch.config.yaml"))
}

/// Acquires the store writer lock for a mutation cycle. The reference
/// locks `<config>.lock` (an `O_CREAT | O_EXCL` lockfile); the port's
/// kernel-owned store-wide `.mulch/.lock` is the documented deviation
/// that already covers config mutations (README DEVIATIONS, mulch-228d).
fn lock_store(opts: &GlobalOpts, config_path: &Path) -> Result<mulch::lock::WriterLock, Failure> {
    let store_root = config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    mulch::lock::WriterLock::acquire(&store_root)
        .map_err(|source| plain_failure(opts, crate::output::chain_message(&source)))
}

// ---- readConfig (utils/config.ts:252-277) ----

/// A config read failure at the reference's two wordings.
#[derive(Debug)]
enum ConfigReadError {
    /// ENOENT — readConfig's "no store" message.
    Missing,
    /// A parse failure, rendered with this module's wording (the
    /// parser vocabulary itself is serde's — README DEVIATIONS
    /// family, like the malformed-line reasons).
    Parse(String),
}

/// Renders the read failure for a caller (the in-lock re-read reuses
/// readConfig's own wording — no caller suffix).
fn render_read_error(source: &ConfigReadError, _in_lock: bool) -> String {
    match source {
        ConfigReadError::Missing => {
            "No .mulch/ directory found. Run `mulch init` to set up this project.".into()
        }
        ConfigReadError::Parse(message) => message.clone(),
    }
}

/// readConfig: reads and parses the document, then normalizes it to
/// the EFFECTIVE shape — `domains` normalized to the object map,
/// `governance` and `classification_defaults.shelf_life` backfilled
/// per field over the reference defaults — preserving each key's file
/// position when present and appending when absent (JS assignment
/// order: domains, governance, classification_defaults).
fn read_effective(config_path: &Path) -> Result<Mapping, ConfigReadError> {
    let text = std::fs::read_to_string(config_path).map_err(|source| match source.kind() {
        std::io::ErrorKind::NotFound => ConfigReadError::Missing,
        _ => ConfigReadError::Parse(format!("reading {}: {source}", config_path.display())),
    })?;
    let value: serde_yaml::Value = serde_yaml::from_str(&text).map_err(|source| {
        ConfigReadError::Parse(format!(
            "Failed to parse mulch.config.yaml: {source}. Check the YAML syntax."
        ))
    })?;
    // A null/scalar top level reads as the empty document (readConfig's
    // `if (!parsed || typeof parsed !== "object")` — an ARRAY stays an
    // array in JS and then fails AJV as a non-object; we fold it to the
    // empty map, the same write behavior for every reachable case).
    let mut doc = match value {
        serde_yaml::Value::Mapping(mapping) => mapping,
        _ => Mapping::new(),
    };

    // domains: normalizeDomains — legacy list → bare-object map;
    // non-object → empty map.
    let domains = doc.get(yaml_str("domains")).cloned().map_or_else(
        || serde_yaml::Value::Mapping(Mapping::new()),
        |value| match value {
            serde_yaml::Value::Sequence(items) => {
                let mut map = Mapping::new();
                for item in items {
                    if let Some(name) = item.as_str() {
                        map.insert(yaml_str(name), serde_yaml::Value::Mapping(Mapping::new()));
                    }
                }
                serde_yaml::Value::Mapping(map)
            }
            mapping @ serde_yaml::Value::Mapping(_) => mapping,
            _ => serde_yaml::Value::Mapping(Mapping::new()),
        },
    );
    doc.insert(yaml_str("domains"), domains);

    // governance: {...defaults, ...user} — default key order first,
    // then any extra user keys (JS spread order). A non-mapping user
    // block reads as absent.
    let governance = merge_defaults_block(doc.get(yaml_str("governance")), &[
        ("max_entries", serde_yaml::Value::from(100_u64)),
        ("warn_entries", serde_yaml::Value::from(150_u64)),
        ("hard_limit", serde_yaml::Value::from(200_u64)),
    ]);
    doc.insert(yaml_str("governance"), governance);

    // classification_defaults: REPLACED by {shelf_life: {...defaults,
    // ...user}} — other keys under the block drop from the effective
    // document (applyConfigDefaults assigns a fresh object).
    let shelf = merge_defaults_block(
        doc.get(yaml_str("classification_defaults"))
            .and_then(|value| value.as_mapping())
            .and_then(|block| block.get(yaml_str("shelf_life"))),
        &[
            ("tactical", serde_yaml::Value::from(14_u64)),
            ("observational", serde_yaml::Value::from(30_u64)),
        ],
    );
    let mut defaults = Mapping::new();
    defaults.insert(yaml_str("shelf_life"), shelf);
    doc.insert(
        yaml_str("classification_defaults"),
        serde_yaml::Value::Mapping(defaults),
    );
    Ok(doc)
}

/// One backfilled block: the reference defaults in declaration order,
/// then the user block's values (same keys overwritten in place,
/// extra keys appended in user order — `{...defaults, ...user}`).
fn merge_defaults_block(
    user: Option<&serde_yaml::Value>,
    defaults: &[(&str, serde_yaml::Value)],
) -> serde_yaml::Value {
    let mut merged = Mapping::new();
    for (key, value) in defaults {
        merged.insert(yaml_str(key), value.clone());
    }
    if let Some(user) = user.and_then(serde_yaml::Value::as_mapping) {
        for (key, value) in user {
            merged.insert(key.clone(), value.clone());
        }
    }
    serde_yaml::Value::Mapping(merged)
}

// ---- the document dump ----

/// Serializes the document with serde_yaml in mapping order (js-yaml
/// `yaml.dump(cfg, {lineWidth: -1})`; the serde indentation of nested
/// sequences is the documented config-rewrite deviation, README
/// DEVIATIONS). A trailing newline is included, like both serializers.
fn dump_yaml(doc: &Mapping) -> String {
    serde_yaml::to_string(&serde_yaml::Value::Mapping(doc.clone()))
        .expect("YAML mapping serialization is infallible")
}

/// writeFileAtomic (config.ts:317-330): write a `<file>.tmp.<unique>`
/// sibling, rename over the target, unlink the temp on failure.
fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    let unique = format!(
        "{:x}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |span| span.as_nanos()),
        std::process::id()
    );
    let tmp = path.with_file_name(format!(
        "{}.tmp.{unique}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("mulch.config.yaml")
    ));
    std::fs::write(&tmp, contents)?;
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(source) => {
            std::fs::remove_file(&tmp).ok();
            Err(source)
        }
    }
}

// ---- path + value manipulation (config.ts:180-276) ----

/// The path split law: `rawPath.split(".").filter(non-empty)` —
/// `governance..max_entries` reaches the same knob as the plain path.
fn split_path(path: &str) -> Vec<String> {
    path.split('.')
        .filter(|seg| !seg.is_empty())
        .map(String::from)
        .collect()
}

/// setAtPath (config.ts:264-276): descend creating missing
/// intermediates; a non-object (null/scalar/array) intermediate is
/// REPLACED by a fresh map; the leaf insert keeps an existing key's
/// position or appends.
fn set_at_path(node: &mut Mapping, segments: &[String], value: serde_yaml::Value) {
    let key = segments[0].as_str();
    if segments.len() == 1 {
        node.insert(yaml_str(key), value);
        return;
    }
    if !node.get(key).is_some_and(serde_yaml::Value::is_mapping) {
        node.insert(yaml_str(key), serde_yaml::Value::Mapping(Mapping::new()));
    }
    let Some(child) = node
        .get_mut(key)
        .and_then(serde_yaml::Value::as_mapping_mut)
    else {
        return;
    };
    set_at_path(child, &segments[1..], value);
}

/// unsetAtPath (config.ts:213-240): false when any intermediate is a
/// non-object or the leaf is absent (the idempotent no-op); otherwise
/// removes the leaf — order-preservingly — and prunes empty ancestors
/// walking up, stopping at an ancestor that is `required` in its
/// parent's schema node.
fn unset_at_path(root: &mut Mapping, segments: &[String]) -> bool {
    unset_walk(root, segments, &[])
}

/// The recursion behind [`unset_at_path`]: `node` is the document at
/// `prefix`, `segments` the remaining path. The prune's required-check
/// needs the ABSOLUTE prefix (config.ts:236 passes
/// `segments.slice(0, i - 1)` of the full path), hence the threaded
/// parameter.
fn unset_walk(node: &mut Mapping, segments: &[String], prefix: &[String]) -> bool {
    let Some(key) = segments.first() else {
        return false;
    };
    let key = key.as_str();
    if segments.len() == 1 {
        if !node.contains_key(key) {
            return false;
        }
        // shift_remove: sibling order survives (JS delete keeps it).
        node.shift_remove(key);
        return true;
    }
    if !node.get(key).is_some_and(serde_yaml::Value::is_mapping) {
        return false;
    }
    let mut child_prefix = prefix.to_vec();
    child_prefix.push(key.to_string());
    let changed = node
        .get_mut(key)
        .and_then(serde_yaml::Value::as_mapping_mut)
        .is_some_and(|child| unset_walk(child, &segments[1..], &child_prefix));
    if !changed {
        return false;
    }
    // Prune law: an empty child goes unless its key is required in
    // THIS node's schema context (the absolute prefix).
    let empty = node
        .get(key)
        .and_then(serde_yaml::Value::as_mapping)
        .is_some_and(Mapping::is_empty);
    if empty && !is_required_in_schema(prefix, key) {
        node.shift_remove(key);
    }
    changed
}

// ---- schema walks (config.ts:180-206, 242-262, 332-381) ----

/// One schema-node walk step: a hit through `properties`, a hit
/// through an `additionalProperties` object schema, or a miss.
enum SchemaStep<'a> {
    Child(&'a serde_json::Value),
    Miss,
}

/// The shared step law (`if (props && hasOwn(props, seg)) … else if
/// (additional && typeof additional === "object") … else miss`). The
/// caller checks the descend-past-a-leaf arm (a non-object node)
/// before stepping.
fn schema_step<'a>(node: &'a serde_json::Value, segment: &str) -> SchemaStep<'a> {
    let Some(object) = node.as_object() else {
        return SchemaStep::Miss;
    };
    if let Some(child) = object
        .get("properties")
        .and_then(|props| props.get(segment))
    {
        return SchemaStep::Child(child);
    }
    if let Some(child) = object
        .get("additionalProperties")
        .filter(|value| value.is_object())
    {
        return SchemaStep::Child(child);
    }
    SchemaStep::Miss
}

/// validatePathInSchema (config.ts:180-206): every segment must step
/// through `properties` or an open-map `additionalProperties`. The
/// descend-past-leaf arm is unreachable for this schema (every node
/// is an object) but implemented per the reference.
fn validate_path_in_schema(segments: &[String]) -> Result<(), String> {
    let mut node = schema();
    for (index, segment) in segments.iter().enumerate() {
        if !node.is_object() {
            return Err(format!(
                "Path '{}' descends past a leaf in the schema.",
                segments[..=index].join(".")
            ));
        }
        match schema_step(node, segment) {
            SchemaStep::Child(child) => node = child,
            SchemaStep::Miss => {
                let known = node
                    .get("properties")
                    .and_then(serde_json::Value::as_object)
                    .map_or_else(
                        || "(none)".to_string(),
                        |props| props.keys().cloned().collect::<Vec<_>>().join(", "),
                    );
                let parent = if index == 0 {
                    "<root>".to_string()
                } else {
                    segments[..index].join(".")
                };
                return Err(format!(
                    "Unknown config path: '{}' is not a known knob. Known keys at '{parent}': {known}.",
                    segments[..=index].join(".")
                ));
            }
        }
    }
    Ok(())
}

/// isRequiredInSchema (config.ts:242-262): walks to the PARENT node
/// and asks its `required` list. A walk miss is "not required".
fn is_required_in_schema(parent_segments: &[String], key: &str) -> bool {
    let mut node = schema();
    for segment in parent_segments {
        match schema_step(node, segment) {
            SchemaStep::Child(child) => node = child,
            SchemaStep::Miss => return false,
        }
    }
    node.get("required")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|required| required.iter().any(|entry| entry.as_str() == Some(key)))
}

/// walkConfig (config.ts:332-340): object-only descent — an array or
/// null intermediate is a miss; a null LEAF is a value (found).
fn walk_config<'a>(
    value: &'a serde_json::Value,
    segments: &[String],
) -> Option<&'a serde_json::Value> {
    let mut node = value;
    for segment in segments {
        let object = node.as_object()?;
        node = object.get(segment)?;
    }
    Some(node)
}

/// walkSchemaDefault (config.ts:349-367): steps the schema like the
/// validator, then synthesizes the node's default.
fn walk_schema_default(segments: &[String]) -> Option<serde_json::Value> {
    let mut node = schema();
    for segment in segments {
        node = match schema_step(node, segment) {
            SchemaStep::Child(child) => child,
            SchemaStep::Miss => return None,
        };
    }
    collect_schema_defaults(node)
}

/// collectSchemaDefaults (config.ts:369-381): the node's declared
/// `default` (by PRESENCE — a 0 default returns 0), else the object
/// of children's defaults (dropping default-less children); empty →
/// None.
fn collect_schema_defaults(node: &serde_json::Value) -> Option<serde_json::Value> {
    let object = node.as_object()?;
    if let Some(default) = object.get("default") {
        return Some(default.clone());
    }
    let properties = object.get("properties")?.as_object()?;
    let mut result = serde_json::Map::new();
    for (key, child) in properties {
        if let Some(value) = collect_schema_defaults(child) {
            result.insert(key.clone(), value);
        }
    }
    if result.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(result))
    }
}

// ---- the validator (config.ts:129-135, 167-173, 278-315) ----

/// One AJV-style validation error.
struct SchemaError {
    /// instancePath, "" for the root (`<root>` when rendered).
    path:    String,
    /// The AJV message template text.
    message: String,
}

/// Validates the post-mutation document against the pinned schema
/// (an AJV subset: type/enum/minimum/maximum/required/properties/
/// additionalProperties/items — every keyword the schema uses) and
/// renders the reference's error block on failure. Error order:
/// type, then enum/bounds, then (for objects) required, properties
/// in schema order, additionalProperties in instance order — the
/// traversal AJV's allErrors walk produced in the probes.
fn validate_mutation(opts: &GlobalOpts, doc: &Mapping, verb: &str) -> Result<(), Failure> {
    let json = yaml_to_json(&serde_yaml::Value::Mapping(doc.clone()));
    let mut errors = Vec::new();
    validate_node(schema(), &json, "", &mut errors);
    if errors.is_empty() {
        return Ok(());
    }
    let lines: Vec<String> = errors.iter().map(render_schema_error).collect();
    Err(plain_failure(
        opts,
        format!("Invalid config after {verb}:\n{}", lines.join("\n")),
    ))
}

/// Renders `formatAjvError` (config.ts:278-283): `  - <path>:
/// <message>[ (<title>)]` — `<root>` for the root path, and NO title
/// for root errors (`lookupSchemaMeta("")` short-circuits).
fn render_schema_error(error: &SchemaError) -> String {
    let path = if error.path.is_empty() {
        "<root>"
    } else {
        &error.path
    };
    let title =
        lookup_schema_meta(&error.path).map_or_else(String::new, |title| format!(" ({title})"));
    format!("  - {path}: {}{title}", error.message)
}

/// lookupSchemaMeta (config.ts:285-315): the `title` of the schema
/// node AT the instance path (walking properties, then open maps).
fn lookup_schema_meta(instance_path: &str) -> Option<&str> {
    if instance_path.is_empty() {
        return None;
    }
    let mut node = schema();
    for segment in instance_path
        .split('/')
        .skip(1)
        .filter(|seg| !seg.is_empty())
    {
        node = match schema_step(node, segment) {
            SchemaStep::Child(child) => child,
            SchemaStep::Miss => return None,
        };
    }
    node.get("title").and_then(serde_json::Value::as_str)
}

/// The recursive AJV-subset walk. A type failure skips the node's
/// remaining keywords (probe-pinned: `governance: 5` reports ONLY
/// `must be object`, no required follow-ups).
fn validate_node(
    node_schema: &serde_json::Value,
    instance: &serde_json::Value,
    path: &str,
    errors: &mut Vec<SchemaError>,
) {
    let Some(object) = node_schema.as_object() else {
        return;
    };
    if let Some(expected) = object.get("type").and_then(serde_json::Value::as_str)
        && !type_matches(expected, instance)
    {
        errors.push(SchemaError {
            path:    path.to_string(),
            message: format!("must be {expected}"),
        });
        return;
    }
    if let Some(allowed) = object.get("enum").and_then(serde_json::Value::as_array)
        && !allowed.contains(instance)
    {
        errors.push(SchemaError {
            path:    path.to_string(),
            message: "must be equal to one of the allowed values".into(),
        });
    }
    match instance {
        serde_json::Value::Number(number) => {
            let value = number.as_f64().unwrap_or_default();
            if let Some(minimum) = object.get("minimum").and_then(serde_json::Value::as_f64)
                && value < minimum
            {
                errors.push(SchemaError {
                    path:    path.to_string(),
                    message: format!("must be >= {minimum}"),
                });
            }
            if let Some(maximum) = object.get("maximum").and_then(serde_json::Value::as_f64)
                && value > maximum
            {
                errors.push(SchemaError {
                    path:    path.to_string(),
                    message: format!("must be <= {maximum}"),
                });
            }
        }
        serde_json::Value::Object(map) => {
            if let Some(required) = object.get("required").and_then(serde_json::Value::as_array) {
                for key in required {
                    if let Some(key) = key.as_str()
                        && !map.contains_key(key)
                    {
                        errors.push(SchemaError {
                            path:    path.to_string(),
                            message: format!("must have required property '{key}'"),
                        });
                    }
                }
            }
            let properties = object
                .get("properties")
                .and_then(serde_json::Value::as_object);
            if let Some(properties) = properties {
                for (key, child_schema) in properties {
                    if let Some(child) = map.get(key) {
                        validate_node(child_schema, child, &format!("{path}/{key}"), errors);
                    }
                }
            }
            match object.get("additionalProperties") {
                Some(serde_json::Value::Bool(false)) => {
                    for key in map.keys() {
                        if properties.is_none_or(|props| !props.contains_key(key)) {
                            errors.push(SchemaError {
                                path:    path.to_string(),
                                message: "must NOT have additional properties".into(),
                            });
                        }
                    }
                }
                Some(open @ serde_json::Value::Object(_)) => {
                    for (key, child) in map {
                        if properties.is_none_or(|props| !props.contains_key(key)) {
                            validate_node(open, child, &format!("{path}/{key}"), errors);
                        }
                    }
                }
                _ => {}
            }
        }
        serde_json::Value::Array(items) => {
            if let Some(item_schema) = object.get("items") {
                for (index, item) in items.iter().enumerate() {
                    validate_node(item_schema, item, &format!("{path}/{index}"), errors);
                }
            }
        }
        _ => {}
    }
}

/// The JSON Schema `type` names the config schema uses (a type ARRAY
/// never appears in the fixture; integer accepts integral floats —
/// JS has no separate integer kind).
fn type_matches(expected: &str, instance: &serde_json::Value) -> bool {
    match expected {
        "object" => instance.is_object(),
        "array" => instance.is_array(),
        "string" => instance.is_string(),
        "boolean" => instance.is_boolean(),
        "number" => instance.is_number(),
        "integer" => instance
            .as_f64()
            .is_some_and(|value| value.fract() == 0.0 && value.is_finite()),
        _ => true,
    }
}

// ---- YAML⇄JSON with the JS-number law ----

/// Converts a YAML value to the JSON view the reference's
/// `JSON.stringify` renders: tag wrappers unwrap, non-string mapping
/// keys stringify, and an INTEGRAL float becomes an integer (JS has
/// one number kind — `1.0` prints `1` both in `config show` and in
/// js-yaml's dump).
fn yaml_to_json(value: &serde_yaml::Value) -> serde_json::Value {
    match value {
        serde_yaml::Value::Null => serde_json::Value::Null,
        serde_yaml::Value::Bool(flag) => serde_json::Value::Bool(*flag),
        serde_yaml::Value::Number(number) => json_number(number),
        serde_yaml::Value::String(text) => serde_json::Value::String(text.clone()),
        serde_yaml::Value::Sequence(items) => {
            serde_json::Value::Array(items.iter().map(yaml_to_json).collect())
        }
        serde_yaml::Value::Mapping(mapping) => {
            let mut object = serde_json::Map::new();
            for (key, value) in mapping {
                // Non-string mapping keys stringify (JS object keys):
                // the JSON view of the key decides the text.
                let json_key = yaml_to_json(key);
                let key = json_key
                    .as_str()
                    .map_or_else(|| mulch::value_text(&json_key), ToString::to_string);
                object.insert(key, yaml_to_json(value));
            }
            serde_json::Value::Object(object)
        }
        serde_yaml::Value::Tagged(tagged) => yaml_to_json(&tagged.value),
    }
}

/// One YAML number as the JSON value JS would hold: integers stay,
/// integral floats collapse to integers (|x| within JS safe range).
fn json_number(number: &serde_yaml::Number) -> serde_json::Value {
    if let Some(integer) = number.as_i64() {
        return serde_json::Value::from(integer);
    }
    if let Some(unsigned) = number.as_u64() {
        return serde_json::Value::from(unsigned);
    }
    let float = number.as_f64().unwrap_or_default();
    if float.is_finite() && float.fract() == 0.0 && float.abs() <= 9_007_199_254_740_992.0 {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the fract/abs guard makes the cast exact (JS safe integer)"
        )]
        {
            serde_json::Value::from(float as i64)
        }
    } else {
        serde_json::Value::from(float)
    }
}

/// The dump-side twin of [`json_number`]: integral floats become
/// integers IN the document, so `yaml.dump` writes `1` where js-yaml
/// writes `1` for the JS number 1.0 (serde_yaml would write `1.0`).
fn normalize_yaml_numbers(value: serde_yaml::Value) -> serde_yaml::Value {
    match value {
        serde_yaml::Value::Number(number) => match json_number(&number) {
            serde_json::Value::Number(json) => {
                serde_yaml::to_value(&json).unwrap_or(serde_yaml::Value::Number(number))
            }
            _ => serde_yaml::Value::Number(number),
        },
        serde_yaml::Value::Sequence(items) => {
            serde_yaml::Value::Sequence(items.into_iter().map(normalize_yaml_numbers).collect())
        }
        serde_yaml::Value::Mapping(mapping) => serde_yaml::Value::Mapping(
            mapping
                .into_iter()
                .map(|(key, value)| (key, normalize_yaml_numbers(value)))
                .collect(),
        ),
        serde_yaml::Value::Tagged(tagged) => {
            let inner = normalize_yaml_numbers(tagged.value);
            serde_yaml::Value::Tagged(Box::new(serde_yaml::value::TaggedValue {
                tag:   tagged.tag,
                value: inner,
            }))
        }
        other => other,
    }
}

/// The `serde_yaml` string key (one owner, like the lib's copy).
fn yaml_str(text: &str) -> serde_yaml::Value {
    serde_yaml::Value::String(text.to_owned())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    /// JSON-mode quiet-law anchor: the schema fixture is the probe
    /// capture verbatim — valid JSON, the draft-07 header first, the
    /// closed root last, one trailing newline (23144 bytes probed).
    #[test]
    fn schema_fixture_is_the_probe_capture() {
        assert!(
            SCHEMA_FIXTURE
                .starts_with("{\n  \"$schema\": \"http://json-schema.org/draft-07/schema#\"")
        );
        assert!(SCHEMA_FIXTURE.ends_with("  \"additionalProperties\": false\n}\n"));
        assert_eq!(SCHEMA_FIXTURE.len(), 23_144);
        serde_json::from_str::<serde_json::Value>(SCHEMA_FIXTURE).expect("parses");
    }

    /// The split law: empty segments drop, so `governance..max_entries`
    /// is `governance.max_entries` and `.`/``` are empty paths
    /// (probed: `show --path governance..max_entries` → 100).
    #[test]
    fn split_path_drops_empty_segments() {
        assert_eq!(split_path("governance..max_entries"), [
            "governance",
            "max_entries"
        ]);
        assert_eq!(split_path("."), Vec::<String>::new());
        assert_eq!(split_path(""), Vec::<String>::new());
    }

    /// The validatePathInSchema table (all probes, 2026-10-26):
    /// known knobs pass at every depth; unknown keys name the parent
    /// and its known keys (root lists all twelve); leaves report
    /// `(none)`; open maps (domains.<name>, custom_types.<name>)
    /// accept every name but stay closed one level deeper.
    #[test]
    fn validate_path_in_schema_matches_probed_errors() {
        for path in [
            "version",
            "governance.max_entries",
            "search.boost_factor",
            "domains.warren.allowed_types",
            "custom_types.hyp.required",
            "hooks.pre-record",
            "hook_settings.timeout_ms",
            "decay.anchor_validity.threshold",
            "audit.thresholds.floater_max",
            "audit.per_domain.warren.max_stale",
            "prime.session_close.custom",
        ] {
            let segments = split_path(path);
            validate_path_in_schema(&segments)
                .unwrap_or_else(|error| panic!("{path} should be known: {error}"));
        }
        assert_eq!(
            validate_path_in_schema(&split_path("governance.typo")).unwrap_err(),
            "Unknown config path: 'governance.typo' is not a known knob. Known keys at 'governance': max_entries, warn_entries, hard_limit."
        );
        assert_eq!(
            validate_path_in_schema(&split_path("bogus")).unwrap_err(),
            "Unknown config path: 'bogus' is not a known knob. Known keys at '<root>': version, domains, governance, classification_defaults, prime, search, custom_types, decay, audit, disabled_types, hooks, hook_settings."
        );
        assert_eq!(
            validate_path_in_schema(&split_path("governance.max_entries.deep")).unwrap_err(),
            "Unknown config path: 'governance.max_entries.deep' is not a known knob. Known keys at 'governance.max_entries': (none)."
        );
        assert_eq!(
            validate_path_in_schema(&split_path("domains.warren.bogus")).unwrap_err(),
            "Unknown config path: 'domains.warren.bogus' is not a known knob. Known keys at 'domains.warren': allowed_types, required_fields."
        );
        assert_eq!(
            validate_path_in_schema(&split_path("hooks.bogus")).unwrap_err(),
            "Unknown config path: 'hooks.bogus' is not a known knob. Known keys at 'hooks': pre-record, post-record, pre-prime, pre-prune, pre-compact."
        );
    }

    /// walkSchemaDefault's recursive law (probed): a leaf returns its
    /// declared default; an object collects its children's defaults
    /// and drops default-less children (`weight` vanishes from
    /// `decay.anchor_validity`, `custom` from `prime.session_close`);
    /// nodes with no defaults at all (custom_types, hooks) are misses.
    #[test]
    fn schema_default_walk_matches_probes() {
        let walk = |path: &str| walk_schema_default(&split_path(path));
        assert_eq!(walk("search.boost_factor"), Some(serde_json::json!(0.1)));
        assert_eq!(
            walk("search"),
            Some(serde_json::json!({"boost_factor": 0.1}))
        );
        assert_eq!(
            walk("decay"),
            Some(serde_json::json!({
                "anchor_validity": {"threshold": 0.5, "grace_days": 7}
            }))
        );
        assert_eq!(
            walk("prime"),
            Some(serde_json::json!({
                "default_mode": "full",
                "tier_weights": {"star": 100, "foundational": 50, "tactical": 20, "observational": 10},
                "session_close": {"style": "conditional"}
            }))
        );
        assert_eq!(
            walk("hook_settings"),
            Some(serde_json::json!({"timeout_ms": 5000}))
        );
        assert_eq!(walk("custom_types"), None);
        assert_eq!(walk("hooks"), None);
        // a 0 default is a PRESENCE hit, not a truthy one
        assert_eq!(
            walk("audit.thresholds.max_stale"),
            Some(serde_json::json!(0))
        );
    }

    /// The readConfig normalization table (probed): file key order
    /// with governance/classification_defaults/domains replaced in
    /// position when present and appended (domains → governance →
    /// classification_defaults) when absent; a legacy domains list
    /// becomes the bare-object map; partial blocks backfill per field.
    #[test]
    fn read_effective_backfills_in_readconfig_order() {
        // The probe store: legacy list domains + partial governance —
        // domains normalizes in position, governance backfills in
        // position, classification_defaults appends.
        let dir = scratch_config(
            "cfg-legacy",
            "domains:\n  - alpha\n  - beta\ngovernance:\n  max_entries: 7\n",
        );
        let doc = read_effective(&dir.join(".mulch").join("mulch.config.yaml")).expect("reads");
        let json = yaml_to_json(&serde_yaml::Value::Mapping(doc));
        assert_eq!(
            json,
            serde_json::json!({
                "domains": {"alpha": {}, "beta": {}},
                "governance": {"max_entries": 7, "warn_entries": 150, "hard_limit": 200},
                "classification_defaults": {"shelf_life": {"tactical": 14, "observational": 30}}
            })
        );
        // Key order: domains, governance, classification_defaults.
        let serde_json::Value::Object(object) = &json else {
            panic!("object");
        };
        assert_eq!(object.keys().collect::<Vec<_>>(), [
            "domains",
            "governance",
            "classification_defaults"
        ]);
        std::fs::remove_dir_all(&dir).ok();

        // A bare `version`-only config appends the three blocks in the
        // JS assignment order.
        let dir = scratch_config("cfg-bare", "version: '1'\n");
        let doc = read_effective(&dir.join(".mulch").join("mulch.config.yaml")).expect("reads");
        let json = yaml_to_json(&serde_yaml::Value::Mapping(doc));
        let serde_json::Value::Object(object) = &json else {
            panic!("object");
        };
        assert_eq!(object.keys().collect::<Vec<_>>(), [
            "version",
            "domains",
            "governance",
            "classification_defaults"
        ]);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The set dump law (probed twice): `set governance.max_entries 5`
    /// then `set search.boost_factor 0.5` on the fresh-init template
    /// rewrite it to exactly these bytes — comments stripped, order
    /// preserved, the new key appended.
    #[test]
    fn set_dump_matches_the_probed_file_bytes() {
        let dir = scratch_init_store("cfg-dump");
        let config_path = dir.join(".mulch").join("mulch.config.yaml");
        let mut doc = read_effective(&config_path).expect("reads");
        set_at_path(
            &mut doc,
            &split_path("governance.max_entries"),
            serde_yaml::Value::from(5),
        );
        let dumped = dump_yaml(&doc);
        assert_eq!(
            dumped,
            "version: '1'\ndomains: {}\ngovernance:\n  max_entries: 5\n  warn_entries: 150\n  hard_limit: 200\nclassification_defaults:\n  shelf_life:\n    tactical: 14\n    observational: 30\n"
        );
        set_at_path(
            &mut doc,
            &split_path("search.boost_factor"),
            yaml_to_yaml(0.5),
        );
        let dumped = dump_yaml(&doc);
        assert_eq!(
            dumped,
            "version: '1'\ndomains: {}\ngovernance:\n  max_entries: 5\n  warn_entries: 150\n  hard_limit: 200\nclassification_defaults:\n  shelf_life:\n    tactical: 14\n    observational: 30\nsearch:\n  boost_factor: 0.5\n"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// setAtPath's replace law: a non-object intermediate (scalar,
    /// array, null) is REPLACED by a fresh map, not descended into.
    #[test]
    fn set_at_path_replaces_non_object_intermediates() {
        let mut doc = Mapping::new();
        doc.insert(yaml_str("search"), serde_yaml::Value::from("oops"));
        set_at_path(
            &mut doc,
            &split_path("search.boost_factor"),
            serde_yaml::Value::from(0.5),
        );
        assert_eq!(
            yaml_to_json(doc.get(yaml_str("search")).expect("present")),
            serde_json::json!({"boost_factor": 0.5})
        );
    }

    /// unsetAtPath's three laws (probed): a set leaf removes and
    /// prunes empty ancestors up to (not through) required keys; a
    /// never-set path returns false (the caller skips the write); a
    /// non-object intermediate is a no-op miss.
    #[test]
    fn unset_at_path_prunes_and_stops_at_required() {
        // prime.tier_weights.star: unset prunes tier_weights AND prime.
        let mut doc = Mapping::new();
        set_at_path(
            &mut doc,
            &split_path("prime.tier_weights.star"),
            serde_yaml::Value::from(5),
        );
        assert!(unset_at_path(
            &mut doc,
            &split_path("prime.tier_weights.star")
        ));
        assert!(doc.get(yaml_str("prime")).is_none(), "prime pruned");

        // governance.max_entries is required in governance: the leaf
        // removes but the parent stays (the VALIDATOR then rejects —
        // pinned in the mutation table below).
        let dir = scratch_init_store("cfg-unset-req");
        let config_path = dir.join(".mulch").join("mulch.config.yaml");
        let mut doc = read_effective(&config_path).expect("reads");
        assert!(unset_at_path(
            &mut doc,
            &split_path("governance.max_entries")
        ));
        assert!(
            doc.get(yaml_str("governance")).is_some(),
            "required parent kept"
        );
        let mut errors = Vec::new();
        let json = yaml_to_json(&serde_yaml::Value::Mapping(doc));
        validate_node(schema(), &json, "", &mut errors);
        assert_eq!(
            errors.iter().map(render_schema_error).collect::<Vec<_>>(),
            ["  - /governance: must have required property 'max_entries' (Governance limits)"]
        );
        std::fs::remove_dir_all(&dir).ok();

        // never-set and non-object-intermediate paths are misses.
        let mut doc = Mapping::new();
        set_at_path(
            &mut doc,
            &split_path("search.boost_factor"),
            serde_yaml::Value::from(0.5),
        );
        assert!(!unset_at_path(
            &mut doc,
            &split_path("decay.anchor_validity.threshold")
        ));
        assert!(!unset_at_path(
            &mut doc,
            &split_path("search.boost_factor.deep")
        ));
        assert!(
            doc.get(yaml_str("search")).is_some(),
            "a miss never touches the document"
        );
    }

    /// The mutation-validator table — every probed `Invalid config
    /// after set/unset` byte line (2026-10-26): wrong types with the
    /// schema title, enum, minimum, required-at-root (no title),
    /// required-in-block (titled), items index paths (no title).
    #[test]
    fn validator_renders_the_probed_error_lines() {
        let check = |set_path: &str, value: serde_json::Value, expected: &[&str]| {
            let mut doc = Mapping::new();
            // The backfilled blocks every real mutation validates
            // against (readConfig already applied them) — BEFORE the
            // set, which may overwrite one of them.
            doc.insert(yaml_str("version"), yaml_str("1"));
            doc.insert(
                yaml_str("domains"),
                serde_yaml::Value::Mapping(Mapping::new()),
            );
            doc.insert(
                yaml_str("governance"),
                serde_yaml::to_value(serde_json::json!({
                    "max_entries": 100, "warn_entries": 150, "hard_limit": 200
                }))
                .expect("serializes"),
            );
            doc.insert(
                yaml_str("classification_defaults"),
                serde_yaml::to_value(serde_json::json!({
                    "shelf_life": {"tactical": 14, "observational": 30}
                }))
                .expect("serializes"),
            );
            set_at_path(
                &mut doc,
                &split_path(set_path),
                serde_yaml::to_value(&value).expect("serializes"),
            );
            let json = yaml_to_json(&serde_yaml::Value::Mapping(doc));
            let mut errors = Vec::new();
            validate_node(schema(), &json, "", &mut errors);
            let lines: Vec<String> = errors.iter().map(render_schema_error).collect();
            assert_eq!(lines, expected, "set {set_path} = {value}");
        };

        check("governance.max_entries", serde_json::json!("abc"), &[
            "  - /governance/max_entries: must be integer (Soft target)",
        ]);
        check("governance.max_entries", serde_json::json!(0), &[
            "  - /governance/max_entries: must be >= 1 (Soft target)",
        ]);
        check("search.boost_factor", serde_json::json!(-1), &[
            "  - /search/boost_factor: must be >= 0 (Confirmation boost factor)",
        ]);
        check("prime.default_mode", serde_json::json!("bogus"), &[
            "  - /prime/default_mode: must be equal to one of the allowed values (Default prime mode)",
        ]);
        check("prime.session_close.style", serde_json::json!("bogus"), &[
            "  - /prime/session_close/style: must be equal to one of the allowed values (Preset style)",
        ]);
        check("governance", serde_json::json!(5), &[
            "  - /governance: must be object (Governance limits)",
        ]);
        check("search", serde_json::json!(5), &[
            "  - /search: must be object (Search tuning)",
        ]);
        check("domains.warren.allowed_types", serde_json::json!(5), &[
            "  - /domains/warren/allowed_types: must be array (Allowed record types)",
        ]);
        check("disabled_types", serde_json::json!(5), &[
            "  - /disabled_types: must be array (Disabled types)",
        ]);
        check("custom_types.hyp.required", serde_json::json!([1]), &[
            "  - /custom_types/hyp/required/0: must be string",
        ]);

        // required-at-root (unset version): `<root>`, no title.
        let doc = read_effective_scratch("version: '1'\ndomains: {}\n");
        let mut doc = doc;
        doc.shift_remove(yaml_str("version"));
        let json = yaml_to_json(&serde_yaml::Value::Mapping(doc));
        let mut errors = Vec::new();
        validate_node(schema(), &json, "", &mut errors);
        assert_eq!(
            errors.iter().map(render_schema_error).collect::<Vec<_>>(),
            ["  - <root>: must have required property 'version'"]
        );
    }

    /// The JS-number law: an integral float collapses to the integer
    /// both in the JSON view and in the dumped YAML (`1.0` shows and
    /// writes as `1`, exactly like JS); a real fraction survives.
    #[test]
    fn integral_floats_collapse_like_js_numbers() {
        assert_eq!(
            yaml_to_json(&serde_yaml::Value::from(1.0_f64)),
            serde_json::json!(1)
        );
        assert_eq!(
            yaml_to_json(&serde_yaml::Value::from(0.5_f64)),
            serde_json::json!(0.5)
        );
        let normalized = normalize_yaml_numbers(serde_yaml::Value::from(2.0_f64));
        assert_eq!(dump_single(&normalized), "search:\n  boost_factor: 2\n");
        let kept = normalize_yaml_numbers(serde_yaml::Value::from(0.5_f64));
        assert_eq!(dump_single(&kept), "search:\n  boost_factor: 0.5\n");
    }

    // ---- test scaffolding ----

    /// A scratch `.mulch` with a hand-written config body.
    fn scratch_config(tag: &str, config: &str) -> PathBuf {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "mulch-config-{tag}-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(dir.join(".mulch")).expect("scratch store");
        std::fs::write(dir.join(".mulch").join("mulch.config.yaml"), config).expect("config");
        dir
    }

    /// The fresh-init template store (the probe's starting bytes).
    fn scratch_init_store(tag: &str) -> PathBuf {
        scratch_config(tag, include_str!("../../fixtures/init/mulch.config.yaml"))
    }

    /// readEffective over a scratch config body.
    fn read_effective_scratch(config: &str) -> Mapping {
        let dir = scratch_config("cfg-inline", config);
        let doc = read_effective(&dir.join(".mulch").join("mulch.config.yaml")).expect("reads");
        std::fs::remove_dir_all(&dir).ok();
        doc
    }

    /// Dumps `{search: {boost_factor: <value>}}` for one number.
    fn dump_single(value: &serde_yaml::Value) -> String {
        let mut inner = Mapping::new();
        inner.insert(yaml_str("boost_factor"), value.clone());
        let mut doc = Mapping::new();
        doc.insert(yaml_str("search"), serde_yaml::Value::Mapping(inner));
        dump_yaml(&doc)
    }

    /// A YAML number from an f64 literal (test sugar).
    fn yaml_to_yaml(value: f64) -> serde_yaml::Value {
        normalize_yaml_numbers(serde_yaml::Value::from(value))
    }
}
