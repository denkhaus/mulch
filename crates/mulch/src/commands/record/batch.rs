//! The `record` stdin/batch family: JSON input parsing (including
//! the reference's JavaScriptCore parse-error bytes, mulch-943b, and
//! the Node io-error bytes for unreadable batch paths, mulch-72d1),
//! the mixed-batch partial-write contract, and the batch summary
//! surfaces (mulch-8c9c, mulch-ca49).

use std::fmt::Write as _;
use std::io::Read as _;
use std::path::Path;

use mulch::json_error::jsc_parse_error;
use mulch::{record_id, schema};
use serde_json::Value;

use crate::cli::{GlobalOpts, RecordArgs};
use crate::commands::now_iso;
use crate::output::{Failure, failure_envelope, print_json, print_line};

/// Reads the batch file before any store side effect (the
/// reference's `readFileSync` leads `processStdinRecords`' silent
/// addDomain): unreadable paths fail with the Node io-error bytes on
/// stderr in both channels, exit 1, no domain registration
/// (mulch-72d1). The decode is lossy, like the reference's `utf-8`
/// read of binary files.
pub(super) fn read_batch_file(path: &Path) -> Result<String, Failure> {
    std::fs::read(path)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .map_err(|source| {
            Failure::handled_on_stderr(
                "record",
                format!("Error: {}", node_read_error(path, &source)),
            )
        })
}

/// The reference's `readFileSync` failure text for the batch path
/// (Node/Bun io-error rendering, probed against ml 0.10.7): a
/// directory reads `EISDIR … read` with no path (the open succeeds,
/// the read fails), everything else appends the open syscall and the
/// path exactly as given.
fn node_read_error(path: &Path, error: &std::io::Error) -> String {
    if error.raw_os_error() == Some(libc_eisdir()) {
        return "EISDIR: illegal operation on a directory, read".into();
    }
    // libuv's strerror texts are lowercase (Node renders
    // `EACCES: permission denied, open '<path>'`); Rust's io Display
    // capitalizes, so the codes we map carry their libuv wording.
    let (code, detail) = match error.raw_os_error() {
        Some(1) => ("EPERM", String::from("operation not permitted")),
        Some(2) => ("ENOENT", String::from("no such file or directory")),
        Some(13) => ("EACCES", String::from("permission denied")),
        Some(20) => ("ENOTDIR", String::from("not a directory")),
        Some(36) => ("ENAMETOOLONG", String::from("name too long")),
        Some(40) => ("ELOOP", String::from("too many symbolic links encountered")),
        _ => {
            let text = error.to_string();
            let stripped = text
                .split_once(" (os error")
                .map_or(text.as_str(), |(before, _)| before);
            ("EUNKNOWN", stripped.to_string())
        }
    };
    format!("{code}: {detail}, open '{}'", path.display())
}

/// EISDIR's errno on this platform (21 on Linux, the only host the
/// reference parity surface targets).
fn libc_eisdir() -> i32 {
    21
}

/// stdin/batch path: preserves input key order, appends recorded_at,
/// classification, id. The batch file wins over stdin when both are
/// given (the reference's batch block precedes the stdin block); the
/// batch content arrives pre-read by the caller.
pub(super) fn stdin_batch(
    opts: &GlobalOpts,
    args: &RecordArgs,
    batch: Option<String>,
    store: &mulch::StoreFiles,
) -> Result<(), Failure> {
    let raw = if let Some(text) = batch {
        text
    } else {
        let mut buffer = Vec::new();
        std::io::stdin()
            .read_to_end(&mut buffer)
            .map_err(|source| Failure::handled("record", format!("reading stdin: {source}")))?;
        String::from_utf8_lossy(&buffer).into_owned()
    };
    // The reference parses with JavaScriptCore's JSON.parse (Bun
    // runtime): failures say `Failed to parse JSON from stdin: JSON
    // Parse error: …` on stderr in BOTH channels, exit 1, after the
    // silent domain auto-create (mulch-943b).
    let parsed: Value = match serde_json::from_str(&raw) {
        Ok(value) => value,
        Err(source) => {
            return Err(match jsc_parse_error(&raw) {
                Some(detail) => Failure::handled_on_stderr(
                    "record",
                    format!("Error: Failed to parse JSON from stdin: JSON Parse error: {detail}"),
                ),
                // JSC accepts what serde rejected (a lone \u surrogate
                // decodes there): keep the serde wording for that
                // corner instead of inventing JSC bytes.
                None => Failure::handled("record", format!("parsing input: {source}")),
            });
        }
    };
    let items: Vec<Value> = match parsed {
        Value::Array(items) => items,
        single => vec![single],
    };

    let existing = crate::commands::read_domain_records(
        "record",
        store,
        &args.domain,
        opts.allow_unknown_types,
    )?;
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
            schema::full_verdict(store.registry(), &Value::Object(line.clone()))
        {
            // Reference batch entry: `Record ${i}: ${subs}` with the
            // type hint only when the record declares a registered
            // type (`requirements[recordType]` is undefined otherwise).
            let registered = line
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|kind| store.registry().is_known(kind));
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
        let id_key = store.registry().id_key_field(&record_type);
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
        let plan = mulch::upsert_plan(
            store.registry(),
            &working,
            &Value::Object(line.clone()),
            args.force,
        );

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
        crate::commands::persist("record", store, &args.domain, &working)?;
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
        let mut fields = serde_json::Map::new();
        fields.insert(
            "error".into(),
            Value::String(format!("Validation errors: {summary}")),
        );
        print_json(&failure_envelope("record", fields), true);
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
