//! `mulch record` — the record-creation surface (flag path, stdin,
//! batch, dry-run).

use std::fmt::Write as _;
use std::io::Read as _;

use mulch::{id_key_field, record_id};
use serde_json::{Map, Value};

use crate::cli::{GlobalOpts, RecordArgs};
use crate::commands::now_iso;
use crate::output::{Failure, print_json, print_line, success_envelope};

/// Builds the canonical JSONL object for the flag path (field order
/// pinned from the reference: type, classification, recorded_at,
/// evidence?, tags?, relates_to?, supersedes?, outcomes?,
/// dir_anchors?, payload…, files?, id).
fn build_record(
    args: &RecordArgs,
    recorded_at: &str,
    evidence: Option<Map<String, Value>>,
) -> Result<Map<String, Value>, Failure> {
    let record_type = args
        .record_type
        .clone()
        .unwrap_or_else(|| "convention".into());
    let payload: &[&str] = mulch::payload_fields(&record_type);

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
        record.insert("relates_to".into(), strings_value(&relates));
    }
    if let Some(supersedes) = split_list(args.supersedes.as_deref()) {
        record.insert("supersedes".into(), strings_value(&supersedes));
    }
    // outcomes precede dir_anchors (reference buildRecordFromOptions
    // key order, record.ts:82-86 — probe-pinned by the sprint-6 spec
    // review).
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
    if !args.dir_anchors.is_empty() {
        let anchors: Vec<Value> = args
            .dir_anchors
            .iter()
            .map(|a| Value::String(a.trim_end_matches('/').into()))
            .collect();
        record.insert("dir_anchors".into(), Value::Array(anchors));
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

    // Reference-list pattern validation runs on the built record and
    // surfaces the reference's schema-validation blob (probe 3).
    if let crate::commands::schema::FullVerdict::Invalid { subs, hint } =
        crate::commands::schema::full_verdict(&Value::Object(record.clone()))
    {
        return Err(ref_validation_failure(&subs, &hint));
    }

    let id_key_value = provided(id_key_field(&record_type)).unwrap_or_default();
    let id = record_id(&record_type, &id_key_value);
    record.insert("id".into(), Value::String(id));
    Ok(record)
}

/// The reference-pattern failure: plain renders as multi-line
/// `record failed schema validation` with the Hint line; the json
/// envelope says `Schema validation failed: <joined>. <hint>`.
fn ref_validation_failure(subs: &[String], hint: &str) -> Failure {
    let mut message = String::from("Error: record failed schema validation:");
    for sub in subs {
        let _ = write!(message, "\n  {sub}");
    }
    let _ = write!(message, "\n{hint}");
    let mut failure = Failure::handled("record", message);
    let joined = subs.join(crate::commands::schema::SUB_SEP);
    failure.envelope["error"] = Value::String(format!(
        "{}{joined}. {hint}",
        crate::commands::schema::VALIDATION_PREFIX
    ));
    failure
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
fn split_list(raw: Option<&str>) -> Option<Vec<String>> {
    raw.map(|text| {
        text.split(',')
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect()
    })
}

/// A JSON array of strings.
fn strings_value(items: &[String]) -> Value {
    Value::Array(items.iter().map(|i| Value::String(i.clone())).collect())
}

/// Numeric outcome duration.
pub(super) fn number_value(raw: &str) -> Result<Value, Failure> {
    raw.parse::<u64>()
        .map(Value::from)
        .map_err(|_| Failure::handled("record", format!("invalid number: {raw}")))
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
    let mut store = crate::commands::open_store("record", false)?;

    // Auto-create precedes validation AND dry-run (reference behavior:
    // the domain side effects happen even for dry-runs and failed
    // records).
    let known = store.domains().iter().any(|d| d == &args.domain);
    if !known {
        store
            .register_domain(&args.domain)
            .map_err(|source| Failure::handled("record", crate::output::chain_message(&source)))?;
        // The auto-create line hits stdout even in --json mode; it is
        // quiet-gated (spec review).
        print_line(
            opts.quiet,
            &format!("✓ Auto-created domain \"{}\"", args.domain),
        );
    }

    if args.stdin || args.batch.is_some() {
        return stdin_batch(opts, args, &store);
    }

    let recorded_at = now_iso();
    let record = match build_record(args, &recorded_at, evidence_map(args)) {
        Ok(built) => built,
        Err(mut failure) => {
            // Parity contract: the --json error drops the `Error: `
            // prefix and says `Example:` where plain stderr says
            // `Retry:` (probe 2, §3d; spec review round 2). Only the
            // missing-flags failure gets that rewrite — schema
            // validation failures carry their own envelope text.
            failure.envelope_to_stderr = true;
            if opts.json && failure.message.contains("\n  Retry: ") {
                let json_text = failure
                    .message
                    .replacen("Error: ", "", 1)
                    .replace("\n  Retry: ", " Example: ");
                failure.envelope["error"] = Value::String(json_text);
            }
            return Err(failure);
        }
    };

    // Duplicate detection over the STRICT read (reference
    // `readExpertiseFile` + `findDuplicate`): malformed lines and
    // unregistered types abort before the write, and the dedup key is
    // the type's dedup FIELD (registry dedupKey), never the id — a
    // renamed record still dedupes after `edit --name` (mulch-ccf6).
    let existing = store
        .read_records(&args.domain, opts.allow_unknown_types)
        .map_err(|source| {
            Failure::handled_on_stderr("record", crate::commands::render_core_error(&source))
        })?;
    let duplicate_position = mulch::find_duplicate(
        existing.iter().map(|line| &line.record),
        &Value::Object(record.clone()),
    );
    if let Some(position) = duplicate_position
        && !args.force
    {
        return duplicate_action(opts, args, &store, &existing, position, record);
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
    store
        .append_domain_line(&args.domain, &line)
        .map_err(|source| Failure::handled("record", crate::output::chain_message(&source)))?;

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

/// Handles a detected duplicate without `--force` (reference
/// `record.ts` duplicate branch, mulch-ccf6): named types UPSERT in
/// place, anonymous types (`convention`/`failure`) skip with the
/// advisory.
fn duplicate_action(
    opts: &GlobalOpts,
    args: &RecordArgs,
    store: &mulch::StoreFiles,
    existing: &[mulch::LineRecord],
    position: usize,
    record: Map<String, Value>,
) -> Result<(), Failure> {
    let kind = args
        .record_type
        .clone()
        .unwrap_or_else(|| "convention".into());
    let named = mulch::is_named_type(&kind);
    if args.dry_run {
        // Dry-run mirrors the write decision (`wouldDo`), and the json
        // `record` carries no id — ids are a write-time product.
        if opts.json {
            let mut without_id = record;
            without_id.remove("id");
            let mut fields = serde_json::Map::new();
            fields.insert("action".into(), Value::String("dry-run".into()));
            fields.insert(
                "wouldDo".into(),
                Value::String(if named { "updated" } else { "skipped" }.into()),
            );
            fields.insert("domain".into(), Value::String(args.domain.clone()));
            fields.insert("type".into(), Value::String(kind));
            fields.insert("record".into(), Value::Object(without_id));
            print_json(&success_envelope("record", fields), false);
        } else if named {
            print_line(
                opts.quiet,
                &format!(
                    "✓ Dry-run: Would update existing {kind} in {}\n  Run without --dry-run to apply changes.",
                    args.domain
                ),
            );
        } else {
            print_line(
                opts.quiet,
                &format!(
                    "Dry-run: Duplicate {kind} already exists in {}. Would skip.\n  Run without --dry-run to apply changes.",
                    args.domain
                ),
            );
        }
        return Ok(());
    }
    if !named {
        if opts.json {
            let mut fields = serde_json::Map::new();
            fields.insert("action".into(), Value::String("skipped".into()));
            fields.insert("domain".into(), Value::String(args.domain.clone()));
            fields.insert("type".into(), Value::String(kind));
            fields.insert("index".into(), Value::from((position + 1) as u64));
            print_json(&success_envelope("record", fields), false);
        } else {
            print_line(
                opts.quiet,
                &format!(
                    "Duplicate {kind} already exists in {} (record #{}). Use --force to add anyway.",
                    args.domain,
                    position + 1
                ),
            );
        }
        return Ok(());
    }

    // Named upsert: the built record replaces the line — fresh
    // recorded_at, new field values, recomputed id (a stale id from an
    // earlier rename corrects itself) — with merged outcomes and the
    // key-order contract owned by [`mulch::upsert_record`]. The file
    // rewrites compactly.
    let upserted = mulch::upsert_record(&existing[position].record, record);
    let mut lines: Vec<Value> = existing.iter().map(|line| line.record.clone()).collect();
    lines[position] = Value::Object(upserted.clone());
    store
        .rewrite_domain(&args.domain, &lines)
        .map_err(|source| Failure::handled("record", crate::output::chain_message(&source)))?;

    if opts.json {
        let mut fields = serde_json::Map::new();
        fields.insert("action".into(), Value::String("updated".into()));
        fields.insert("domain".into(), Value::String(args.domain.clone()));
        fields.insert("type".into(), Value::String(kind));
        fields.insert("index".into(), Value::from((position + 1) as u64));
        fields.insert("record".into(), Value::Object(upserted));
        print_json(&success_envelope("record", fields), false);
    } else {
        print_line(
            opts.quiet,
            &format!(
                "✓ Updated existing {kind} in {} (record #{})",
                args.domain,
                position + 1
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
    store: &mulch::StoreFiles,
) -> Result<(), Failure> {
    let raw = if args.stdin {
        let mut buffer = String::new();
        std::io::stdin()
            .read_to_string(&mut buffer)
            .map_err(|source| Failure::handled("record", format!("reading stdin: {source}")))?;
        buffer
    } else {
        let path = args
            .batch
            .as_ref()
            .expect("stdin_batch runs only when stdin or a batch path was given");
        std::fs::read_to_string(path)
            .map_err(|source| Failure::handled("record", format!("reading batch: {source}")))?
    };
    let parsed: Value = serde_json::from_str(&raw)
        .map_err(|source| Failure::handled("record", format!("parsing input: {source}")))?;
    let items: Vec<Value> = match parsed {
        Value::Array(items) => items,
        single => vec![single],
    };

    let existing = store
        .read_records(&args.domain, opts.allow_unknown_types)
        .map_err(|source| {
            Failure::handled_on_stderr("record", crate::commands::render_core_error(&source))
        })?;
    // The working copy the reference dedupes against (`currentRecords`):
    // existing records plus this batch's accepted appends — within-batch
    // duplicates upsert too. Dry-run never mutates it (the reference
    // counts a within-batch duplicate as another create there).
    let mut working: Vec<Value> = existing.iter().map(|line| line.record.clone()).collect();
    let mut created = 0usize;
    let mut updated = 0usize;
    let mut skipped = 0usize;
    let mut errors: Vec<Value> = Vec::new();

    for (index, item) in items.into_iter().enumerate() {
        let Some(object) = item.as_object().cloned() else {
            // Reference: ajv rejects non-objects with `must be object`
            // (the empty instance path supplies the extra gap).
            errors.push(Value::String(format!("Record {index}:  must be object")));
            continue;
        };
        // The reference normalizes each batch record FIRST — recorded_at
        // and classification are filled when absent — and only then
        // validates (`processStdinRecords`), so a record missing the
        // common fields is accepted and enriched (mulch-5f8a).
        let mut line = object;
        line.entry("recorded_at")
            .or_insert_with(|| Value::String(now_iso()));
        line.entry("classification")
            .or_insert_with(|| Value::String("tactical".into()));

        // Schema validation on the enriched record (reference blobs).
        if let crate::commands::schema::FullVerdict::Invalid { subs, hint } =
            crate::commands::schema::full_verdict(&Value::Object(line.clone()))
        {
            // Reference batch entry: `Record ${i}: ${subs}` with the
            // type hint only when the record declares a registered
            // type (`requirements[recordType]` is undefined otherwise).
            let registered = line
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|kind| mulch::PAYLOAD_TYPES.contains(&kind));
            let hint_part = if registered {
                format!(". {hint}")
            } else {
                String::new()
            };
            errors.push(Value::String(format!(
                "Record {index}: {}{hint_part}",
                subs.join(crate::commands::schema::SUB_SEP)
            )));
            continue;
        }
        let record_type = line
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("convention")
            .to_string();
        let named = mulch::is_named_type(&record_type);
        let id_key = id_key_field(&record_type);
        let id_value = line
            .get(id_key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let id = record_id(&record_type, &id_value);
        // Dedupe by the type's dedup FIELD against the working copy
        // (`findDuplicate`), never by id (mulch-ccf6). The enriched
        // record is safe here: find_duplicate compares only `type` and
        // the type's dedup field, which enrichment never touches.
        let duplicate = if args.force {
            None
        } else {
            mulch::find_duplicate(working.iter(), &Value::Object(line.clone()))
        };

        if args.dry_run {
            // Count-only pass over the unmutated working copy.
            match duplicate {
                Some(_) if named => updated += 1,
                Some(_) => skipped += 1,
                None => created += 1,
            }
            continue;
        }
        match duplicate {
            Some(position) if named => {
                // merge first, then the id — an input id keeps its
                // position, a generated one lands after the merged
                // outcomes (reference write-time `if (!r.id)`).
                let mut line = mulch::merge_outcomes(&working[position], line);
                line.entry("id").or_insert_with(|| Value::String(id));
                working[position] = Value::Object(line);
                updated += 1;
            }
            Some(_) => {
                skipped += 1;
            }
            None => {
                line.entry("id").or_insert_with(|| Value::String(id));
                working.push(Value::Object(line));
                created += 1;
            }
        }
    }
    let action = if args.stdin { "stdin" } else { "batch" };

    if args.dry_run {
        // Reference dry-run shape (spec review round 2): no writes.
        if opts.json {
            let mut fields = serde_json::Map::new();
            fields.insert("action".into(), Value::String("dry-run".into()));
            fields.insert("domain".into(), Value::String(args.domain.clone()));
            fields.insert("created".into(), Value::from(created as u64));
            fields.insert("updated".into(), Value::from(updated as u64));
            fields.insert("skipped".into(), Value::from(skipped as u64));
            fields.insert("errors".into(), Value::Array(errors.clone()));
            fields.insert("warnings".into(), Value::Array(Vec::new()));
            print_json(&success_envelope("record", fields), false);
        } else {
            // Reference summary: `Would process` counts created+updated;
            // the per-action lines print only when non-zero, and an
            // all-zero batch says so instead.
            let total = created + updated;
            if total > 0 || skipped > 0 {
                let mut text = format!(
                    "✓ Dry-run complete. Would process {total} record(s) in {}:",
                    args.domain
                );
                if created > 0 {
                    use std::fmt::Write as _;
                    let _ = write!(text, "\n  Create: {created}");
                }
                if updated > 0 {
                    let _ = write!(text, "\n  Update: {updated}");
                }
                if skipped > 0 {
                    let _ = write!(text, "\n  Skip: {skipped}");
                }
                text.push_str("\n  Run without --dry-run to apply changes.");
                print_line(opts.quiet, &text);
            } else {
                print_line(opts.quiet, "No records would be processed.");
            }
        }
        return Ok(());
    }

    if !errors.is_empty() {
        // Reference failure contract (spec review round 2): the action
        // envelope with `errors` goes to stdout, the simple error
        // envelope to stderr, the store stays untouched, exit 1.
        // Plain rendering indents one entry per line; the json
        // envelope keeps the `; `-joined single line (reference:
        // console.error per entry vs join("; ") in outputJsonError).
        let summary = errors
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect::<Vec<_>>()
            .join("; ");
        let mut plain = String::from("Validation errors:");
        for error in &errors {
            if let Some(text) = error.as_str() {
                plain.push_str("\n  ");
                plain.push_str(text);
            }
        }
        let mut failure = Failure::handled("record", plain);
        failure.envelope["error"] = Value::String(format!("Validation errors: {summary}"));
        failure.envelope_to_stderr = true;
        if opts.json {
            let mut fields = serde_json::Map::new();
            fields.insert("action".into(), Value::String(action.into()));
            fields.insert("domain".into(), Value::String(args.domain.clone()));
            fields.insert("created".into(), Value::from(0u64));
            fields.insert("updated".into(), Value::from(0u64));
            fields.insert("skipped".into(), Value::from(skipped as u64));
            fields.insert("errors".into(), Value::Array(errors));
            fields.insert("warnings".into(), Value::Array(Vec::new()));
            let mut body = serde_json::Map::new();
            body.insert("success".into(), Value::Bool(false));
            body.insert("command".into(), Value::String("record".into()));
            body.extend(fields);
            print_json(&Value::Object(body), false);
            print_json(&failure.envelope, true);
            failure.rendered = true;
        }
        return Err(failure);
    }

    // The batch path always rewrites through the compact writer
    // (reference `writeExpertiseFile`): comments and blank lines drop,
    // id-less survivors get ids, and create-only batches rewrite too
    // (mulch-ca49; the FLAG path keeps its verbatim append).
    store
        .rewrite_domain(&args.domain, &working)
        .map_err(|source| Failure::handled("record", crate::output::chain_message(&source)))?;

    if opts.json {
        let mut fields = serde_json::Map::new();
        fields.insert("action".into(), Value::String(action.into()));
        fields.insert("domain".into(), Value::String(args.domain.clone()));
        fields.insert("created".into(), Value::from(created as u64));
        fields.insert("updated".into(), Value::from(updated as u64));
        fields.insert("skipped".into(), Value::from(skipped as u64));
        fields.insert("errors".into(), Value::Array(Vec::new()));
        fields.insert("warnings".into(), Value::Array(Vec::new()));
        print_json(&success_envelope("record", fields), false);
    } else {
        // Reference order: created, updated, then the duplicates line —
        // each only when non-zero (a zero-batch prints nothing).
        if created > 0 {
            print_line(
                opts.quiet,
                &format!("✓ Created {created} record(s) in {}", args.domain),
            );
        }
        if updated > 0 {
            print_line(
                opts.quiet,
                &format!("✓ Updated {updated} record(s) in {}", args.domain),
            );
        }
        if skipped > 0 {
            print_line(
                opts.quiet,
                &format!("Skipped {skipped} duplicate(s) in {}", args.domain),
            );
        }
    }
    Ok(())
}
