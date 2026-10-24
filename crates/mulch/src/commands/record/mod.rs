//! `mulch record` — the record-creation surface: the command
//! prologue (store, writer lock, auto-create, `--type` contract) and
//! dispatch into the flag path, the stdin/batch family, and the
//! report family (mulch-d524 made every surface custom-type aware
//! over the merged registry).

use mulch::truthy_str;
use serde_json::Value;

use crate::cli::{GlobalOpts, RecordArgs};
use crate::commands::now_iso;
use crate::commands::record::batch::{read_batch_file, stdin_batch};
use crate::commands::record::flag::{build_record, evidence_map};
use crate::commands::record::report::{
    RecordAction, report_record_action, skip_advisory, upsert_update,
};
use crate::output::{Failure, print_line};

mod batch;
mod flag;
mod report;

pub(super) fn run(opts: &GlobalOpts, args: &RecordArgs) -> Result<(), Failure> {
    // The writer lock wraps the whole load→mutate→save cycle of real
    // runs; `--dry-run` reads lock-free like the reference's dry-run
    // branch (mulch-228d).
    let mut store = if args.dry_run {
        crate::commands::open_store("record", false)?
    } else {
        crate::commands::open_store_mut("record", false)?
    };

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

    // Duplicate detection over the STRICT read (reference
    // `readExpertiseFile` + `findDuplicate`): malformed lines and
    // unregistered types abort before the write, and the dedup key is
    // the type's dedup FIELD (registry dedupKey), never the id — a
    // renamed record still dedupes after `edit --name` (mulch-ccf6).
    let existing = crate::commands::read_domain_records(
        "record",
        &store,
        &args.domain,
        opts.allow_unknown_types,
    )?;
    // One decision owner (lib `upsert_plan`): named duplicates upsert
    // with merged outcomes, anonymous ones skip, --force falls through
    // to the create path below.
    let working: Vec<Value> = existing.iter().map(|line| line.record.clone()).collect();
    match mulch::upsert_plan(
        store.registry(),
        &working,
        &Value::Object(record.clone()),
        args.force,
    ) {
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
