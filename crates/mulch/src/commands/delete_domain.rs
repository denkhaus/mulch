//! `mulch delete-domain` — remove a domain entry and its expertise
//! file (the archive file stays).
//!
//! `--json` skips the confirmation prompt entirely (reference quirk:
//! the machine path deletes without `--yes`); the prompt answer is
//! compared raw (`y`/`yes`, no trimming — reference `delete-domain.ts`);
//! EOF cancels instead of blocking (README DEVIATIONS).

use crate::cli::GlobalOpts;
use crate::commands::read_confirmation;
use crate::output::{Failure, print_json, print_line, success_envelope};

/// Runs `delete-domain`.
pub(super) fn run(
    opts: &GlobalOpts,
    domain: &str,
    yes: bool,
    dry_run: bool,
) -> Result<(), Failure> {
    let mut store = crate::commands::open_store("delete-domain", true)?;

    let domains = store.domains();
    if !domains.iter().any(|d| d == domain) {
        return Err(not_in_config(opts, domain, &domains));
    }

    // Strict read: malformed lines abort before anything is deleted.
    let file = store.domain_path(domain);
    let records = mulch::read_strict(&file, opts.allow_unknown_types).map_err(|source| {
        Failure::handled_on_stderr("delete-domain", crate::commands::render_core_error(&source))
    })?;
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

/// A `serde_json::Map` from a `json!` macro result.
fn object(value: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    match value {
        serde_json::Value::Object(map) => map,
        _ => serde_json::Map::new(),
    }
}
