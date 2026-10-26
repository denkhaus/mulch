//! `mulch edit <domain> <id>` — in-place field updates.

use serde_json::{Map, Value};

use crate::cli::{EditArgs, GlobalOpts};
use crate::commands::failure::DomainFailure;
use crate::commands::input::PayloadFlags;
use crate::commands::store::locate_record;
use crate::output::{Failure, print_json, print_line, success_envelope};

/// Runs `edit`: updates fields in place (key positions preserved),
/// appends files/relates_to/supersedes at line end, never recomputes
/// the id, and succeeds as a no-op when no flags are given.
pub(super) fn run(opts: &GlobalOpts, args: &EditArgs) -> Result<(), Failure> {
    // The record-locator seam: open store, check domain, strict read
    // (reference `readExpertiseFile`), resolve the id. The canonical
    // domain comes back with the read — every write below addresses
    // it (an alias never creates a file, ADR-0006/mulch-8452).
    let (store, domain, mut records, position) = locate_record(
        opts,
        "edit",
        &args.domain,
        &args.id,
        DomainFailure::Standard,
        false,
        true,
    )?;
    // Output surfaces carry the resolved record's own id (reference
    // `record.id`; its `?? id` fallback is dead — resolution matches
    // only identified records).
    let resolved_id = records[position]
        .id()
        .expect("resolve_record_id matches only identified records")
        .to_string();
    let mut record: Map<String, Value> = match records[position].record.as_object().cloned() {
        Some(object) => object,
        None => {
            return Err(Failure::error_line(
                "edit",
                format!("Record \"{}\" is not a JSON object.", args.id),
            ));
        }
    };
    let record_type = mulch::effective_type(record.get("type").and_then(Value::as_str)).to_string();

    // The reference's application order (edit.ts): classification,
    // then the base list props (relates_to, supersedes), then the
    // outcome entry, then the type's own payload and optional fields
    // — absent keys append at line end in that order.
    if let Some(classification) = &args.classification {
        record.insert(
            "classification".into(),
            Value::String(classification.clone()),
        );
    }
    set_end_list(&mut record, "relates_to", args.relates_to.as_deref());
    set_end_list(&mut record, "supersedes", args.supersedes.as_deref());

    // Record-time-style outcome (no recorded_at inside).
    if let Some(status) = &args.outcome.status {
        append_outcome_without_time(&mut record, status, &args.outcome, &args.id)?;
    }

    // The type's payload fields through the shared flag table: scalars
    // replace in place, and `files` applies only where the type
    // declares it (pattern and reference — the reference's def loop
    // drops the flag for every other type, so no unknown extra field
    // is ever written).
    // The reference's def loop (edit.ts:151-167): the record type's
    // required ∪ optional fields read the RAW flag view — `value ===
    // undefined` is the only gate, empty strings store — with the
    // extracts-files field splitting its comma string into an array.
    // Unregistered types write NO payload fields (def undefined);
    // undeclared flags never reach a record (mulch-d524).
    let flags = PayloadFlags::from_edit(args);
    if let Some(row) = store.registry().row(&record_type) {
        for field in row.payload.iter().chain(row.optional.iter()) {
            let Some(value) = flags.get(field) else {
                continue;
            };
            if row.files_array.as_deref() == Some(field.as_str()) {
                set_end_list(&mut record, field, Some(value));
            } else {
                record.insert(field.as_str().into(), Value::String(value.to_string()));
            }
        }
    }

    // The write gate (mulch-7896): the UPDATED record must pass the
    // schema or nothing is written (reference edit.ts).
    let updated = Value::Object(record.clone());
    if let mulch::schema::FullVerdict::Invalid { subs, .. } =
        store.updated_line_verdict(&domain, records[position].line, &updated)
    {
        return Err(updated_record_failed(&subs, opts.json));
    }

    records[position].record = updated;
    let payload: Vec<Value> = records.iter().map(|line| line.record.clone()).collect();
    crate::commands::store::persist("edit", &store, &domain, &payload)?;

    if opts.json {
        let mut fields = serde_json::Map::new();
        fields.insert("domain".into(), Value::String(domain.clone()));
        fields.insert("id".into(), Value::String(resolved_id.clone()));
        fields.insert("type".into(), Value::String(record_type));
        fields.insert("record".into(), Value::Object(record));
        print_json(&success_envelope("edit", fields), false);
    } else {
        print_line(
            opts.quiet,
            &format!("✓ Updated {record_type} {resolved_id} in {domain}"),
        );
    }
    Ok(())
}

/// The write-gate failure (reference edit.ts): plain mode prints the
/// header then one `  `-prefixed ajv line per sub-error on stderr;
/// json mode carries the `; `-joined sub-errors behind a capital-U
/// wrapper in the error envelope (also stderr); exit 1 either way and
/// nothing is written.
fn updated_record_failed(subs: &[mulch::schema::SubError], json: bool) -> Failure {
    let rendered = mulch::schema::render_subs(subs);
    if json {
        return Failure::handled_on_stderr(
            "edit",
            format!(
                "Updated record failed schema validation: {}",
                rendered.join(mulch::schema::SUB_SEP)
            ),
        );
    }
    let mut message = String::from("Error: updated record failed schema validation:");
    for sub in &rendered {
        message.push_str("\n  ");
        message.push_str(sub);
    }
    let mut failure = Failure::handled_on_stderr("edit", "");
    failure.message = message;
    failure
}

/// Appends an edit-time outcome object (no recorded_at). The entry
/// shape and the strict duration parse live in the lib seam; the
/// caller gates on `status` being present (the reference builds the
/// outcome only with `--outcome-status`).
fn append_outcome_without_time(
    record: &mut Map<String, Value>,
    status: &str,
    flags: &crate::cli::EditOutcomeFlags,
    id: &str,
) -> Result<(), Failure> {
    let outcome = mulch::OutcomeEntry {
        timestamped: false,
        status,
        now: "",
        duration: flags.duration.as_deref(),
        duration_flag: "--outcome-duration",
        agent: flags.agent.as_deref(),
        notes: None,
        test_results: flags.test_results.as_deref(),
    }
    .build()
    .map_err(|message| Failure::error_line("edit", message))?;
    let outcomes_entry = record
        .entry("outcomes")
        .or_insert_with(|| Value::Array(Vec::new()));
    let Some(array) = outcomes_entry.as_array_mut() else {
        return Err(Failure::handled(
            "edit",
            format!("record {id} carries a non-array outcomes field"),
        ));
    };
    array.push(Value::Object(outcome));
    Ok(())
}

/// Replaces (or appends at line end) a string-list field.
fn set_end_list(record: &mut Map<String, Value>, key: &str, raw: Option<&str>) {
    let Some(items) = raw else {
        return;
    };
    let parsed: Vec<Value> = items
        .split(',')
        .filter(|item| !item.trim().is_empty())
        .map(|item| Value::String(item.trim().to_string()))
        .collect();
    // Replace-or-append is the same insert for an order-preserving map
    // when the position at line end is the desired one for new keys.
    record.insert(key.into(), Value::Array(parsed));
}

#[cfg(test)]
mod gate_tests {
    use mulch::schema::SubError;

    use super::updated_record_failed;

    /// The plain surface: header line, then one `  `-prefixed ajv
    /// line per sub-error (probe-pinned 2026-10-11, ml 0.10.7).
    #[test]
    fn plain_gate_renders_header_and_indented_subs() {
        let failure = updated_record_failed(
            &[
                SubError::Missing("content".into()),
                SubError::TypeConst,
                SubError::OneOfTail,
            ],
            false,
        );
        assert_eq!(
            failure.message,
            "Error: updated record failed schema validation:\n   must have required property 'content'\n  /type must be equal to constant\n   must match exactly one schema in oneOf"
        );
    }

    /// The json surface: capital-U wrapper, `; `-join, the sub's own
    /// path space producing the double gap (envelope channel is
    /// stderr, pinned by the battery).
    #[test]
    fn json_gate_joins_subs_behind_the_wrapper() {
        let failure = updated_record_failed(&[SubError::Missing("content".into())], true);
        assert_eq!(
            failure.envelope["error"],
            "Updated record failed schema validation:  must have required property 'content'"
        );
    }
}
