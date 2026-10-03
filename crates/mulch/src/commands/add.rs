//! `mulch add <domain>` — register a domain.

use crate::cli::GlobalOpts;
use crate::commands::{StoreLocation, locate};
use crate::output::{Failure, print_json, print_line, success_envelope};

/// Runs `add`: fresh domains get an empty expertise file and a
/// comment-free config rewrite (reference behavior).
pub(super) fn run(opts: &GlobalOpts, domain: String) -> Result<(), Failure> {
    let cwd = std::env::current_dir()
        .map_err(|source| Failure::handled("add", format!("resolving cwd: {source}")))?;
    let store = match locate(&cwd) {
        Ok(StoreLocation::Open(store)) => store,
        Ok(_) => {
            let mut failure = Failure::handled("add", crate::commands::NO_STORE_MESSAGE);
            failure.envelope_to_stderr = true;
            return Err(failure);
        }
        Err(source) => {
            return Err(Failure::handled(
                "add",
                crate::output::chain_message(&source),
            ));
        }
    };

    if store.domains().iter().any(|d| d == &domain) {
        return Err(Failure::handled(
            "add",
            format!("Domain \"{domain}\" already exists."),
        ));
    }

    let mut store = store;
    store
        .register_domain(&domain)
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
