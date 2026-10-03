//! `mulch delete` — remove records (single id, `--records`,
//! `--all-except`), rewriting the file from surviving raw lines.

use serde_json::{Map, Value};

use crate::cli::GlobalOpts;
use crate::commands::{
    NO_STORE_MESSAGE, StoreLocation, domain_not_found, locate, parsed_lines, read_domain_lines,
    record_not_found, record_summary, write_domain_lines,
};
use crate::output::{Failure, print_json, print_line, success_envelope};

/// Which records the invocation targets (reference mode exclusivity).
pub(super) enum Mode {
    /// One positional id.
    Single(String),
    /// `--records` list.
    Records(Vec<String>),
    /// `--all-except` list.
    AllExcept(Vec<String>),
    /// Nothing provided (error).
    Missing,
    /// An id combined with `--records`/`--all-except` (error).
    Conflict,
}

impl Mode {
    /// Parses the mutually exclusive id sources.
    pub(super) fn from_args(
        id: Option<&str>,
        records: Option<&str>,
        all_except: Option<&str>,
    ) -> Self {
        let list = |raw: &str| -> Vec<String> {
            raw.split(',')
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect()
        };
        match (id, records, all_except) {
            (Some(id), None, None) => Mode::Single(id.to_string()),
            (None, Some(raw), None) => Mode::Records(list(raw)),
            (None, None, Some(raw)) => Mode::AllExcept(list(raw)),
            (None, None, None) => Mode::Missing,
            _ => Mode::Conflict,
        }
    }
}

/// Runs `delete`.
pub(super) fn run(
    opts: &GlobalOpts,
    mode: &Mode,
    domain: &str,
    dry_run: bool,
) -> Result<(), Failure> {
    let cwd = std::env::current_dir()
        .map_err(|source| Failure::handled("delete", format!("resolving cwd: {source}")))?;
    let store = match locate(&cwd) {
        Ok(StoreLocation::Open(store)) => store,
        Ok(_) => {
            let mut failure = Failure::handled("delete", NO_STORE_MESSAGE);
            failure.envelope_to_stderr = true;
            return Err(failure);
        }
        Err(source) => {
            return Err(Failure::handled(
                "delete",
                crate::output::chain_message(&source),
            ));
        }
    };

    let domains = store.domains();
    if !domains.iter().any(|d| d == domain) {
        let mut failure = domain_not_found("delete", domain, &domains);
        failure.envelope_to_stderr = true;
        return Err(failure);
    }

    // Mode validation (reference texts, plain `Error:` form).
    let (targets, bulk): (Vec<String>, bool) = match mode {
        Mode::Missing => {
            return Err(handled(
                "delete",
                "Error: must provide a record ID, --records, or --all-except.",
            ));
        }
        Mode::Conflict => {
            return Err(handled(
                "delete",
                "Error: cannot combine a record ID with --records or --all-except. Use only one mode.",
            ));
        }
        Mode::Single(id) => (vec![id.clone()], false),
        Mode::Records(ids) if ids.is_empty() => {
            return Err(handled(
                "delete",
                "Error: --records requires at least one ID.",
            ));
        }
        Mode::Records(ids) => (ids.clone(), true),
        Mode::AllExcept(ids) if ids.is_empty() => {
            return Err(handled(
                "delete",
                "Error: --all-except requires at least one ID to keep.",
            ));
        }
        Mode::AllExcept(keep) => {
            let lines = read_domain_lines(&store.root, domain).map_err(|source| {
                Failure::handled("delete", format!("reading domain file: {source}"))
            })?;
            let selected: Vec<String> = parsed_lines(&lines)
                .into_iter()
                .filter(|(_, record, _)| {
                    let id = record
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    !keep.contains(&id)
                })
                .filter_map(|(_, record, _)| {
                    record.get("id").and_then(Value::as_str).map(str::to_string)
                })
                .collect();
            (selected, true)
        }
    };
    let _ = bulk;

    let lines = read_domain_lines(&store.root, domain)
        .map_err(|source| Failure::handled("delete", format!("reading domain file: {source}")))?;
    let parsed = parsed_lines(&lines);

    // Resolve targets to (line index, record), first match per id.
    let mut removals: Vec<(usize, Value)> = Vec::new();
    for id in &targets {
        let found = parsed
            .iter()
            .find(|(_, record, _)| record.get("id").and_then(Value::as_str) == Some(id.as_str()))
            .map(|(index, record, _)| (*index, record.clone()));
        if let Some(hit) = found {
            removals.push(hit);
        } else {
            // Atomic: nothing is deleted on a missing id.
            let mut failure = record_not_found("delete", id);
            failure.envelope_to_stderr = true;
            return Err(failure);
        }
    }

    let deleted: Vec<(String, String, String)> = removals
        .iter()
        .map(|(_, record)| {
            let id = record
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let kind = record
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("convention")
                .to_string();
            let summary = record_summary(record);
            (id, kind, summary)
        })
        .collect();

    let single_shape = !bulk && !dry_run && matches!(mode, Mode::Single(_));
    let keep_indices: Vec<usize> = removals.iter().map(|(index, _)| *index).collect();
    let survivors: Vec<String> = parsed
        .iter()
        .filter(|(index, _, _)| !keep_indices.contains(index))
        .map(|(_, _, raw)| raw.clone())
        .collect();
    let kept = survivors.len();

    if !dry_run {
        write_domain_lines(&store.root, domain, &survivors).map_err(|source| {
            Failure::handled("delete", format!("writing domain file: {source}"))
        })?;
    }

    if opts.json {
        if single_shape {
            let (id, kind, summary) = deleted.first().cloned().unwrap_or_default();
            let mut fields = Map::new();
            fields.insert("domain".into(), Value::String(domain.into()));
            fields.insert("id".into(), Value::String(id));
            fields.insert("type".into(), Value::String(kind));
            fields.insert("summary".into(), Value::String(summary));
            print_json(&success_envelope("delete", fields), false);
        } else {
            let list: Vec<Value> = deleted
                .iter()
                .map(|(id, kind, summary)| {
                    let mut item = Map::new();
                    item.insert("id".into(), Value::String(id.clone()));
                    item.insert("type".into(), Value::String(kind.clone()));
                    item.insert("summary".into(), Value::String(summary.clone()));
                    Value::Object(item)
                })
                .collect();
            let mut fields = Map::new();
            fields.insert("domain".into(), Value::String(domain.into()));
            fields.insert("dryRun".into(), Value::Bool(dry_run));
            fields.insert("deleted".into(), Value::Array(list));
            fields.insert("kept".into(), Value::from(kept as u64));
            print_json(&success_envelope("delete", fields), false);
        }
    } else {
        let mut text = String::new();
        for (id, kind, summary) in &deleted {
            let verb = if dry_run {
                "[DRY RUN] Would delete"
            } else if single_shape {
                "✓ Deleted"
            } else {
                "Deleted"
            };
            let _ = std::fmt::Write::write_fmt(
                &mut text,
                format_args!("{verb} {kind} {id} from {domain}: {summary}\n"),
            );
        }
        if !single_shape && !dry_run && deleted.len() > 1 {
            let _ = std::fmt::Write::write_fmt(
                &mut text,
                format_args!("✓ Deleted {} records from {domain}\n", deleted.len()),
            );
        }
        print_line(opts.quiet, text.trim_end_matches('\n'));
    }
    Ok(())
}

/// A plain handled failure with the `Error:` prefix already baked in.
fn handled(command: &str, message: &str) -> Failure {
    let mut failure = Failure::handled(command, message);
    failure.envelope_to_stderr = true;
    failure
}
