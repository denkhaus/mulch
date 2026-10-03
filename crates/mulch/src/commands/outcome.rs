//! `mulch outcome <domain> <id>` — append an outcome entry.

use serde_json::{Map, Value};

use crate::cli::{GlobalOpts, OutcomeFlags};
use crate::commands::now_iso;
use crate::output::{Failure, print_json, print_line, success_envelope};

/// Runs `outcome`: appends `{status, recorded_at, duration?, agent?,
/// notes?, test_results?}` (only provided keys) to the record's
/// `outcomes` array at line end.
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

    let mut records = store
        .read_records(domain, opts.allow_unknown_types)
        .map_err(|source| {
            Failure::handled_on_stderr("outcome", crate::commands::render_core_error(&source))
        })?;
    let Some(position) = records.iter().position(|line| line.id() == Some(id)) else {
        return Err(Failure::handled_on_stderr(
            "outcome",
            crate::commands::record_not_found_text(id),
        ));
    };
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
    if let Some(status) = &flags.status {
        outcome.insert("status".into(), Value::String(status.clone()));
    }
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
            &format!(
                "✓ Outcome recorded: {}{attribution} on {id}",
                status_of(flags)
            ),
        );
    }
    Ok(())
}

/// The status string (defaults to success when omitted).
fn status_of(flags: &OutcomeFlags) -> String {
    flags.status.clone().unwrap_or_else(|| "success".into())
}
