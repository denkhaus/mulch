//! `mulch outcome <domain> <id>` — append an outcome entry or list
//! the record's outcomes (mulch-b88b).

use serde_json::{Map, Value};

use crate::cli::{GlobalOpts, OutcomeFlags};
use crate::commands::now_iso;
use crate::output::{Failure, print_json, print_line, success_envelope};

/// Runs `outcome`: with `--status` it appends
/// `{status, recorded_at, duration?, agent?, notes?, test_results?}`
/// (only provided keys) to the record's `outcomes` array at line end.
/// Without `--status` it is read-only: it lists the record's outcomes
/// and never writes (reference `commands/outcome.ts`).
pub(super) fn run(
    opts: &GlobalOpts,
    domain: &str,
    id: &str,
    flags: &OutcomeFlags,
) -> Result<(), Failure> {
    let store = crate::commands::open_store("outcome", false)?;

    let domains = store.domains();
    if !domains.iter().any(|d| d == domain) {
        return Err(if opts.json {
            crate::commands::domain_not_found_json("outcome", domain, &domains)
        } else {
            crate::commands::domain_not_found("outcome", domain, &domains)
        });
    }

    match flags.status.as_deref() {
        Some(status) => append(opts, &store, domain, id, status, flags),
        None => list(opts, &store, domain, id),
    }
}

/// Reads the domain strictly and locates the record whose `id` equals
/// `id` exactly (identifier resolution stays mulch-351d's scope).
fn read_and_locate(
    store: &mulch::StoreFiles,
    domain: &str,
    id: &str,
    allow_unknown_types: bool,
) -> Result<(Vec<mulch::LineRecord>, usize), Failure> {
    let records = store
        .read_records(domain, allow_unknown_types)
        .map_err(|source| {
            Failure::handled_on_stderr("outcome", crate::commands::render_core_error(&source))
        })?;
    let Some(position) = records.iter().position(|line| line.id() == Some(id)) else {
        return Err(Failure::handled_on_stderr(
            "outcome",
            crate::commands::record_not_found_text(id),
        ));
    };
    Ok((records, position))
}

/// The read-only branch (no `--status`): prints the record's outcomes
/// — empty notice, listing, or JSON envelope — and leaves the store
/// untouched (reference contract; the pre-b88b code appended a
/// schema-invalid outcome instead).
fn list(
    opts: &GlobalOpts,
    store: &mulch::StoreFiles,
    domain: &str,
    id: &str,
) -> Result<(), Failure> {
    let (records, position) = read_and_locate(store, domain, id, opts.allow_unknown_types)?;
    // The header carries the record's own id (the input id only when
    // the record has none — impossible while matching is exact).
    let record_id = records[position].id().unwrap_or(id);
    let outcomes = records[position]
        .record
        .get("outcomes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    if opts.json {
        let mut fields = Map::new();
        fields.insert("domain".into(), Value::String(domain.into()));
        fields.insert("id".into(), Value::String(record_id.into()));
        fields.insert("outcomes".into(), Value::Array(outcomes));
        print_json(&success_envelope("outcome", fields), false);
    } else {
        for line in listing_lines(record_id, &outcomes) {
            print_line(opts.quiet, &line);
        }
    }
    Ok(())
}

/// The plain listing (reference rendering; chalk styling strips when
/// piped, which is the byte contract our output pins).
fn listing_lines(id: &str, outcomes: &[Value]) -> Vec<String> {
    if outcomes.is_empty() {
        return vec!["No outcomes recorded for this record.".into()];
    }
    let mut lines = Vec::with_capacity(outcomes.len() * 5 + 1);
    lines.push(format!("Outcomes for {id} ({}):", outcomes.len()));
    for (index, outcome) in outcomes.iter().enumerate() {
        let fields = outcome.as_object();
        // The reference template-stringifies a missing status as
        // "undefined"; keep that byte shape for hand-edited stores.
        let status = fields
            .and_then(|fields| fields.get("status"))
            .map_or_else(|| "undefined".into(), value_text);
        let attribution = fields
            .and_then(|fields| fields.get("agent"))
            .filter(|agent| truthy(agent))
            .map_or_else(String::new, |agent| format!(" ({})", value_text(agent)));
        lines.push(format!("  {}. {status}{attribution}", index + 1));
        // `duration` prints when present (even null); the other detail
        // lines only for truthy fields, like the reference's checks.
        if let Some(duration) = fields.and_then(|fields| fields.get("duration")) {
            lines.push(format!("     duration: {}ms", value_text(duration)));
        }
        if let Some(tests) = fields
            .and_then(|fields| fields.get("test_results"))
            .filter(|tests| truthy(tests))
        {
            lines.push(format!("     tests: {}", value_text(tests)));
        }
        if let Some(notes) = fields
            .and_then(|fields| fields.get("notes"))
            .filter(|notes| truthy(notes))
        {
            lines.push(format!("     notes: {}", value_text(notes)));
        }
        if let Some(recorded_at) = fields
            .and_then(|fields| fields.get("recorded_at"))
            .filter(|recorded_at| truthy(recorded_at))
        {
            lines.push(format!("     recorded: {}", value_text(recorded_at)));
        }
    }
    lines
}

/// The append branch (`--status` set): appends the outcome and
/// rewrites the domain file compactly.
fn append(
    opts: &GlobalOpts,
    store: &mulch::StoreFiles,
    domain: &str,
    id: &str,
    status: &str,
    flags: &OutcomeFlags,
) -> Result<(), Failure> {
    let (mut records, position) = read_and_locate(store, domain, id, opts.allow_unknown_types)?;
    let mut record: Map<String, Value> = match records[position].record.as_object().cloned() {
        Some(object) => object,
        None => {
            return Err(Failure::handled_on_stderr(
                "outcome",
                format!("Error: Record \"{id}\" is not a JSON object."),
            ));
        }
    };
    let mut outcome = Map::new();
    outcome.insert("status".into(), Value::String(status.into()));
    outcome.insert("recorded_at".into(), Value::String(now_iso()));
    if let Some(number) = flags
        .duration
        .as_deref()
        .and_then(|d| d.parse::<u64>().ok())
    {
        outcome.insert("duration".into(), Value::from(number));
    }
    if let Some(agent) = &flags.agent {
        outcome.insert("agent".into(), Value::String(agent.clone()));
    }
    if let Some(notes) = &flags.notes {
        outcome.insert("notes".into(), Value::String(notes.clone()));
    }
    if let Some(test_results) = &flags.test_results {
        outcome.insert("test_results".into(), Value::String(test_results.clone()));
    }

    let total = {
        let array = record
            .entry("outcomes")
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .expect("outcomes stays an array");
        array.push(Value::Object(outcome.clone()));
        array.len()
    };
    records[position].record = Value::Object(record);
    let payload: Vec<Value> = records.iter().map(|line| line.record.clone()).collect();
    store
        .rewrite_domain(domain, &payload)
        .map_err(|source| Failure::handled("outcome", crate::output::chain_message(&source)))?;

    if opts.json {
        let mut fields = serde_json::Map::new();
        fields.insert("action".into(), Value::String("appended".into()));
        fields.insert("domain".into(), Value::String(domain.into()));
        fields.insert("id".into(), Value::String(id.into()));
        fields.insert("outcome".into(), Value::Object(outcome));
        fields.insert("total_outcomes".into(), Value::from(total as u64));
        print_json(&success_envelope("outcome", fields), false);
    } else {
        let attribution = flags
            .agent
            .as_deref()
            .map_or_else(String::new, |agent| format!(" ({agent})"));
        print_line(
            opts.quiet,
            &format!("✓ Outcome recorded: {status}{attribution} on {id}"),
        );
    }
    Ok(())
}

/// Renders a JSON value the way a JS template literal would (strings
/// raw, `null` as "null", numbers and booleans as text).
fn value_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => "null".into(),
        other => other.to_string(),
    }
}

/// JS truthiness for the optional listing fields (empty string, 0 and
/// null are skipped, like the reference's bare field checks).
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_some_and(|n| n != 0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(fields) => !fields.is_empty(),
    }
}
