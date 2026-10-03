//! `mulch delete-domain` — remove a domain entry and its expertise
//! file (the archive file stays).
//!
//! `--json` skips the confirmation prompt entirely (reference quirk:
//! the machine path deletes without `--yes`); the prompt answer is
//! compared raw (`y`/`yes`, no trimming — reference `delete-domain.ts`);
//! EOF cancels instead of blocking (README DEVIATIONS).

use crate::cli::GlobalOpts;
use crate::commands::{NO_STORE_CONFIG_MESSAGE, StoreLocation, locate, read_confirmation};
use crate::output::{Failure, print_json, print_line, success_envelope};

/// Runs `delete-domain`.
pub(super) fn run(
    opts: &GlobalOpts,
    domain: &str,
    yes: bool,
    dry_run: bool,
) -> Result<(), Failure> {
    let cwd = std::env::current_dir()
        .map_err(|source| Failure::handled("delete-domain", format!("resolving cwd: {source}")))?;
    let mut store = match locate(&cwd) {
        Ok(StoreLocation::Open(store)) => store,
        Ok(_) => {
            return Err(Failure::handled_on_stderr(
                "delete-domain",
                NO_STORE_CONFIG_MESSAGE,
            ));
        }
        Err(source) => {
            return Err(Failure::handled(
                "delete-domain",
                crate::output::chain_message(&source),
            ));
        }
    };

    let domains = store.domains();
    if !domains.iter().any(|d| d == domain) {
        return Err(not_in_config(opts, domain, &domains));
    }

    // Strict read: malformed lines abort before anything is deleted.
    let file = crate::commands::domain_file(&store.root, domain);
    let records = mulch::read_strict(&file, opts.allow_unknown_types)
        .map_err(|source| Failure::handled_on_stderr("delete-domain", render_error(&source)))?;
    let record_count = records.len();
    let plural = if record_count == 1 {
        "record"
    } else {
        "records"
    };

    if dry_run {
        if opts.json {
            let fields = serde_json::json!({
                "domain": domain,
                "dryRun": true,
                "recordCount": record_count,
            });
            print_json(&success_envelope("delete-domain", object(fields)), false);
        } else {
            print_line(
                opts.quiet,
                &format!(
                    "[DRY RUN] Would delete domain {domain} ({record_count} {plural}) and its expertise file."
                ),
            );
        }
        return Ok(());
    }

    // --json deletes immediately (reference prompt-skip quirk).
    if !opts.json && !yes {
        print_prompt(&format!(
            "This will delete domain \"{domain}\" ({record_count} {plural}) and its expertise file. Continue?"
        ));
        let confirmed = read_confirmation().is_ok_and(|answer| {
            let answer = answer.to_lowercase();
            answer == "y" || answer == "yes"
        });
        if !confirmed {
            print_line(opts.quiet, "Cancelled.");
            return Ok(());
        }
    }

    // One domain-level operation: config rewrite + live-file removal.
    store.delete_domain(domain).map_err(|source| {
        Failure::handled("delete-domain", crate::output::chain_message(&source))
    })?;

    if opts.json {
        let fields = serde_json::json!({
            "domain": domain,
            "deletedFile": true,
            "recordCount": record_count,
        });
        print_json(&success_envelope("delete-domain", object(fields)), false);
    } else {
        print_line(
            opts.quiet,
            &format!("✓ Removed domain {domain} and deleted expertise file."),
        );
    }
    Ok(())
}

/// Renders the prompt on stdout without a trailing newline.
#[allow(clippy::print_stdout, reason = "prompt renders on stdout")]
fn print_prompt(text: &str) {
    use std::io::Write as _;
    print!("{text} ");
    let _ = std::io::stdout().flush();
}

/// The unknown-domain failure: plain mode carries the add-hint, json
/// mode the available-domains list (reference divergence).
fn not_in_config(opts: &GlobalOpts, domain: &str, available: &[String]) -> Failure {
    let list = if available.is_empty() {
        "(none)".to_string()
    } else {
        available.join(", ")
    };
    let message = if opts.json {
        format!("Domain \"{domain}\" not found in config. Available domains: {list}")
    } else {
        format!(
            "Error: domain \"{domain}\" not found in config.\nHint: Run `mulch add {domain}` to create it, or check `mulch status` for existing domains."
        )
    };
    Failure::handled_on_stderr("delete-domain", message)
}

/// Renders a format-core error the way the reference does.
fn render_error(error: &mulch::Error) -> String {
    match error {
        mulch::Error::MalformedLine {
            path,
            line,
            preview,
            reason,
        } => format!(
            "Error: Malformed JSONL at {}:{line}: {reason}. Line: {preview}",
            path.display()
        ),
        mulch::Error::UnknownRecordType {
            path,
            line,
            id,
            record_type,
        } => {
            let id_part = id
                .as_ref()
                .map_or_else(String::new, |id| format!(" (id={id})"));
            format!(
                "Error: Unknown record type \"{record_type}\" at {}:{line}{id_part}. Register it under custom_types in mulch.config.yaml, remove the record, or pass --allow-unknown-types to bypass.",
                path.display()
            )
        }
        other => format!("Error: {other}"),
    }
}

/// A `serde_json::Map` from a `json!` macro result.
fn object(value: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    match value {
        serde_json::Value::Object(map) => map,
        _ => serde_json::Map::new(),
    }
}
