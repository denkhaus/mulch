//! `mulch outcome <domain> <id>` — append an outcome entry.

use serde_json::{Map, Value};

use crate::cli::{GlobalOpts, OutcomeFlags};
use crate::commands::{
    NO_STORE_MESSAGE, StoreLocation, find_by_id, locate, now_iso, read_domain_lines,
    record_not_found, write_domain_lines,
};
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
    let cwd = std::env::current_dir()
        .map_err(|source| Failure::handled("outcome", format!("resolving cwd: {source}")))?;
    let store = match locate(&cwd) {
        Ok(StoreLocation::Open(store)) => store,
        Ok(_) => {
            let mut failure = Failure::handled("outcome", NO_STORE_MESSAGE);
            failure.envelope_to_stderr = true;
            return Err(failure);
        }
        Err(source) => {
            return Err(Failure::handled(
                "outcome",
                crate::output::chain_message(&source),
            ));
        }
    };

    let domains = store.domains();
    if !domains.iter().any(|d| d == domain) {
        let list = domains.join(", ");
        return Err(Failure::handled(
            "outcome",
            format!("domain \"{domain}\" not found in config.\nAvailable domains: {list}"),
        ));
    }

    let mut lines = read_domain_lines(&store.root, domain)
        .map_err(|source| Failure::handled("outcome", format!("reading domain file: {source}")))?;
    let Some(position) = find_by_id(&lines, id) else {
        let mut failure = record_not_found("outcome", id);
        failure.envelope_to_stderr = true;
        return Err(failure);
    };

    let mut record: Map<String, Value> = serde_json::from_str(&lines[position])
        .map_err(|source| Failure::handled("outcome", format!("parsing record {id}: {source}")))?;
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
    lines[position] = Value::Object(record).to_string();
    write_domain_lines(&store.root, domain, &lines)
        .map_err(|source| Failure::handled("outcome", format!("writing domain file: {source}")))?;

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
