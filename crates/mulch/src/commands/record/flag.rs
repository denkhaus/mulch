//! The `record` flag path: building the canonical record from the
//! CLI flags over the merged registry (the reference's
//! `buildRecordFromOptions` law, mulch-d524).

use std::fmt::Write as _;

use mulch::{js_string, record_id, schema, truthy_str};
use serde_json::{Map, Value};

use crate::cli::RecordArgs;
use crate::commands::PayloadFlags;
use crate::output::Failure;

/// Builds the canonical JSONL object for the flag path (field order
/// pinned from the reference: type, classification, recorded_at,
/// evidence?, tags?, relates_to?, supersedes?, outcomes?,
/// dir_anchors?, payload…, files?, id).
pub(super) fn build_record(
    registry: &mulch::Registry,
    args: &RecordArgs,
    recorded_at: &str,
    evidence: Option<Map<String, Value>>,
) -> Result<Map<String, Value>, Failure> {
    let record_type = args
        .record_type
        .as_deref()
        .expect("run rejects the flag path without --type (mulch-cbe2)")
        .to_string();
    let row = registry
        .row(&record_type)
        .expect("clap gates --type to the registry");
    let payload: &[String] = &row.payload;
    let flags = PayloadFlags::from_record(args, &record_type);

    // The reference's collectField law: a flag value OR the
    // positional fallback counts only when non-empty — `value ===
    // undefined || value === ""` is missing (required) or dropped
    // (optional) there (mulch-1209, mulch-e2cf).
    let present = |field: &str| flags.resolved(field).is_some_and(|value| !value.is_empty());
    let missing: Vec<&str> = payload
        .iter()
        .map(String::as_str)
        .filter(|field| !present(field))
        .collect();
    if !missing.is_empty() {
        return Err(missing_flags_failure(&record_type, &missing, args, &flags));
    }
    // The optional fields collect after the required set (reference
    // collectField's second loop): present-and-non-empty values
    // store, everything else drops.
    let optional: &[String] = &row.optional;

    let mut record = Map::new();
    record.insert("type".into(), Value::String(record_type.clone()));
    record.insert(
        "classification".into(),
        Value::String(
            args.classification
                .clone()
                .unwrap_or_else(|| "tactical".into()),
        ),
    );
    record.insert("recorded_at".into(), Value::String(recorded_at.into()));
    if let Some(evidence) = evidence {
        record.insert("evidence".into(), Value::Object(evidence));
    }
    // The base list props store only non-empty splits (reference
    // `base.tags.length > 0` gate): an empty flag value, a comma-only
    // value, or all-whitespace items mean the key is absent
    // (mulch-1209).
    if let Some(tags) = non_empty_list(args.tags.as_deref()) {
        record.insert("tags".into(), strings_value(&tags));
    }
    if let Some(relates) = non_empty_list(args.relates_to.as_deref()) {
        record.insert("relates_to".into(), strings_value(&relates));
    }
    if let Some(supersedes) = non_empty_list(args.supersedes.as_deref()) {
        record.insert("supersedes".into(), strings_value(&supersedes));
    }
    // outcomes precede dir_anchors (reference buildRecordFromOptions
    // key order, record.ts:82-86 — probe-pinned by the sprint-6 spec
    // review). The entry itself (key order + strict duration parse)
    // lives in the lib seam.
    if let Some(status) = &args.outcome_status {
        let entry = mulch::OutcomeEntry {
            timestamped: false,
            status,
            now: "",
            duration: args.outcome_duration.as_deref(),
            duration_flag: "--outcome-duration",
            agent: args.outcome_agent.as_deref(),
            notes: None,
            test_results: args.outcome_test_results.as_deref(),
        }
        .build()
        .map_err(|message| Failure::handled_on_stderr("record", format!("Error: {message}")))?;
        record.insert("outcomes".into(), Value::Array(vec![Value::Object(entry)]));
    }
    if !args.dir_anchors.is_empty() {
        let anchors: Vec<Value> = args
            .dir_anchors
            .iter()
            .map(|a| Value::String(a.trim_end_matches('/').into()))
            .collect();
        record.insert("dir_anchors".into(), Value::Array(anchors));
    }
    // collectField over `def.required ∪ def.optional`: the extracts-
    // files field splits its comma string into an array (builtins and
    // customs alike — the row carries the resolved `files_array`);
    // every other field stores the plain string (mulch-d524).
    for field in payload.iter().chain(optional.iter()) {
        let Some(value) = truthy_str(flags.resolved(field)) else {
            continue;
        };
        if row.files_array.as_deref() == Some(field.as_str()) {
            let files = split_list(Some(value)).unwrap_or_default();
            record.insert(field.as_str().into(), strings_value(&files));
        } else {
            record.insert(field.as_str().into(), Value::String(value.to_string()));
        }
    }
    // Git-context auto-populate for extracts-files types whose field
    // stayed absent (reference buildRecordFromOptions: `getContextFiles`
    // — staged plus unstaged diff names, deduped and sorted; absent
    // outside a git repository). Builtins ride the same law: a
    // pattern without --files picks up the changed files too
    // (mulch-d524, probed).
    if let Some(files_field) = &row.files_array
        && !record.contains_key(files_field.as_str())
    {
        let context = context_files();
        if !context.is_empty() {
            record.insert(files_field.as_str().into(), strings_value(&context));
        }
    }

    // Reference-list pattern validation runs on the built record and
    // surfaces the reference's schema-validation blob (probe 3).
    if let schema::FullVerdict::Invalid { subs, hint } =
        schema::full_verdict(registry, &Value::Object(record.clone()))
    {
        return Err(ref_validation_failure(&subs, &hint));
    }

    let id_key_value =
        truthy_str(flags.resolved(registry.id_key_field(&record_type))).unwrap_or_default();
    let id = record_id(&record_type, id_key_value);
    record.insert("id".into(), Value::String(id));
    Ok(record)
}

/// The reference-pattern failure: plain renders as multi-line
/// `record failed schema validation` with the Hint line; the json
/// envelope says `Schema validation failed: <joined>. <hint>`.
fn ref_validation_failure(subs: &[schema::SubError], hint: &str) -> Failure {
    let rendered = schema::render_subs(subs);
    let mut message = String::from("Error: record failed schema validation:");
    for sub in &rendered {
        let _ = write!(message, "\n  {sub}");
    }
    let _ = write!(message, "\n{hint}");
    let mut failure = Failure::handled("record", message);
    let joined = rendered.join(schema::SUB_SEP);
    failure.envelope["error"] =
        Value::String(format!("{}{joined}. {hint}", schema::VALIDATION_PREFIX));
    failure
}

/// The missing-required-flag failure with the reference's Retry hint
/// (buildRetryCommand, mulch-2153): the echo mirrors the reference's
/// argument order — the positional content first (bare,
/// JSON.stringify-quoted), `--type` and `--classification` raw
/// (classification only when it differs from tactical), the six
/// payload flag pairs, then `--files` (the raw comma string), one
/// `--dir-anchor` per anchor, `--tags`, the evidence pairs
/// (`--evidence-commit` unquoted, the rest JSON.stringify), and the
/// missing payload pairs last. Empty flag values are not echoed (JS
/// truthiness); the positional is echoed as a bare argument, never
/// as a `--description` pair.
fn missing_flags_failure(
    record_type: &str,
    missing: &[&str],
    args: &RecordArgs,
    flags: &PayloadFlags<'_>,
) -> Failure {
    // The missing flag hyphenates its field name; the placeholder
    // keeps the underscores (reference collectField + buildRetryCommand:
    // `--code-name "<code_name>"`, probed mulch-d524).
    let missing_flags: Vec<String> = missing
        .iter()
        .map(|field| format!("--{}", field.replace('_', "-")))
        .collect();
    // The echo promotes THIS binary: the reference's buildRetryCommand
    // hardcodes its own name (`ml record`), we say `mulch record` — the
    // user-facing surface never sends anyone to the retired binary
    // (user directive 2026-10-13, README DEVIATIONS; the battery
    // normalizes the name and pins ours explicitly).
    let mut echo = format!("mulch record {}", args.domain);
    if let Some(content) = truthy_str(args.content.as_deref()) {
        let _ = write!(echo, " {}", js_string(content));
    }
    let _ = write!(echo, " --type {record_type}");
    if let Some(classification) = args
        .classification
        .as_deref()
        .filter(|value| !value.is_empty() && *value != "tactical")
    {
        let _ = write!(echo, " --classification {classification}");
    }
    // buildRetryCommand's payload pair order, not the registry's.
    for field in [
        "name",
        "content",
        "description",
        "resolution",
        "title",
        "rationale",
    ] {
        if let Some(pair) = flags.retry_pair(field) {
            let _ = write!(echo, " {pair}");
        }
    }
    if let Some(files) = truthy_str(args.files.as_deref()) {
        let _ = write!(echo, " --files {}", js_string(files));
    }
    for anchor in &args.dir_anchors {
        let _ = write!(echo, " --dir-anchor {}", js_string(anchor));
    }
    if let Some(tags) = truthy_str(args.tags.as_deref()) {
        let _ = write!(echo, " --tags {}", js_string(tags));
    }
    if let Some(commit) = truthy_str(args.evidence_commit.as_deref()) {
        let _ = write!(echo, " --evidence-commit {commit}");
    }
    for (key, value) in [
        ("issue", &args.evidence_issue),
        ("file", &args.evidence_file),
        ("bead", &args.evidence_bead),
        ("seeds", &args.evidence_seeds),
        ("gh", &args.evidence_gh),
        ("linear", &args.evidence_linear),
    ] {
        if let Some(value) = truthy_str(value.as_deref()) {
            let _ = write!(echo, " --evidence-{key} {}", js_string(value));
        }
    }
    for (flag, field) in missing_flags.iter().zip(missing) {
        let _ = write!(echo, " {flag} \"<{field}>\"");
    }
    Failure::handled(
        "record",
        format!(
            "Error: {} records are missing required flag(s): {}.\n  Retry: {echo}",
            record_type,
            missing_flags.join(", ")
        ),
    )
}

/// Comma list split (raw): `""` and comma-only values split to an
/// EMPTY list.
fn split_list(raw: Option<&str>) -> Option<Vec<String>> {
    raw.map(|text| {
        text.split(',')
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect()
    })
}

/// The base list props' gate (reference `base.tags.length > 0`):
/// absent, empty, or comma-only values store no key.
fn non_empty_list(raw: Option<&str>) -> Option<Vec<String>> {
    split_list(raw).filter(|list| !list.is_empty())
}

/// A JSON array of strings.
fn strings_value(items: &[String]) -> Value {
    Value::Array(items.iter().map(|i| Value::String(i.clone())).collect())
}

/// The reference's `getContextFiles` (utils/git-context.ts): staged
/// plus unstaged changed file names, deduped and sorted; no git
/// repository (or git failure) means no files. Untracked files never
/// appear — the source is `git diff --name-only [--cached]`.
fn context_files() -> Vec<String> {
    let mut files: Vec<String> = Vec::new();
    for staged in [true, false] {
        // each call is independent (the reference's two try/catch
        // blocks): a failed staged read keeps whatever the unstaged
        // one collected
        let mut command = std::process::Command::new("git");
        command.args(["diff", "--name-only"]);
        if staged {
            command.arg("--cached");
        }
        let Ok(output) = command.output() else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        let Ok(text) = String::from_utf8(output.stdout) else {
            continue;
        };
        for file in text.lines().filter(|line| !line.is_empty()) {
            if !files.iter().any(|seen| seen == file) {
                files.push(file.to_string());
            }
        }
    }
    files.sort();
    files
}

/// Auto-populated git evidence: the full HEAD sha when cwd is a git
/// repository (reference probe: full 40-char sha; absent outside git).
fn git_head() -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|sha| sha.trim().to_string())
}

/// Builds the evidence object: user flags, git commit fallback.
pub(super) fn evidence_map(args: &RecordArgs) -> Option<Map<String, Value>> {
    let mut evidence = Map::new();
    let pairs: [(&str, &Option<String>); 7] = [
        ("commit", &args.evidence_commit),
        ("issue", &args.evidence_issue),
        ("file", &args.evidence_file),
        ("bead", &args.evidence_bead),
        ("seeds", &args.evidence_seeds),
        ("gh", &args.evidence_gh),
        ("linear", &args.evidence_linear),
    ];
    // JS truthiness (mulch-1209): an empty evidence flag is absent —
    // dropped from the object, and the git-commit fallback triggers
    // for `--evidence-commit ""` too (`if (!options.evidenceCommit)`).
    for (key, value) in pairs {
        if let Some(value) = truthy_str(value.as_deref()) {
            evidence.insert(key.into(), Value::String(value.to_string()));
        }
    }
    if !evidence.contains_key("commit")
        && let Some(sha) = git_head()
    {
        evidence.insert("commit".into(), Value::String(sha));
    }
    (!evidence.is_empty()).then_some(evidence)
}
