//! `mulch record` — the record-creation surface (flag path, stdin,
//! batch, dry-run).

use std::fmt::Write as _;
use std::io::Read as _;

use mulch::{id_key_field, record_id, schema};
use serde_json::{Map, Value};

use crate::cli::{GlobalOpts, RecordArgs};
use crate::commands::{PayloadFlags, now_iso};
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
    let record_type = mulch::effective_type(args.record_type.as_deref()).to_string();
    let payload: &[&str] = mulch::payload_fields(&record_type);
    let flags = PayloadFlags::from_record(args);

    let missing: Vec<&str> = payload
        .iter()
        .copied()
        .filter(|field| flags.get(field).is_none())
        .collect();
    if !missing.is_empty() {
        return Err(missing_flags_failure(
            &record_type,
            &missing,
            &args.domain,
            &flags,
        ));
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
    for field in payload {
        record.insert(
            (*field).into(),
            Value::String(flags.get(field).unwrap_or_default().to_string()),
        );
    }
    // The reference collects fields from `def.required ∪ def.optional`
    // only: `--files` on a type that does not declare it is DROPPED
    // (mulch-b8ca; `files` is declared by pattern and reference).
    let declares_files =
        mulch::type_spec(&record_type).is_some_and(|spec| spec.optional.contains(&"files"));
    if declares_files && let Some(files) = split_list(args.files.as_deref()) {
        record.insert("files".into(), strings_value(&files));
    }

    // Reference-list pattern validation runs on the built record and
    // surfaces the reference's schema-validation blob (probe 3).
    if let schema::FullVerdict::Invalid { subs, hint } =
        schema::full_verdict(&Value::Object(record.clone()))
    {
        return Err(ref_validation_failure(&subs, &hint));
    }

    let id_key_value = flags.get(id_key_field(&record_type)).unwrap_or_default();
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

/// The missing-required-flag failure with the reference's Retry hint.
fn missing_flags_failure(
    record_type: &str,
    missing: &[&str],
    domain: &str,
    flags: &PayloadFlags<'_>,
) -> Failure {
    let missing_flags: Vec<String> = missing.iter().map(|f| format!("--{f}")).collect();
    let mut echo = format!("ml record {domain} --type {record_type}");
    for field in ["name", "description", "resolution", "title", "rationale"] {
        if let Some(pair) = flags.retry_pair(field) {
            let _ = write!(echo, " {pair}");
        }
    }
    for flag in &missing_flags {
        let hint = flag.trim_start_matches("--");
        let _ = write!(echo, " {flag} \"<{hint}>\"");
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
    // One decision owner (lib `upsert_plan`): named duplicates upsert
    // with merged outcomes, anonymous ones skip, --force falls through
    // to the create path below.
    let working: Vec<Value> = existing.iter().map(|line| line.record.clone()).collect();
    match mulch::upsert_plan(&working, &Value::Object(record.clone()), args.force) {
        mulch::UpsertPlan::Skip { index } => {
            skip_advisory(opts, args, index, record);
            return Ok(());
        }
        mulch::UpsertPlan::Update { index, merged } => {
            return upsert_update(opts, args, &store, &existing, index, record, merged);
        }
        mulch::UpsertPlan::Create => {}
    }

    if args.dry_run {
        report_record_action(opts, args, RecordAction::Create, Some(record));
        return Ok(());
    }

    let line = Value::Object(record.clone()).to_string();
    store
        .append_domain_line(&args.domain, &line)
        .map_err(|source| Failure::handled("record", crate::output::chain_message(&source)))?;

    report_record_action(opts, args, RecordAction::Create, Some(record));
    Ok(())
}

/// Which flag-path write decision is being reported (the upsert
/// plan's shape): create, or a duplicate skip/update against the
/// existing line at `index` (rendered 1-based, real runs only).
#[derive(Clone, Copy)]
enum RecordAction {
    Create,
    Skip { index: usize },
    Update { index: usize },
}

impl RecordAction {
    /// The 0-based index of the existing duplicate line, if any.
    fn index(self) -> Option<usize> {
        match self {
            RecordAction::Create => None,
            RecordAction::Skip { index } | RecordAction::Update { index } => Some(index),
        }
    }
}

/// Builds the json success envelope for one flag-path decision:
/// `action`/`wouldDo?`/`domain`/`type`/`index?`/`record?` (key order
/// pinned from the reference). Dry-run mirrors the write decision —
/// `action` reads `dry-run`, `wouldDo` carries the verb — and the
/// reported record drops its write-time id.
fn record_envelope(
    action: RecordAction,
    domain: &str,
    record_type: Option<&str>,
    dry_run: bool,
    record: Option<Map<String, Value>>,
) -> Value {
    let kind = mulch::effective_type(record_type);
    let verb = match action {
        RecordAction::Create => "created",
        RecordAction::Skip { .. } => "skipped",
        RecordAction::Update { .. } => "updated",
    };
    let mut fields = serde_json::Map::new();
    if dry_run {
        fields.insert("action".into(), Value::String("dry-run".into()));
        fields.insert("wouldDo".into(), Value::String(verb.into()));
    } else {
        fields.insert("action".into(), Value::String(verb.into()));
    }
    fields.insert("domain".into(), Value::String(domain.into()));
    fields.insert("type".into(), Value::String(kind.to_string()));
    if !dry_run && let Some(index) = action.index() {
        fields.insert("index".into(), Value::from((index + 1) as u64));
    }
    if let Some(mut record) = record {
        if dry_run {
            record.remove("id");
        }
        fields.insert("record".into(), Value::Object(record));
    }
    success_envelope("record", fields)
}

/// The plain line for one flag-path decision (wording pinned from the
/// reference): dry-run mirrors the write decision, skip/update render
/// the 1-based index of the existing duplicate.
fn record_plain_line(
    action: RecordAction,
    domain: &str,
    record_type: Option<&str>,
    dry_run: bool,
) -> String {
    let kind = mulch::effective_type(record_type);
    match (action, dry_run) {
        (RecordAction::Create, true) => format!(
            "✓ Dry-run: Would create {kind} in {domain}\n  Run without --dry-run to apply changes."
        ),
        (RecordAction::Create, false) => format!("✓ Recorded {kind} in {domain}"),
        (RecordAction::Skip { .. }, true) => format!(
            "Dry-run: Duplicate {kind} already exists in {domain}. Would skip.\n  Run without --dry-run to apply changes."
        ),
        (RecordAction::Skip { index }, false) => format!(
            "Duplicate {kind} already exists in {domain} (record #{}). Use --force to add anyway.",
            index + 1
        ),
        (RecordAction::Update { .. }, true) => format!(
            "✓ Dry-run: Would update existing {kind} in {domain}\n  Run without --dry-run to apply changes."
        ),
        (RecordAction::Update { index }, false) => format!(
            "✓ Updated existing {kind} in {domain} (record #{})",
            index + 1
        ),
    }
}

/// Renders one flag-path write decision on its channel: the json
/// success envelope or the plain line (quiet-gated), one family for
/// create/skip/update in dry-run and real runs.
fn report_record_action(
    opts: &GlobalOpts,
    args: &RecordArgs,
    action: RecordAction,
    record: Option<Map<String, Value>>,
) {
    if opts.json {
        print_json(
            &record_envelope(
                action,
                &args.domain,
                args.record_type.as_deref(),
                args.dry_run,
                record,
            ),
            false,
        );
    } else {
        print_line(
            opts.quiet,
            &record_plain_line(
                action,
                &args.domain,
                args.record_type.as_deref(),
                args.dry_run,
            ),
        );
    }
}

/// Handles a detected duplicate without `--force` (reference
/// `record.ts` duplicate branch, mulch-ccf6): named types UPSERT in
/// place, anonymous types (`convention`/`failure`) skip with the
/// advisory.
/// The anonymous-duplicate advisory (flag path, `Skip` arm): nothing
/// is written; `--force` is the documented escape hatch.
fn skip_advisory(opts: &GlobalOpts, args: &RecordArgs, index: usize, record: Map<String, Value>) {
    // Dry-run mirrors the write decision through the incoming record
    // (reported id-less); the real skip reports no record at all.
    report_record_action(
        opts,
        args,
        RecordAction::Skip { index },
        args.dry_run.then_some(record),
    );
}

/// The named-duplicate upsert (flag path, `Update` arm): the plan's
/// merged record (incoming fields, outcomes existing-first) replaces
/// the line, with the builder's pre-assigned id LIFTED LAST so it
/// lands after the appended outcomes (probe-pinned key order). The
/// file rewrites compactly.
fn upsert_update(
    opts: &GlobalOpts,
    args: &RecordArgs,
    store: &mulch::StoreFiles,
    existing: &[mulch::LineRecord],
    index: usize,
    record: Map<String, Value>,
    merged: Map<String, Value>,
) -> Result<(), Failure> {
    if args.dry_run {
        report_record_action(opts, args, RecordAction::Update { index }, Some(record));
        return Ok(());
    }

    // id LAST: lift the builder's pre-assigned id over the merged
    // outcomes (the reference's `{ ...record, outcomes }` + write-time
    // id placement).
    let mut upserted = merged;
    if let Some(id) = upserted.remove("id") {
        upserted.insert("id".into(), id);
    }
    let mut lines: Vec<Value> = existing.iter().map(|line| line.record.clone()).collect();
    lines[index] = Value::Object(upserted.clone());
    store
        .rewrite_domain(&args.domain, &lines)
        .map_err(|source| Failure::handled("record", crate::output::chain_message(&source)))?;

    report_record_action(opts, args, RecordAction::Update { index }, Some(upserted));
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
        if let schema::FullVerdict::Invalid { subs, hint } =
            schema::full_verdict(&Value::Object(line.clone()))
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
                schema::render_subs(&subs).join(schema::SUB_SEP)
            )));
            continue;
        }
        let record_type =
            mulch::effective_type(line.get("type").and_then(Value::as_str)).to_string();
        let id_key = id_key_field(&record_type);
        let id_value = line
            .get(id_key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let id = record_id(&record_type, &id_value);
        // One decision owner (lib `upsert_plan`): the dedup field
        // against the working copy, never the id (mulch-ccf6). The
        // enriched record is safe here: the plan compares only `type`
        // and the type's dedup field, which enrichment never touches.
        let plan = mulch::upsert_plan(&working, &Value::Object(line.clone()), args.force);

        if args.dry_run {
            // Count-only pass over the unmutated working copy
            // (within-batch duplicates count as creates here — the
            // reference never mutates its copy in dry-run either).
            match plan {
                mulch::UpsertPlan::Create => created += 1,
                mulch::UpsertPlan::Update { .. } => updated += 1,
                mulch::UpsertPlan::Skip { .. } => skipped += 1,
            }
            continue;
        }
        match plan {
            mulch::UpsertPlan::Update { index, merged } => {
                // The plan already merged the outcomes (existing
                // first); an input id keeps its position, a generated
                // one lands after the merged outcomes (reference
                // write-time `if (!r.id)`).
                let mut line = merged;
                line.entry("id").or_insert_with(|| Value::String(id));
                working[index] = Value::Object(line);
                updated += 1;
            }
            mulch::UpsertPlan::Skip { .. } => {
                skipped += 1;
            }
            mulch::UpsertPlan::Create => {
                line.entry("id").or_insert_with(|| Value::String(id));
                working.push(Value::Object(line));
                created += 1;
            }
        }
    }
    // Partial writes (reference `processStdinRecords` + the batch
    // caller): valid records WRITE even when others failed; the write
    // guard is `created > 0 || updated > 0` (comments and blank lines
    // drop, id-less survivors get ids — a skip-only or empty batch
    // leaves the file byte-identical; mulch-ca49).
    if !args.dry_run && (created > 0 || updated > 0) {
        store
            .rewrite_domain(&args.domain, &working)
            .map_err(|source| Failure::handled("record", crate::output::chain_message(&source)))?;
    }

    let failed = !errors.is_empty();
    let wrote = created + updated > 0;
    let action = if args.dry_run {
        "dry-run"
    } else if args.stdin {
        "stdin"
    } else {
        "batch"
    };

    // Reference: the caller prints the error block BEFORE the surfaces
    // (stderr: console.error per entry plain, outputJsonError joined
    // in json); here that printing lives in this function, ahead of
    // the surface rendering below. The exit-1 tail then carries an
    // already-rendered, empty-message failure.
    let summary = errors
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<Vec<_>>()
        .join("; ");
    if failed && opts.json {
        let mut body = serde_json::Map::new();
        body.insert("success".into(), Value::Bool(false));
        body.insert("command".into(), Value::String("record".into()));
        body.insert(
            "error".into(),
            Value::String(format!("Validation errors: {summary}")),
        );
        print_json(&Value::Object(body), true);
    }
    if failed && !opts.json {
        #[allow(
            clippy::print_stderr,
            reason = "batch error rendering is the CLI boundary"
        )]
        for line in validation_errors_block(&errors) {
            eprintln!("{line}");
        }
    }

    if opts.json {
        // The result envelope always prints (stdout) — its success is
        // `errors empty || anything written` (the reference formula).
        let mut fields = serde_json::Map::new();
        fields.insert("action".into(), Value::String(action.into()));
        fields.insert("domain".into(), Value::String(args.domain.clone()));
        fields.insert("created".into(), Value::from(created as u64));
        fields.insert("updated".into(), Value::from(updated as u64));
        fields.insert("skipped".into(), Value::from(skipped as u64));
        fields.insert("errors".into(), Value::Array(errors.clone()));
        fields.insert("warnings".into(), Value::Array(Vec::new()));
        let mut body = serde_json::Map::new();
        body.insert("success".into(), Value::Bool(!failed || wrote));
        body.insert("command".into(), Value::String("record".into()));
        body.extend(fields);
        print_json(&Value::Object(body), false);
    } else if args.dry_run {
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

    if failed && !wrote {
        // Exit 1 only when nothing was written (reference:
        // `errors.length > 0 && created + updated === 0`). Plain: the
        // block already printed above (both modes); only the exit
        // code remains.
        let mut failure = Failure::handled("record", "");
        failure.envelope["error"] = Value::String(format!("Validation errors: {summary}"));
        failure.envelope_to_stderr = true;
        failure.rendered = true;
        return Err(failure);
    }
    Ok(())
}

/// The plain `Validation errors:` block, one entry per line
/// (reference console.error shape).
fn validation_errors_block(errors: &[Value]) -> Vec<String> {
    let mut lines = vec!["Validation errors:".to_string()];
    for error in errors {
        if let Some(text) = error.as_str() {
            lines.push(format!("  {text}"));
        }
    }
    lines
}

#[cfg(test)]
mod report_family_tests {
    use super::*;

    fn sample_record() -> Map<String, Value> {
        let mut record = Map::new();
        record.insert("type".into(), Value::String("pattern".into()));
        record.insert("name".into(), Value::String("p".into()));
        record.insert("id".into(), Value::String("mx-test".into()));
        record
    }

    fn keys(envelope: &Value) -> Vec<&str> {
        envelope
            .as_object()
            .expect("envelope object")
            .keys()
            .map(String::as_str)
            .collect()
    }

    /// The three dry-run envelopes are one family: `dry-run` action,
    /// `wouldDo` verb, no index, and the reported record without its
    /// write-time id (key order is part of the parity contract).
    #[test]
    fn dry_run_envelopes_mirror_the_decision_and_drop_the_id() {
        for (action, would_do) in [
            (RecordAction::Create, "created"),
            (RecordAction::Skip { index: 3 }, "skipped"),
            (RecordAction::Update { index: 3 }, "updated"),
        ] {
            let envelope =
                record_envelope(action, "d", Some("pattern"), true, Some(sample_record()));
            assert_eq!(keys(&envelope), [
                "success", "command", "action", "wouldDo", "domain", "type", "record",
            ]);
            assert_eq!(envelope["action"], "dry-run");
            assert_eq!(envelope["wouldDo"], would_do);
            assert_eq!(envelope["type"], "pattern");
            assert!(envelope["record"].get("id").is_none());
        }
    }

    /// Real runs carry the action verb, the 1-based index for
    /// skip/update (never for create), and the written record — the
    /// skip advisory reports no record at all.
    #[test]
    fn real_run_envelopes_carry_action_index_and_record() {
        let create = record_envelope(
            RecordAction::Create,
            "d",
            None,
            false,
            Some(sample_record()),
        );
        assert_eq!(keys(&create), [
            "success", "command", "action", "domain", "type", "record"
        ]);
        assert_eq!(create["action"], "created");
        assert_eq!(create["type"], "convention");
        assert_eq!(create["record"]["id"], "mx-test");

        let skip = record_envelope(RecordAction::Skip { index: 2 }, "d", None, false, None);
        assert_eq!(keys(&skip), [
            "success", "command", "action", "domain", "type", "index"
        ]);
        assert_eq!(skip["index"], 3);

        let update = record_envelope(
            RecordAction::Update { index: 2 },
            "d",
            None,
            false,
            Some(sample_record()),
        );
        assert_eq!(keys(&update), [
            "success", "command", "action", "domain", "type", "index", "record"
        ]);
        assert_eq!(update["index"], 3);
        assert_eq!(update["record"]["id"], "mx-test");
    }

    /// The six plain lines, word-for-word; a missing `--type`
    /// reports the convention default.
    #[test]
    fn plain_lines_pin_the_reference_wording() {
        assert_eq!(
            record_plain_line(RecordAction::Create, "d", None, true),
            "✓ Dry-run: Would create convention in d\n  Run without --dry-run to apply changes."
        );
        assert_eq!(
            record_plain_line(RecordAction::Create, "d", Some("pattern"), false),
            "✓ Recorded pattern in d"
        );
        assert_eq!(
            record_plain_line(RecordAction::Skip { index: 1 }, "d", None, true),
            "Dry-run: Duplicate convention already exists in d. Would skip.\n  Run without --dry-run to apply changes."
        );
        assert_eq!(
            record_plain_line(RecordAction::Skip { index: 1 }, "d", None, false),
            "Duplicate convention already exists in d (record #2). Use --force to add anyway."
        );
        assert_eq!(
            record_plain_line(
                RecordAction::Update { index: 4 },
                "d",
                Some("reference"),
                true
            ),
            "✓ Dry-run: Would update existing reference in d\n  Run without --dry-run to apply changes."
        );
        assert_eq!(
            record_plain_line(RecordAction::Update { index: 4 }, "d", Some("guide"), false),
            "✓ Updated existing guide in d (record #5)"
        );
    }
}
