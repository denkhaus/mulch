//! `mulch outcome <domain> <id>` — append an outcome entry.

use serde_json::{Map, Value};

use crate::cli::{GlobalOpts, OutcomeFlags};
use crate::commands::edit::not_found;
use crate::commands::{NO_STORE_MESSAGE, StoreLocation, domain_file, locate, now_iso};
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

    let file = domain_file(&store.root, domain);
    let text = std::fs::read_to_string(&file).unwrap_or_default();
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let position = lines.iter().position(|line| {
        serde_json::from_str::<Value>(line)
            .ok()
            .and_then(|r| r.get("id").and_then(Value::as_str).map(String::from))
            .is_some_and(|existing| existing == id)
    });
    let Some(position) = position else {
        return Err(not_found(id));
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
    let mut updated = lines.join("\n");
    updated.push('\n');
    std::fs::write(&file, updated).map_err(|source| {
        Failure::handled("outcome", format!("writing {}: {source}", file.display()))
    })?;

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
