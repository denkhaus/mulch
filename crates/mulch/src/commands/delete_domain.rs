//! `mulch delete-domain` — remove a domain entry and its expertise
//! file (archive stays).

use mulch::Error;

use crate::cli::GlobalOpts;
use crate::commands::{
    NO_STORE_MESSAGE, StoreLocation, domain_file, locate, parsed_lines, read_confirmation,
    read_domain_lines,
};
use crate::output::{Failure, print_json, success_envelope};

/// Runs `delete-domain`. `--json` skips the prompt entirely;
/// `--yes` skips it in plain mode; a cancelled prompt is exit 0.
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
            let mut failure = Failure::handled("delete-domain", NO_STORE_MESSAGE);
            failure.envelope_to_stderr = true;
            return Err(failure);
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

    let lines = read_domain_lines(&store.root, domain).map_err(|source| {
        Failure::handled("delete-domain", format!("reading domain file: {source}"))
    })?;
    let record_count = parsed_lines(&lines).len();
    let plural = if record_count == 1 {
        "record"
    } else {
        "records"
    };

    if dry_run {
        let text = format!(
            "[DRY RUN] Would delete domain {domain} ({record_count} {plural}) and its expertise file."
        );
        #[allow(clippy::print_stdout, reason = "dry-run preview renders on stdout")]
        {
            println!("{text}");
        }
        return Ok(());
    }

    // --json deletes immediately (reference prompt-skip quirk).
    if !opts.json && !yes {
        #[allow(clippy::print_stdout, reason = "prompt renders on stdout")]
        {
            print!(
                "This will delete domain \"{domain}\" ({record_count} {plural}) and its expertise file. Continue? (y/N): "
            );
            let _ = std::io::Write::flush(&mut std::io::stdout());
        }
        let answer = read_confirmation();
        // EOF (closed stdin) cancels instead of blocking — the
        // reference hangs here (README DEVIATIONS).
        let confirmed = answer.is_ok_and(|line| {
            line.trim().eq_ignore_ascii_case("y") || line.trim().eq_ignore_ascii_case("yes")
        });
        if !confirmed {
            #[allow(clippy::print_stdout, reason = "prompt result renders on stdout")]
            {
                println!("Cancelled.");
            }
            return Ok(());
        }
    }

    // Effects: config entry removed (YAML serializer rewrite, comments
    // stripped), live expertise file deleted, archive untouched.
    store.remove_domain(domain).map_err(|source| {
        Failure::handled("delete-domain", crate::output::chain_message(&source))
    })?;
    let file = domain_file(&store.root, domain);
    if file.is_file() {
        std::fs::remove_file(&file).map_err(|source| {
            Failure::handled(
                "delete-domain",
                format!("removing {}: {source}", file.display()),
            )
        })?;
    }

    if opts.json {
        let mut fields = serde_json::Map::new();
        fields.insert("domain".into(), serde_json::Value::String(domain.into()));
        fields.insert("deletedFile".into(), serde_json::Value::Bool(true));
        fields.insert(
            "recordCount".into(),
            serde_json::Value::from(record_count as u64),
        );
        print_json(&success_envelope("delete-domain", fields), false);
    } else {
        #[allow(clippy::print_stdout, reason = "success renders on stdout")]
        {
            println!("✓ Removed domain {domain} and deleted expertise file.");
        }
    }
    Ok(())
}

/// The unknown-domain failure: plain mode carries the add-hint, json
/// mode the available-domains list (reference divergence).
fn not_in_config(opts: &GlobalOpts, domain: &str, available: &[String]) -> Failure {
    let message = if opts.json {
        format!(
            "Domain \"{domain}\" not found in config. Available domains: {}",
            available.join(", ")
        )
    } else {
        format!(
            "Error: domain \"{domain}\" not found in config.\nHint: Run `mulch add {domain}` to create it, or check `mulch status` for existing domains."
        )
    };
    let mut failure = Failure::handled("delete-domain", message);
    failure.envelope_to_stderr = true;
    failure
}

/// Config write errors are typed; keep the error surface explicit.
const _: fn() = || {
    let _ = Error::InvalidDomain {
        domain: String::new(),
    };
};
