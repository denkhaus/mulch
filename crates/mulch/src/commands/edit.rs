//! `mulch edit <domain> <id>` — in-place field updates.

use serde_json::{Map, Value};

use crate::cli::{EditArgs, GlobalOpts};
use crate::output::{Failure, print_json, print_line, success_envelope};

/// Runs `edit`: updates fields in place (key positions preserved),
/// appends files/relates_to/supersedes at line end, never recomputes
/// the id, and succeeds as a no-op when no flags are given.
pub(super) fn run(opts: &GlobalOpts, args: &EditArgs) -> Result<(), Failure> {
    let store = crate::commands::open_store("edit", false)?;

    let domains = store.domains();
    if !domains.iter().any(|d| d == &args.domain) {
        return Err(crate::commands::unknown_domain_failure(
            "edit",
            &args.domain,
            &domains,
            opts.json,
            crate::commands::DomainFailure::Standard,
        ));
    }

    // Strict read (reference `readExpertiseFile`): malformed lines and
    // unregistered types abort before the rewrite.
    let mut records = store
        .read_records(&args.domain, opts.allow_unknown_types)
        .map_err(|source| {
            Failure::handled_on_stderr("edit", crate::commands::render_core_error(&source))
        })?;
    // Identifier resolution like delete/move: exact id, bare hash, or
    // a unique prefix (reference `resolveRecordId`; mulch-351d).
    let position = mulch::resolve_record_id(&records, &args.id)
        .map_err(|error| crate::commands::resolve_failure("edit", error))?;
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
            return Err(Failure::handled_on_stderr(
                "edit",
                format!("Error: Record \"{}\" is not a JSON object.", args.id),
            ));
        }
    };
    let record_type = record
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("convention")
        .to_string();

    // In-place scalar updates keep their key position.
    if let Some(classification) = &args.classification {
        record.insert(
            "classification".into(),
            Value::String(classification.clone()),
        );
    }
    let payload: &[&str] = mulch::payload_fields(&record_type);
    let updates: [(&str, &Option<String>); 6] = [
        ("content", &args.content),
        ("name", &args.name),
        ("description", &args.description),
        ("resolution", &args.resolution),
        ("title", &args.title),
        ("rationale", &args.rationale),
    ];
    for (field, value) in updates {
        if let Some(value) = value
            && payload.contains(&field)
        {
            record.insert(field.into(), Value::String(value.clone()));
        }
    }

    // Record-time-style outcome (no recorded_at inside).
    if args.outcome.status.is_some() {
        append_outcome_without_time(&mut record, &args.outcome, &args.id)?;
    }

    // List updates REPLACE the values (reference `update` semantics);
    // absent keys are appended at line end (order: relates_to,
    // supersedes, files).
    set_end_list(&mut record, "relates_to", args.relates_to.as_deref());
    set_end_list(&mut record, "supersedes", args.supersedes.as_deref());
    set_end_list(&mut record, "files", args.files.as_deref());

    records[position].record = Value::Object(record.clone());
    let payload: Vec<Value> = records.iter().map(|line| line.record.clone()).collect();
    store
        .rewrite_domain(&args.domain, &payload)
        .map_err(|source| Failure::handled("edit", crate::output::chain_message(&source)))?;

    if opts.json {
        let mut fields = serde_json::Map::new();
        fields.insert("domain".into(), Value::String(args.domain.clone()));
        fields.insert("id".into(), Value::String(resolved_id.clone()));
        fields.insert("type".into(), Value::String(record_type));
        fields.insert("record".into(), Value::Object(record));
        print_json(&success_envelope("edit", fields), false);
    } else {
        print_line(
            opts.quiet,
            &format!("✓ Updated {} {resolved_id} in {}", record_type, args.domain),
        );
    }
    Ok(())
}

/// Appends an edit-time outcome object (no recorded_at).
fn append_outcome_without_time(
    record: &mut Map<String, Value>,
    flags: &crate::cli::EditOutcomeFlags,
    id: &str,
) -> Result<(), Failure> {
    let mut outcome = Map::new();
    if let Some(status) = &flags.status {
        outcome.insert("status".into(), Value::String(status.clone()));
    }
    if let Some(number) = flags
        .duration
        .as_deref()
        .and_then(|d| d.parse::<u64>().ok())
    {
        outcome.insert("duration".into(), Value::from(number));
    }
    if let Some(test_results) = &flags.test_results {
        outcome.insert("test_results".into(), Value::String(test_results.clone()));
    }
    if let Some(agent) = &flags.agent {
        outcome.insert("agent".into(), Value::String(agent.clone()));
    }
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
