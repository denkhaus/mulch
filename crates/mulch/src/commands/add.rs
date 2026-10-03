//! `mulch add <domain>` — register a domain.

use crate::cli::GlobalOpts;
use crate::commands::{StoreLocation, locate};
use crate::output::{Failure, print_json, print_line, success_envelope};

/// Valid domain names (reference `src/utils/config.ts:204`).
fn valid_domain(domain: &str) -> bool {
    let mut chars = domain.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Runs `add`: fresh domains get an empty expertise file and a
/// comment-free config rewrite (reference behavior).
pub(super) fn run(opts: &GlobalOpts, domain: String) -> Result<(), Failure> {
    let cwd = std::env::current_dir()
        .map_err(|source| Failure::handled("add", format!("resolving cwd: {source}")))?;
    let store = match locate(&cwd) {
        Ok(StoreLocation::Open(store)) => store,
        Ok(_) => return Err(no_store("add", opts)),
        Err(source) => {
            return Err(Failure::handled(
                "add",
                crate::output::chain_message(&source),
            ));
        }
    };

    if !valid_domain(&domain) {
        // The reference crashes with a Bun stack trace here; we render
        // its message cleanly (README DEVIATIONS).
        return Err(Failure::handled(
            "add",
            format!(
                "Invalid domain name: \"{domain}\". Only alphanumeric characters, hyphens, \
                 and underscores are allowed."
            ),
        ));
    }
    if store.domains().iter().any(|d| d == &domain) {
        return Err(Failure::handled(
            "add",
            format!("Domain \"{domain}\" already exists."),
        ));
    }

    let mut store = store;
    store
        .add_domain(&domain)
        .map_err(|source| Failure::handled("add", crate::output::chain_message(&source)))?;

    if opts.json {
        let mut fields = serde_json::Map::new();
        fields.insert("domain".into(), serde_json::Value::String(domain));
        print_json(&success_envelope("add", fields), false);
    } else {
        print_line(opts.quiet, &format!("Added domain \"{domain}\"."));
    }
    Ok(())
}

/// The handled no-store failure (envelope on stderr, exit 1).
fn no_store(command: &str, opts: &GlobalOpts) -> Failure {
    let _ = opts;
    let mut failure = Failure::handled(command, crate::commands::NO_STORE_MESSAGE);
    failure.envelope_to_stderr = true;
    failure
}
