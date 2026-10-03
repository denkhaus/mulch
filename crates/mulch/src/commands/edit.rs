//! `mulch edit <domain> <id>` — in-place field updates.

use serde_json::{Map, Value};

use crate::cli::{EditArgs, GlobalOpts, OutcomeFlags};
use crate::commands::{NO_STORE_MESSAGE, StoreLocation, domain_file, locate};
use crate::output::{Failure, print_json, print_line, success_envelope};

/// Payload fields per record type (same table as `record`).
const PAYLOAD_FIELDS: [(&str, &[&str]); 6] = [
    ("convention", &["content"]),
    ("pattern", &["name", "description"]),
    ("failure", &["description", "resolution"]),
    ("decision", &["title", "rationale"]),
    ("reference", &["name", "description"]),
    ("guide", &["name", "description"]),
];

/// Runs `edit`: updates fields in place (key positions preserved),
/// appends files/relates_to/supersedes at line end, never recomputes
/// the id, and succeeds as a no-op when no flags are given.
pub(super) fn run(opts: &GlobalOpts, args: &EditArgs) -> Result<(), Failure> {
    let cwd = std::env::current_dir()
        .map_err(|source| Failure::handled("edit", format!("resolving cwd: {source}")))?;
    let store = match locate(&cwd) {
        Ok(StoreLocation::Open(store)) => store,
        Ok(_) => {
            let mut failure = Failure::handled("edit", NO_STORE_MESSAGE);
            failure.envelope_to_stderr = true;
            return Err(failure);
        }
        Err(source) => {
            return Err(Failure::handled(
                "edit",
                crate::output::chain_message(&source),
            ));
        }
    };

    let domains = store.domains();
    if !domains.iter().any(|d| d == &args.domain) {
        let list = domains.join(", ");
        return Err(Failure::handled(
            "edit",
            format!(
                "Error: domain \"{}\" not found in config.\nAvailable domains: {list}",
                args.domain
            ),
        ));
    }

    let file = domain_file(&store.root, &args.domain);
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let position = lines.iter().position(|line| {
        serde_json::from_str::<Value>(line)
            .ok()
            .and_then(|r| r.get("id").and_then(Value::as_str).map(str::to_string))
            .is_some_and(|existing| existing == args.id)
    });
    let Some(position) = position else {
        return Err(not_found(&args.id));
    };

    let mut record: Map<String, Value> =
        serde_json::from_str(&lines[position]).map_err(|source| {
            Failure::handled("edit", format!("parsing record {}: {source}", args.id))
        })?;
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
    let payload: &[&str] = PAYLOAD_FIELDS
        .iter()
        .find(|(name, _)| name == &record_type)
        .map_or(&[], |(_, fields)| *fields);
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
        append_outcome_without_time(&mut record, &args.outcome);
    }

    // End-of-line array extensions (order: relates_to, supersedes, files).
    extend_end_list(&mut record, "relates_to", args.relates_to.as_deref());
    extend_end_list(&mut record, "supersedes", args.supersedes.as_deref());
    extend_end_list(&mut record, "files", args.files.as_deref());

    lines[position] = Value::Object(record.clone()).to_string();
    let mut updated = lines.join("\n");
    updated.push('\n');
    std::fs::write(&file, updated).map_err(|source| {
        Failure::handled("edit", format!("writing {}: {source}", file.display()))
    })?;

    if opts.json {
        let mut fields = serde_json::Map::new();
        fields.insert("domain".into(), Value::String(args.domain.clone()));
        fields.insert("id".into(), Value::String(args.id.clone()));
        fields.insert("type".into(), Value::String(record_type));
        fields.insert("record".into(), Value::Object(record));
        print_json(&success_envelope("edit", fields), false);
    } else {
        print_line(
            opts.quiet,
            &format!("✓ Updated {} {} in {}", record_type, args.id, args.domain),
        );
    }
    Ok(())
}

/// The unknown-id failure (reference text).
pub(super) fn not_found(id: &str) -> Failure {
    Failure::handled(
        "edit",
        format!("Error: Record \"{id}\" not found. Run `mulch query` to see record IDs."),
    )
}

/// Appends an edit-time outcome object (no recorded_at).
fn append_outcome_without_time(record: &mut Map<String, Value>, flags: &OutcomeFlags) {
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
    record
        .entry("outcomes")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .expect("outcomes stays an array")
        .push(Value::Object(outcome));
}

/// Extends (or appends at line end) a string-list field.
fn extend_end_list(record: &mut Map<String, Value>, key: &str, raw: Option<&str>) {
    let Some(items) = raw else {
        return;
    };
    let parsed: Vec<Value> = items
        .split(',')
        .filter(|item| !item.trim().is_empty())
        .map(|item| Value::String(item.trim().to_string()))
        .collect();
    match record.get_mut(key) {
        Some(Value::Array(existing)) => existing.extend(parsed),
        _ => {
            record.insert(key.into(), Value::Array(parsed));
        }
    }
}
