//! `mulch record` — the record-creation surface (flag path, stdin,
//! batch, dry-run).

use std::fmt::Write as _;
use std::io::Read as _;

use serde_json::{Map, Value};

use crate::cli::{GlobalOpts, RecordArgs};
use crate::commands::ids::{id_key_field, record_id};
use crate::commands::{NO_STORE_MESSAGE, StoreLocation, domain_file, locate, now_iso};
use crate::output::{Failure, print_json, print_line, success_envelope};

/// Payload fields per record type, in write order.
pub(super) const PAYLOAD_FIELDS: [(&str, &[&str]); 6] = [
    ("convention", &["content"]),
    ("pattern", &["name", "description"]),
    ("failure", &["description", "resolution"]),
    ("decision", &["title", "rationale"]),
    ("reference", &["name", "description"]),
    ("guide", &["name", "description"]),
];

/// Builds the canonical JSONL object for the flag path (field order
/// pinned from the reference: type, classification, recorded_at,
/// evidence?, tags?, relates_to?, supersedes?, dir_anchors?, outcomes?,
/// payload…, files?, id).
fn build_record(
    args: &RecordArgs,
    recorded_at: &str,
    evidence: Option<Map<String, Value>>,
) -> Result<(Map<String, Value>, String), Failure> {
    let record_type = args
        .record_type
        .clone()
        .unwrap_or_else(|| "convention".into());
    let payload: &[&str] = PAYLOAD_FIELDS
        .iter()
        .find(|(name, _)| name == &record_type)
        .map(|(_, fields)| *fields)
        .expect("type choices are clap-validated");

    let provided = |field: &str| -> Option<String> {
        match field {
            "content" => args.content_flag.clone().or_else(|| args.content.clone()),
            "name" => args.name.clone(),
            "description" => args.description.clone(),
            "resolution" => args.resolution.clone(),
            "title" => args.title.clone(),
            "rationale" => args.rationale.clone(),
            _ => None,
        }
    };

    let missing: Vec<&str> = payload
        .iter()
        .copied()
        .filter(|field| provided(field).is_none())
        .collect();
    if !missing.is_empty() {
        return Err(missing_flags_failure(&record_type, &missing, args));
    }

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
    if let Some(tags) = split_list(args.tags.as_deref()) {
        record.insert("tags".into(), strings_value(&tags));
    }
    if let Some(relates) = split_list(args.relates_to.as_deref()) {
        validate_refs(&relates, "--relates-to")?;
        record.insert("relates_to".into(), strings_value(&relates));
    }
    if let Some(supersedes) = split_list(args.supersedes.as_deref()) {
        validate_refs(&supersedes, "--supersedes")?;
        record.insert("supersedes".into(), strings_value(&supersedes));
    }
    if !args.dir_anchors.is_empty() {
        let anchors: Vec<Value> = args
            .dir_anchors
            .iter()
            .map(|a| Value::String(a.trim_end_matches('/').into()))
            .collect();
        record.insert("dir_anchors".into(), Value::Array(anchors));
    }
    if let Some(outcome) = record_time_outcome(args) {
        let mut outcomes = Map::new();
        outcomes.insert("status".into(), Value::String(outcome.status));
        if let Some(duration) = outcome.duration {
            outcomes.insert("duration".into(), number_value(&duration)?);
        }
        if let Some(test_results) = outcome.test_results {
            outcomes.insert("test_results".into(), Value::String(test_results));
        }
        if let Some(agent) = outcome.agent {
            outcomes.insert("agent".into(), Value::String(agent));
        }
        record.insert(
            "outcomes".into(),
            Value::Array(vec![Value::Object(outcomes)]),
        );
    }
    for field in payload {
        record.insert(
            (*field).into(),
            Value::String(provided(field).unwrap_or_default()),
        );
    }
    if let Some(files) = split_list(args.files.as_deref()) {
        record.insert("files".into(), strings_value(&files));
    }

    let id_key_value = provided(id_key_field(&record_type)).unwrap_or_default();
    let id = record_id(&record_type, &id_key_value);
    record.insert("id".into(), Value::String(id.clone()));
    Ok((record, id))
}

/// Record-time outcome flag bundle.
pub(super) struct OutcomeBundle {
    pub(super) status:       String,
    pub(super) duration:     Option<String>,
    pub(super) test_results: Option<String>,
    pub(super) agent:        Option<String>,
}

fn record_time_outcome(args: &RecordArgs) -> Option<OutcomeBundle> {
    args.outcome_status.as_ref().map(|status| OutcomeBundle {
        status:       status.clone(),
        duration:     args.outcome_duration.clone(),
        test_results: args.outcome_test_results.clone(),
        agent:        args.outcome_agent.clone(),
    })
}

/// The missing-required-flag failure with the reference's Retry hint.
fn missing_flags_failure(record_type: &str, missing: &[&str], args: &RecordArgs) -> Failure {
    let flags: Vec<String> = missing.iter().map(|f| format!("--{f}")).collect();
    let mut echo = format!("ml record {} --type {record_type}", args.domain);
    for field in ["name", "description", "resolution", "title", "rationale"] {
        if let Some(value) = provided_flag(args, field) {
            let _ = write!(echo, " --{field} \"{value}\"");
        }
    }
    for flag in &flags {
        let hint = flag.trim_start_matches("--");
        let _ = write!(echo, " {flag} \"<{hint}>\"");
    }
    Failure::handled(
        "record",
        format!(
            "Error: {} records are missing required flag(s): {}.\n  Retry: {echo}",
            record_type,
            flags.join(", ")
        ),
    )
}

fn provided_flag(args: &RecordArgs, field: &str) -> Option<String> {
    match field {
        "name" => args.name.clone(),
        "description" => args.description.clone(),
        "resolution" => args.resolution.clone(),
        "title" => args.title.clone(),
        "rationale" => args.rationale.clone(),
        _ => None,
    }
}

/// Comma list split.
pub(super) fn split_list(raw: Option<&str>) -> Option<Vec<String>> {
    raw.map(|text| {
        text.split(',')
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect()
    })
}

/// A JSON array of strings.
pub(super) fn strings_value(items: &[String]) -> Value {
    Value::Array(items.iter().map(|i| Value::String(i.clone())).collect())
}

/// Numeric outcome duration.
pub(super) fn number_value(raw: &str) -> Result<Value, Failure> {
    raw.parse::<u64>()
        .map(Value::from)
        .map_err(|_| Failure::handled("record", format!("invalid number: {raw}")))
}

/// Reference-id shape check (`^([a-z0-9-]+:)?mx-[0-9a-f]{4,8}$`).
fn validate_refs(items: &[String], flag: &str) -> Result<(), Failure> {
    for item in items {
        let valid = match item.strip_prefix("mx-") {
            Some(hex) => ref_hex_ok(hex),
            None => match item.split_once(":mx-") {
                Some((prefix, hex)) => {
                    !prefix.is_empty()
                        && prefix
                            .chars()
                            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
                        && ref_hex_ok(hex)
                }
                None => false,
            },
        };
        if !valid {
            return Err(Failure::handled(
                "record",
                format!("Invalid record reference '{item}' for {flag}"),
            ));
        }
    }
    Ok(())
}

/// Lowercase hex, 4..=8 chars.
fn ref_hex_ok(hex: &str) -> bool {
    (4..=8).contains(&hex.len())
        && hex
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
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
fn evidence_map(args: &RecordArgs) -> Option<Map<String, Value>> {
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
    for (key, value) in pairs {
        if let Some(value) = value {
            evidence.insert(key.into(), Value::String(value.clone()));
        }
    }
    if !evidence.contains_key("commit")
        && let Some(sha) = git_head()
    {
        evidence.insert("commit".into(), Value::String(sha));
    }
    (!evidence.is_empty()).then_some(evidence)
}

/// Runs `record`.
pub(super) fn run(opts: &GlobalOpts, args: &RecordArgs) -> Result<(), Failure> {
    let cwd = std::env::current_dir()
        .map_err(|source| Failure::handled("record", format!("resolving cwd: {source}")))?;
    let mut store = match locate(&cwd) {
        Ok(StoreLocation::Open(store)) => store,
        Ok(_) => {
            let mut failure = Failure::handled("record", NO_STORE_MESSAGE);
            failure.envelope_to_stderr = true;
            return Err(failure);
        }
        Err(source) => {
            return Err(Failure::handled(
                "record",
                crate::output::chain_message(&source),
            ));
        }
    };

    // Auto-create precedes validation (reference behavior: failed
    // records still leave the domain side effects).
    let known = store.domains().iter().any(|d| d == &args.domain);
    let mut auto_created = false;
    if !known && !args.dry_run {
        store
            .add_domain(&args.domain)
            .map_err(|source| Failure::handled("record", crate::output::chain_message(&source)))?;
        auto_created = true;
        // The auto-create line hits stdout even in --json mode.
        print_line(false, &format!("✓ Auto-created domain \"{}\"", args.domain));
    }

    if args.stdin || args.batch.is_some() {
        return stdin_batch(opts, args, &store, auto_created);
    }

    let recorded_at = now_iso();
    let (record, id) = match build_record(args, &recorded_at, evidence_map(args)) {
        Ok(built) => built,
        Err(mut failure) => {
            failure.envelope_to_stderr = true;
            if opts.json {
                let swapped = failure.message.replace("\n  Retry: ", "\n  Example: ");
                failure.envelope["error"] = Value::String(swapped);
            }
            return Err(failure);
        }
    };

    // Duplicate detection: same id already in the domain.
    let file = domain_file(&store.root, &args.domain);
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let duplicate_position = lines.iter().position(|line| {
        serde_json::from_str::<Value>(line)
            .ok()
            .and_then(|r| r.get("id").and_then(Value::as_str).map(String::from))
            .is_some_and(|existing| existing == id)
    });
    if let Some(position) = duplicate_position
        && !args.force
    {
        print_line(
            false,
            &format!(
                "Duplicate {} already exists in {} (record #{}). Use --force to add anyway.",
                args.record_type
                    .clone()
                    .unwrap_or_else(|| "convention".into()),
                args.domain,
                position + 1
            ),
        );
        return Ok(());
    }

    if args.dry_run {
        if opts.json {
            let mut without_id = record.clone();
            without_id.remove("id");
            let mut fields = serde_json::Map::new();
            fields.insert("action".into(), Value::String("dry-run".into()));
            fields.insert("wouldDo".into(), Value::String("created".into()));
            fields.insert("domain".into(), Value::String(args.domain.clone()));
            fields.insert(
                "type".into(),
                Value::String(
                    args.record_type
                        .clone()
                        .unwrap_or_else(|| "convention".into()),
                ),
            );
            fields.insert("record".into(), Value::Object(without_id));
            print_json(&success_envelope("record", fields), false);
        } else {
            print_line(
                opts.quiet,
                &format!(
                    "✓ Dry-run: Would create {} in {}\n  Run without --dry-run to apply changes.",
                    args.record_type
                        .clone()
                        .unwrap_or_else(|| "convention".into()),
                    args.domain
                ),
            );
        }
        return Ok(());
    }

    let line = Value::Object(record.clone()).to_string();
    let mut updated = text;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(&line);
    updated.push('\n');
    std::fs::write(&file, updated).map_err(|source| {
        Failure::handled("record", format!("writing {}: {source}", file.display()))
    })?;

    if opts.json {
        let mut fields = serde_json::Map::new();
        fields.insert("action".into(), Value::String("created".into()));
        fields.insert("domain".into(), Value::String(args.domain.clone()));
        fields.insert(
            "type".into(),
            Value::String(
                args.record_type
                    .clone()
                    .unwrap_or_else(|| "convention".into()),
            ),
        );
        fields.insert("record".into(), Value::Object(record));
        print_json(&success_envelope("record", fields), false);
    } else {
        print_line(
            opts.quiet,
            &format!(
                "✓ Recorded {} in {}",
                args.record_type
                    .clone()
                    .unwrap_or_else(|| "convention".into()),
                args.domain
            ),
        );
    }
    Ok(())
}

/// stdin/batch path: preserves input key order, appends recorded_at,
/// classification, id.
fn stdin_batch(
    opts: &GlobalOpts,
    args: &RecordArgs,
    store: &crate::commands::ConfigStore,
    _auto_created: bool,
) -> Result<(), Failure> {
    let raw = if args.stdin {
        let mut buffer = String::new();
        std::io::stdin()
            .read_to_string(&mut buffer)
            .map_err(|source| Failure::handled("record", format!("reading stdin: {source}")))?;
        buffer
    } else {
        std::fs::read_to_string(args.batch.as_ref().expect("batch path"))
            .map_err(|source| Failure::handled("record", format!("reading batch: {source}")))?
    };
    let parsed: Value = serde_json::from_str(&raw)
        .map_err(|source| Failure::handled("record", format!("parsing input: {source}")))?;
    let items: Vec<Value> = match parsed {
        Value::Array(items) => items,
        single => vec![single],
    };

    let file = domain_file(&store.root, &args.domain);
    let mut text = std::fs::read_to_string(&file).unwrap_or_default();
    let mut created = 0usize;
    let mut warnings: Vec<Value> = Vec::new();
    for item in items {
        let Some(object) = item.as_object().cloned() else {
            warnings.push(Value::String("skipped non-object entry".into()));
            continue;
        };
        let record_type = object
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("convention")
            .to_string();
        let id_key = id_key_field(&record_type);
        let id_value = object
            .get(id_key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let id = record_id(&record_type, &id_value);

        let mut line = object;
        line.insert("recorded_at".into(), Value::String(now_iso()));
        line.entry("classification")
            .or_insert_with(|| Value::String("tactical".into()));
        line.insert("id".into(), Value::String(id));
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&Value::Object(line).to_string());
        text.push('\n');
        created += 1;
    }
    std::fs::write(&file, text).map_err(|source| {
        Failure::handled("record", format!("writing {}: {source}", file.display()))
    })?;

    let action = if args.stdin { "stdin" } else { "batch" };
    if opts.json {
        let mut fields = serde_json::Map::new();
        fields.insert("action".into(), Value::String(action.into()));
        fields.insert("domain".into(), Value::String(args.domain.clone()));
        fields.insert("created".into(), Value::from(created as u64));
        fields.insert("updated".into(), Value::from(0));
        fields.insert("skipped".into(), Value::from(0));
        fields.insert("errors".into(), Value::Array(Vec::new()));
        fields.insert("warnings".into(), Value::Array(warnings));
        print_json(&success_envelope("record", fields), false);
    } else {
        print_line(
            opts.quiet,
            &format!("✓ Created {created} record(s) in {}", args.domain),
        );
    }
    Ok(())
}
