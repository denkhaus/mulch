//! `mulch delete` — remove records (single id, `--records`,
//! `--all-except`).
//!
//! Rewrite model (reference): the surviving records are re-serialized
//! compactly by the format core (`write_records`), which also assigns
//! ids to id-less survivors. Malformed lines and unregistered types are
//! hard errors — nothing is written (reference `readExpertiseFile`).

use mulch::{record_summary, resolve_record_id};

use crate::cli::GlobalOpts;
use crate::output::{Failure, print_json, print_line, success_envelope};

/// Which records the invocation targets (reference mode exclusivity).
pub(super) enum Mode {
    /// One positional id (or bare hash / unique prefix).
    Single(String),
    /// `--records` list.
    Records(Vec<String>),
    /// `--all-except` list (ids to keep; each must resolve).
    AllExcept(Vec<String>),
}

impl Mode {
    /// Parses the mutually exclusive id sources, rejecting the invalid
    /// shapes at construction (plain/json texts differ in casing —
    /// reference `delete.ts`).
    pub(super) fn from_args(
        json: bool,
        id: Option<&str>,
        records: Option<&str>,
        all_except: Option<&str>,
    ) -> Result<Self, Failure> {
        let list = |raw: &str| -> Vec<String> {
            raw.split(',')
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect()
        };
        // Plain carries the `Error: ` prefix and lowercase wording;
        // json capitalizes and drops the prefix (reference delete.ts).
        let render = |text: &str| -> String {
            if json {
                let mut chars = text.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            } else {
                format!("Error: {text}")
            }
        };
        match (id, records, all_except) {
            (Some(id), None, None) => Ok(Mode::Single(id.to_string())),
            (None, Some(raw), None) => {
                let ids = list(raw);
                if ids.is_empty() {
                    return Err(Failure::handled_on_stderr(
                        "delete",
                        render("--records requires at least one ID."),
                    ));
                }
                Ok(Mode::Records(ids))
            }
            (None, None, Some(raw)) => {
                let keep = list(raw);
                if keep.is_empty() {
                    return Err(Failure::handled_on_stderr(
                        "delete",
                        render("--all-except requires at least one ID to keep."),
                    ));
                }
                Ok(Mode::AllExcept(keep))
            }
            (None, None, None) => Err(Failure::handled_on_stderr(
                "delete",
                render("must provide a record ID, --records, or --all-except."),
            )),
            _ => Err(Failure::handled_on_stderr(
                "delete",
                render(
                    "cannot combine a record ID with --records or --all-except. Use only one mode.",
                ),
            )),
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
    let store = crate::commands::open_store("delete", true)?;

    let domains = store.domains();
    if !domains.iter().any(|d| d == domain) {
        return Err(domain_failure(opts, "delete", domain, &domains));
    }

    let lines = store
        .read_records(domain, opts.allow_unknown_types)
        .map_err(|source| {
            Failure::handled_on_stderr("delete", crate::commands::render_core_error(&source))
        })?;

    // Resolve every target (missing/ambiguous ids abort before any write).
    let mut removal_indices: Vec<usize> = Vec::new();
    match mode {
        Mode::Single(id) => removal_indices.push(resolve(&lines, id)?),
        Mode::Records(ids) => {
            for id in ids {
                removal_indices.push(resolve(&lines, id)?);
            }
        }
        Mode::AllExcept(keep) => {
            let mut keep_indices: Vec<usize> = Vec::new();
            for id in keep {
                keep_indices.push(resolve(&lines, id)?);
            }
            for index in 0..lines.len() {
                if !keep_indices.contains(&index) {
                    removal_indices.push(index);
                }
            }
        }
    }
    // A Set of indices: duplicate ids never double-delete (reference).
    removal_indices.sort_unstable();
    removal_indices.dedup();

    let bulk = !matches!(mode, Mode::Single(_)) || dry_run;
    let deleted: Vec<(String, String, String)> = removal_indices
        .iter()
        .map(|index| {
            let line = &lines[*index];
            (
                line.id().unwrap_or_default().to_string(),
                line.record_type(),
                record_summary(&line.record),
            )
        })
        .collect();
    let kept = lines.len() - removal_indices.len();

    if !dry_run {
        let survivors: Vec<serde_json::Value> = lines
            .iter()
            .enumerate()
            .filter(|(index, _)| !removal_indices.contains(index))
            .map(|(_, line)| line.record.clone())
            .collect();
        store
            .rewrite_domain(domain, &survivors)
            .map_err(|source| Failure::handled("delete", crate::output::chain_message(&source)))?;
    }

    // The reference prints each deleted record's summary through the
    // type registry; an unregistered type throws AFTER the rewrite
    // (only reachable under --allow-unknown-types).
    if opts.allow_unknown_types {
        for (_, kind, _) in &deleted {
            if !mulch::PAYLOAD_TYPES.contains(&kind.as_str()) {
                return Err(Failure::handled_on_stderr(
                    "delete",
                    format!("Error: Unknown record type: {kind}"),
                ));
            }
        }
    }

    if opts.json {
        if bulk {
            let list: Vec<serde_json::Value> = deleted
                .iter()
                .map(|(id, kind, summary)| {
                    serde_json::json!({ "id": id, "type": kind, "summary": summary })
                })
                .collect();
            let fields = serde_json::json!({
                "domain": domain,
                "dryRun": dry_run,
                "deleted": list,
                "kept": kept,
            });
            print_json(&success_envelope("delete", object(fields)), false);
        } else {
            let (id, kind, summary) = deleted.first().cloned().unwrap_or_default();
            let fields = serde_json::json!({
                "domain": domain,
                "id": id,
                "type": kind,
                "summary": summary,
            });
            print_json(&success_envelope("delete", object(fields)), false);
        }
    } else {
        let mut text = String::new();
        for (id, kind, summary) in &deleted {
            let verb = if dry_run {
                "[DRY RUN] Would delete"
            } else if bulk {
                "Deleted"
            } else {
                "✓ Deleted"
            };
            let _ = std::fmt::Write::write_fmt(
                &mut text,
                format_args!("{verb} {kind} {id} from {domain}: {summary}\n"),
            );
        }
        if !bulk && !dry_run {
            // single-id form already carries the ✓ on its line
        }
        if bulk && !dry_run && deleted.len() > 1 {
            let _ = std::fmt::Write::write_fmt(
                &mut text,
                format_args!("✓ Deleted {} records from {domain}\n", deleted.len()),
            );
        }
        // Zero deletions print nothing at all (reference).
        let text = text.trim_end_matches('\n');
        if !text.is_empty() {
            print_line(opts.quiet, text);
        }
    }
    Ok(())
}

/// Resolves one identifier against the domain's records.
fn resolve(lines: &[mulch::LineRecord], id: &str) -> Result<usize, Failure> {
    resolve_record_id(lines, id)
        .map_err(|error| crate::commands::resolve_failure("delete", id, error))
}

/// The unknown-domain failure (plain and json texts differ).
fn domain_failure(opts: &GlobalOpts, command: &str, domain: &str, available: &[String]) -> Failure {
    let list = if available.is_empty() {
        "(none)".to_string()
    } else {
        available.join(", ")
    };
    if opts.json {
        Failure::handled_on_stderr(
            command,
            format!("Domain \"{domain}\" not found in config. Available domains: {list}"),
        )
    } else {
        Failure::handled_on_stderr(
            command,
            format!("Error: domain \"{domain}\" not found in config.\nAvailable domains: {list}"),
        )
    }
}

/// A `serde_json::Value::Object` from a `json!` macro result.
fn object(value: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    match value {
        serde_json::Value::Object(map) => map,
        _ => serde_json::Map::new(),
    }
}
