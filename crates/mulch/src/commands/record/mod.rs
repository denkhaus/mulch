//! `mulch record` — the record-creation surface: the command
//! prologue (store phases, auto-create, `--type` contract) and
//! dispatch into the flag path, the stdin/batch family, and the
//! report family (mulch-d524 made every surface custom-type aware
//! over the merged registry). The pre/post-record lifecycle hooks
//! (mulch-7f49) bracket the locked write exactly like the reference:
//! pre-record fires BEFORE the writer lock (skipped in dry-run),
//! post-record AFTER the actual write, outside the lock.

use mulch::hooks::HookEvent;
use mulch::truthy_str;
use serde_json::{Map, Value, json};

use crate::cli::{GlobalOpts, RecordArgs};
use crate::commands::failure::now_iso;
use crate::commands::hooks_surface::{Adopt, HookPhase};
use crate::commands::record::batch::{read_batch_file, stdin_batch};
use crate::commands::record::flag::{build_record, evidence_map};
use crate::commands::record::report::{RecordAction, report_record_action, skip_advisory};
use crate::output::{Failure, print_line};

mod batch;
mod flag;
mod report;

pub(super) fn run(opts: &GlobalOpts, args: &RecordArgs) -> Result<(), Failure> {
    // Phase 1 opens the store LOCK-FREE (mulch-7f49): the reference
    // reads its config, auto-creates the domain, and runs the
    // pre-record hooks before any file lock; only the load→mutate→save
    // cycle below takes the writer lock (`--dry-run` never locks at
    // all, mulch-228d).
    let mut store = crate::commands::store::open_store("record", false)?;

    // JS truthiness on the --batch flag: `--batch ""` is NO batch (the
    // reference's `if (options.batch)` gate) — the flag path runs with
    // its own --type check.
    let batch = truthy_str(args.batch.as_deref()).map(std::path::Path::new);

    // The reference checks the batch file BEFORE any store side effect
    // (the existsSync gate leads the command): no auto-create, no
    // domain write. The json envelope rides the stderr channel with a
    // capital-B error where the plain line says `Error: ` lowercase
    // (mulch-8ca8).
    if let Some(path) = batch
        && !path.exists()
    {
        let mut failure = Failure::handled_on_stderr(
            "record",
            format!("Error: batch file not found: {}", path.display()),
        );
        failure.envelope["error"] =
            Value::String(format!("Batch file not found: {}", path.display()));
        return Err(failure);
    }

    // The batch READ also precedes every store side effect (the
    // reference's readFileSync leads processStdinRecords' silent
    // addDomain): EISDIR/EACCES surface the Node io-error bytes on
    // stderr in both channels with no domain registration
    // (mulch-72d1).
    let batch_raw = match batch {
        Some(path) => Some(read_batch_file(path)?),
        None => None,
    };

    // Auto-create precedes validation AND dry-run (reference behavior:
    // the domain side effects happen even for dry-runs and failed
    // records) — but the batch/stdin family creates the domain
    // SILENTLY (processStdinRecords has no notice line); the line is
    // the single-record path's alone (mulch-5fd8). It hits stdout
    // even in --json mode; it is quiet-gated (spec review).
    let known = store.domains().iter().any(|d| d == &args.domain);
    if !known {
        store
            .register_domain(&args.domain)
            .map_err(|source| Failure::handled("record", crate::output::chain_message(&source)))?;
        if !args.stdin && batch.is_none() {
            print_line(
                opts.quiet,
                &format!("✓ Auto-created domain \"{}\"", args.domain),
            );
        }
    }

    // Batch wins over stdin when both are given (the reference's
    // batch block precedes the stdin block); the batch content
    // arrives pre-read above.
    if args.stdin || batch.is_some() {
        return stdin_batch(opts, args, batch_raw, &store);
    }

    // Validate --type is provided for non-stdin mode (reference
    // record.ts order: the check follows the auto-create side
    // effect; nothing is written). `--type ""` never reaches the
    // command — clap's value_parser rejects it before, like the
    // reference's commander choices — so only absence is left
    // (mulch-cbe2). stdin/batch records default to convention in
    // both binaries (effective_type law, mulch-3f08).
    if args.record_type.is_none() {
        // the registry's names, builtins first then customs — the
        // reference's typeChoices (probed: customs join the list,
        // mulch-d524)
        let choices = store
            .registry()
            .rows()
            .iter()
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(Failure::handled_on_stderr(
            "record",
            format!("Error: --type is required ({choices})"),
        ));
    }

    let recorded_at = now_iso();
    let record = match build_record(store.registry(), args, &recorded_at, evidence_map(args)) {
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

    // The pre-record lifecycle hook (mulch-7f49): fires after the
    // record is built and validated but before any file read —
    // outside every lock. Payload `{domain, record}` with the
    // builder's id stripped: the reference sends the built record
    // id-less (ids are write-time, appendRecord's `if (!record.id)`)
    // and re-stamps an adopted record only when the hook left no id.
    // Dry-run skips the whole family (no external side effects while
    // previewing).
    let mut warnings: Vec<String> = Vec::new();
    let mut record = record;
    if !args.dry_run {
        let (adopted, hook_warnings) = pre_record_phase(&store, &args.domain, record)?;
        record = adopted;
        warnings = hook_warnings;
    }

    if args.dry_run {
        // Dry-run previews the write decision over the lock-free
        // read (mulch-228d); no hooks fired, so no warnings exist.
        let existing = crate::commands::store::read_domain_records(
            "record",
            &store,
            &args.domain,
            opts.allow_unknown_types,
        )?;
        let working: Vec<Value> = existing.iter().map(|line| line.record.clone()).collect();
        match mulch::upsert_plan(
            store.registry(),
            &working,
            &Value::Object(record.clone()),
            args.force,
        ) {
            mulch::UpsertPlan::Skip { index } => {
                skip_advisory(opts, args, index, record, &warnings);
            }
            mulch::UpsertPlan::Update { index, .. } => {
                report_record_action(opts, args, RecordAction::Update { index }, Some(record), &[
                ]);
            }
            mulch::UpsertPlan::Create => {
                report_record_action(opts, args, RecordAction::Create, Some(record), &[]);
            }
        }
        return Ok(());
    }

    // Phase 2: the locked load→mutate→save. The writer lock wraps the
    // cycle and drops before the post-record hooks fire — the
    // reference runs them outside the lock so observation hooks can
    // never stall lock holders (mulch-7f49).
    let decision = {
        let locked = crate::commands::store::open_store_mut("record", false)?;
        // Duplicate detection over the STRICT read (reference
        // `readExpertiseFile` + `findDuplicate`): malformed lines and
        // unregistered types abort before the write, and the dedup key is
        // the type's dedup FIELD (registry dedupKey), never the id — a
        // renamed record still dedupes after `edit --name` (mulch-ccf6).
        let existing = crate::commands::store::read_domain_records(
            "record",
            &locked,
            &args.domain,
            opts.allow_unknown_types,
        )?;
        // One decision owner (lib `upsert_plan`): named duplicates upsert
        // with merged outcomes, anonymous ones skip, --force falls through
        // to the create path below.
        let working: Vec<Value> = existing.iter().map(|line| line.record.clone()).collect();
        match mulch::upsert_plan(
            locked.registry(),
            &working,
            &Value::Object(record.clone()),
            args.force,
        ) {
            mulch::UpsertPlan::Skip { index } => Decision::Skip { index },
            mulch::UpsertPlan::Update { index, merged } => {
                // id LAST: lift the record's pre-assigned id over the
                // merged outcomes (the reference's `{ ...record,
                // outcomes }` + write-time id placement, mulch-351e).
                let mut upserted = merged;
                if let Some(id) = upserted.remove("id") {
                    upserted.insert("id".into(), id);
                }
                let mut lines: Vec<Value> =
                    existing.iter().map(|line| line.record.clone()).collect();
                lines[index] = Value::Object(upserted.clone());
                crate::commands::store::persist("record", &locked, &args.domain, &lines)?;
                Decision::Written {
                    action: RecordAction::Update { index },
                    record: upserted,
                }
            }
            mulch::UpsertPlan::Create => {
                let line = Value::Object(record.clone()).to_string();
                locked
                    .append_domain_line(&args.domain, &line)
                    .map_err(|source| {
                        Failure::handled("record", crate::output::chain_message(&source))
                    })?;
                Decision::Written {
                    action: RecordAction::Create,
                    record,
                }
            }
        }
    };

    match decision {
        Decision::Skip { index } => {
            // No post-record for a skipped duplicate (a no-op by
            // design); the pre-record warnings still surface on both
            // channels. The real skip reports no record — only the
            // dry-run preview mirrors one, through skip_advisory
            // above.
            report_record_action(opts, args, RecordAction::Skip { index }, None, &warnings);
        }
        Decision::Written { action, record } => {
            // Post-record fires for actual writes only, payload
            // `{domain, record, action}` carrying the WRITTEN record
            // (id included), outside the lock; failures only warn.
            warnings.extend(post_record_phase(
                &store,
                &args.domain,
                &record,
                action_verb(action),
            ));
            report_record_action(opts, args, action, Some(record), &warnings);
        }
    }
    Ok(())
}

/// The locked write phase's outcome (the reference's `WriteOutcome`):
/// a skip reports only, a write carries its record on to the
/// post-record hook and the report.
enum Decision {
    /// A duplicate no-op: nothing written, no post-record.
    Skip {
        /// The existing duplicate line's 0-based index.
        index: usize,
    },
    /// An actual write: created or updated.
    Written {
        /// Which write happened.
        action: RecordAction,
        /// The written record (id included).
        record: Map<String, Value>,
    },
}

/// The post-record payload's `action` verb of a write decision.
fn action_verb(action: RecordAction) -> &'static str {
    match action {
        RecordAction::Create => "created",
        RecordAction::Update { .. } => "updated",
        RecordAction::Skip { .. } => "skipped",
    }
}

/// The fallback block reason when the engine reports none (the
/// reference's `?? "pre-record hook blocked the write"`).
const BLOCK_DEFAULT: &str = "pre-record hook blocked the write";

/// A hook failure with the reference's two-channel shape: plain mode
/// says `Error: {reason}` on stderr, the json envelope carries the
/// bare reason (probe-pinned).
fn hook_failure(reason: String) -> Failure {
    let mut failure = Failure::handled_on_stderr("record", format!("Error: {reason}"));
    failure.envelope["error"] = Value::String(reason);
    failure
}

/// Stamps the write-time id onto an adopted record: appended LAST
/// when the hook left none (the reference's `if (!record.id)` law),
/// kept verbatim — value AND position — when it carried one.
fn ensure_write_id(
    registry: &mulch::Registry,
    mut record: Map<String, Value>,
) -> Map<String, Value> {
    if !record.contains_key("id") {
        let record_type =
            mulch::effective_type(record.get("type").and_then(Value::as_str)).to_string();
        let id_key = registry.id_key_field(&record_type);
        let id_value = record
            .get(id_key)
            .and_then(Value::as_str)
            .unwrap_or_default();
        record.insert(
            "id".into(),
            Value::String(mulch::record_id(&record_type, id_value)),
        );
    }
    record
}

/// The flag path's pre-record phase: runs the hook family outside
/// every lock, payload `{domain, record}` id-less. A block aborts
/// before any read with the reason on both channels; a valid mutation
/// is adopted (the write-time id re-stamped only when absent), an
/// invalid one fails with the reference's joined sub-errors. Returns
/// the record for the locked phase and the warnings for the report.
fn pre_record_phase(
    store: &mulch::StoreFiles,
    domain: &str,
    record: Map<String, Value>,
) -> Result<(Map<String, Value>, Vec<String>), Failure> {
    // The payload carries the record id-less; the working copy keeps
    // the builder's id — untouched unless a mutation adopts.
    let mut sent = record.clone();
    sent.remove("id");
    let payload = json!({"domain": domain, "record": sent});
    let phase = HookPhase::run(HookEvent::PreRecord, payload, store, "record")?;
    if let Some(reason) = phase.blocked_reason(BLOCK_DEFAULT) {
        return Err(hook_failure(reason.to_string()));
    }
    let mut record = record;
    match phase.adopt_record(store.registry()) {
        Adopt::Keep => {}
        Adopt::Replace(adopted) => record = ensure_write_id(store.registry(), adopted),
        Adopt::Invalid(reason) => return Err(hook_failure(reason)),
    }
    Ok((record, phase.warnings().to_vec()))
}

/// The flag path's post-record phase: fires for actual writes only
/// (created/updated — a skipped duplicate is a no-op), payload
/// `{domain, record, action}` with the written record, outside the
/// writer lock. Failures only warn.
fn post_record_phase(
    store: &mulch::StoreFiles,
    domain: &str,
    record: &Map<String, Value>,
    action: &str,
) -> Vec<String> {
    let payload = json!({"domain": domain, "record": record, "action": action});
    // A post-* event never fails the parent command: an unresolvable
    // cwd skips the observation hook with a warning instead of an
    // error (the write already happened).
    HookPhase::run_warn_only(HookEvent::PostRecord, payload, store, "record")
}
