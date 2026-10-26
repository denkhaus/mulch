//! The store seam of the parity commands (mulch-3326): locating
//! and opening the store (the no-store contracts, the writer
//! lock), the strict domain reads with identifier resolution, the
//! one domain-addressing table ([`resolve_domain_set`] /
//! [`ensure_domain`] / [`ensure_canonical`], mulch-8452), the
//! updated-record write gate, and the write-path law. One
//! owner for every `StoreFiles` lifecycle the commands share —
//! the failure and format shapes live in [`super::failure`], the
//! flag and prompt adapters in [`super::input`].

use std::path::Path;

use mulch::StoreLocation;

use super::failure::{DomainFailure, render_core_error, resolve_failure, unknown_domain_failure};
use crate::output::Failure;

/// The handled-error message for a missing store (status et al.).
pub(crate) const NO_STORE_MESSAGE: &str = "No .mulch/ directory found. Run `mulch init` first.";

/// The no-store message of the read-modify-write commands: the
/// reference's config reader throws it (with the `Error: ` prefix the
/// command adds).
pub(crate) const NO_STORE_CONFIG_MESSAGE: &str =
    "Error: No .mulch/ directory found. Run `mulch init` to set up this project.";

/// Locates the store for the parity commands.
pub(crate) fn locate(root: &Path) -> Result<mulch::StoreLocation, mulch::Error> {
    mulch::StoreFiles::locate(root)
}

/// Opens the store or fails with the command's no-store contract.
///
/// One owner for the "no store" policy: the message variants and the
/// JSON-envelope channel live here, not in ten command prologues.
/// `config_message` selects the read-modify-write wording (the
/// reference's config reader) over the reporting wording.
pub(crate) fn open_store(
    command: &'static str,
    config_message: bool,
) -> Result<mulch::StoreFiles, Failure> {
    open_located(command, config_message, false)
}

/// Opens the store under the writer lock (mulch-228d): the advisory
/// exclusive lock on `.mulch/.lock` is held until the returned store
/// drops, so the command's whole load→mutate→save cycle is atomic
/// against other mulch processes. Every mutating command opens this
/// way; a contended lock fails fast through the shared error path
/// naming the lock file.
pub(crate) fn open_store_mut(
    command: &'static str,
    config_message: bool,
) -> Result<mulch::StoreFiles, Failure> {
    open_located(command, config_message, true)
}

/// The shared open body behind [`open_store`] and
/// [`open_store_mut`]: locate (optionally exclusive) and map the
/// no-store/error contracts once.
fn open_located(
    command: &'static str,
    config_message: bool,
    exclusive: bool,
) -> Result<mulch::StoreFiles, Failure> {
    let cwd = std::env::current_dir()
        .map_err(|source| Failure::handled(command, format!("resolving cwd: {source}")))?;
    let located = if exclusive {
        mulch::StoreFiles::locate_exclusive(&cwd)
    } else {
        locate(&cwd)
    };
    match located {
        Ok(StoreLocation::Open(store)) => Ok(store),
        Ok(StoreLocation::Missing | StoreLocation::NoConfig) => {
            let message = if config_message {
                NO_STORE_CONFIG_MESSAGE
            } else {
                NO_STORE_MESSAGE
            };
            Err(Failure::handled_on_stderr(command, message))
        }
        Err(source) => Err(locate_error(command, &source)),
    }
}

/// The locate-ERROR contract every command shares (mulch-d343's C8
/// fold): a broken custom_types block fails globally with its own
/// shape; every other locate error renders through the chain
/// renderer. No-store handling deliberately stays with each caller —
/// mutating commands error through [`open_store`], doctor renders its
/// no-store REPORT, learn and the dynamic-flag probe continue on
/// builtins, and init creates the store — so only this two-arm map is
/// shared.
pub(crate) fn locate_error(command: &'static str, source: &mulch::Error) -> Failure {
    match source {
        mulch::Error::CustomTypes { message } => Failure::config_error(message),
        other => Failure::handled(command, chain_message_from(other)),
    }
}

/// The crate-local alias for the output module's chain renderer.
fn chain_message_from(error: &mulch::Error) -> String {
    crate::output::chain_message(error)
}

/// The domain-exists prologue half (D3, mulch-0e87): the check every
/// domain-taking command runs, mapped onto ITS wording shape. The
/// open stays with [`open_store`]/[`open_store_mut`] (callers differ
/// on lock and timing); order-pinned commands interleave their own
/// checks around this. Folds the `!domains.iter().any(...) +
/// unknown_domain_failure(...)` couple that lived in six commands
/// across five files (move, archive, delete, prime, and edit/outcome
/// through locate_record).
/// The addressing table's single-domain arm: resolve the address onto
/// its canonical domain (alias chains included, ADR-0006), require
/// membership, and render the command's failure shape. Every
/// single-domain entry (archive, delete, move's two ends, record's
/// write path via [`super::store::open_store_mut`] callers) speaks
/// through this, and the set arm wraps it (mulch-8452).
///
/// # Errors
///
/// [`unknown_domain_failure`]'s [`Failure`] when the address is not a
/// configured domain.
pub(crate) fn ensure_domain<'a>(
    command: &'static str,
    store: &'a mulch::StoreFiles,
    domain: &'a str,
    json: bool,
    shape: DomainFailure,
) -> Result<&'a str, Failure> {
    // Aliases resolve FIRST (ADR-0006): the caller receives — and
    // addresses files by — the canonical domain. The return borrows
    // from BOTH inputs: the canonical may live in the store's alias
    // map or be the argument itself (a non-alias) — hence the
    // twice-bound lifetime.
    let canonical = store.resolve_domain(domain);
    ensure_canonical(command, store, canonical, json, shape)
}

/// The addressing table's check-only arm: prime resolves its names at
/// ENTRY (for dedupe and the exclusion walk), so its validation must
/// not walk the alias map a second time — membership is the whole
/// check from there (mulch-8452: resolve once, validate once).
///
/// # Errors
///
/// [`unknown_domain_failure`]'s [`Failure`] when `canonical` is not a
/// configured domain.
pub(crate) fn ensure_canonical<'a>(
    command: &'static str,
    store: &mulch::StoreFiles,
    canonical: &'a str,
    json: bool,
    shape: DomainFailure,
) -> Result<&'a str, Failure> {
    let domains = store.domains();
    if domains.iter().any(|domain| domain == canonical) {
        return Ok(canonical);
    }
    Err(unknown_domain_failure(
        command, canonical, &domains, json, shape,
    ))
}

/// The addressing table's set arm: `None` is every configured domain
/// in config order, `Some(address)` the resolved canonical singleton.
/// The four read-family commands (query, search, rank, ready) and
/// compact's three modes each spelled this law by hand; this is the
/// one body now (mulch-8452).
///
/// # Errors
///
/// [`unknown_domain_failure`]'s [`Failure`] when the address is not a
/// configured domain.
pub(crate) fn resolve_domain_set(
    command: &'static str,
    store: &mulch::StoreFiles,
    wanted: Option<&str>,
    json: bool,
    shape: DomainFailure,
) -> Result<Vec<String>, Failure> {
    match wanted {
        Some(wanted) => Ok(vec![
            ensure_domain(command, store, wanted, json, shape)?.to_string(),
        ]),
        None => Ok(store.domains()),
    }
}

/// A domain file's mtime as the section-header's `updated …` suffix
/// (`None` when absent) — the shared read behind search/query/prime
/// (mulch-1c32: three byte-identical private copies). `status` keeps
/// its own inline map: its failure renders through the chain
/// renderer, a different channel law.
pub(crate) fn domain_modified_ts(
    command: &'static str,
    store: &mulch::StoreFiles,
    domain: &str,
) -> Result<Option<jiff::Timestamp>, Failure> {
    match store.domain_modified(domain) {
        Ok(Some(time)) => jiff::Timestamp::try_from(time)
            .map(Some)
            .map_err(|source| Failure::error_line(command, source)),
        Ok(None) => Ok(None),
        Err(source) => Err(Failure::error_line(command, source)),
    }
}

/// The strict domain read with its failure mapped onto the command's
/// handled shape (mulch-a8f4): the pair ten call sites repeated beside
/// [`read_and_resolve`]. `render_core_error` owns the byte law
/// (malformed-line + unknown-type texts).
pub(crate) fn read_domain_records(
    command: &str,
    store: &mulch::StoreFiles,
    domain: &str,
    allow_unknown: bool,
) -> Result<Vec<mulch::LineRecord>, Failure> {
    store
        .read_records(domain, allow_unknown)
        .map_err(|source| Failure::handled_on_stderr(command, render_core_error(&source)))
}

/// The read half of the record-locator seam: strict-reads the domain
/// and resolves the identifier (exact id, bare hash, or unique prefix).
/// `move` and `delete` reach this half directly — their prologues pin
/// a different order around it (`move` checks source==target and both
/// domains first, `delete` checks the domain before its mode
/// validation, mulch-e645).
pub(crate) fn read_and_resolve(
    command: &str,
    store: &mulch::StoreFiles,
    domain: &str,
    id: &str,
    allow_unknown: bool,
) -> Result<(Vec<mulch::LineRecord>, usize), Failure> {
    let records = read_domain_records(command, store, domain, allow_unknown)?;
    let position =
        mulch::resolve_record_id(&records, id).map_err(|error| resolve_failure(command, error))?;
    Ok((records, position))
}

/// The full-record verdict of an UPDATED record for the write gates
/// (`edit`, `move`): [`mulch::schema::full_verdict`] over the
/// reference's in-memory shape. When the target's raw line carries a
/// The single-id prologue every per-record command opens with: open
/// the store, check the domain, strict-read the file, resolve the id
/// (mulch-0fd8 — one seam beside [`open_store`],
/// [`unknown_domain_failure`] and [`resolve_failure`]). `shape` and
/// `config_message` select the unknown-domain and no-store wordings;
/// the future per-record commands (query family) start here.
///
/// The CANONICAL domain comes back with the read (mulch-8452): the
/// addressing table already resolved it, so a caller that writes must
/// address the canonical rather than re-resolve the raw argument — an
/// `edit`/`outcome` through an alias that wrote the RAW name would
/// create a new alias-named file, the fragmentation ADR-0006 forbids
/// (caught by the aliases entry batteries).
pub(crate) fn locate_record(
    opts: &crate::cli::GlobalOpts,
    command: &'static str,
    domain: &str,
    id: &str,
    shape: DomainFailure,
    config_message: bool,
    exclusive: bool,
) -> Result<(mulch::StoreFiles, String, Vec<mulch::LineRecord>, usize), Failure> {
    // Mutating callers (edit, outcome) hold the writer lock across
    // the whole cycle; the read family (query, mulch-16da) passes
    // false (mulch-228d).
    let store = if exclusive {
        open_store_mut(command, config_message)?
    } else {
        open_store(command, config_message)?
    };
    let domain = ensure_domain(command, &store, domain, opts.json, shape)?.to_string();
    let (records, position) =
        read_and_resolve(command, &store, &domain, id, opts.allow_unknown_types)?;
    Ok((store, domain, records, position))
}

/// The write-path law of the mutating commands (mulch-b6d5): rewrite
/// the domain file with the new record set, mapping a store failure
/// onto the command's handled shape (chain-rendered). The projection
/// to the record set stays at the call site — it differs per command
/// (plain projection, index replace, survivor filter, batch working
/// copy).
pub(crate) fn persist(
    command: &'static str,
    store: &mulch::StoreFiles,
    domain: &str,
    records: &[serde_json::Value],
) -> Result<(), Failure> {
    store
        .rewrite_domain(domain, records)
        .map_err(|source| Failure::handled(command, chain_message_from(&source)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::testscratch::store_with;

    /// The shared alias fixture: `dev`/`rust` domains with a chained
    /// alias (develop -> dev, devloop -> develop) and a direct one
    /// (rs -> rust).
    fn alias_store(tag: &str) -> (std::path::PathBuf, mulch::StoreFiles) {
        store_with(
            tag,
            "version: '1'\ndomains:\n  dev: {}\n  rust: {}\n\
             domain_aliases:\n  develop: dev\n  devloop: develop\n  rs: rust\n",
            true,
        )
    }

    /// The table's all-arm: `None` is every configured domain in
    /// config order — never alphabetical, never resolved.
    #[test]
    fn resolve_domain_set_none_is_all_in_config_order() {
        let (root, store) = alias_store("set-all");
        let all = resolve_domain_set("query", &store, None, false, DomainFailure::QueryHint)
            .expect("all domains");
        assert_eq!(all, vec!["dev".to_string(), "rust".to_string()]);
        std::fs::remove_dir_all(&root).ok();
    }

    /// The table's one-arm resolves alias CHAINS to the canonical
    /// singleton and leaves real domains alone (ADR-0006).
    #[test]
    fn resolve_domain_set_resolves_aliases_to_the_canonical_singleton() {
        let (root, store) = alias_store("set-alias");
        let chained = resolve_domain_set(
            "query",
            &store,
            Some("devloop"),
            false,
            DomainFailure::QueryHint,
        )
        .expect("alias");
        assert_eq!(chained, vec!["dev".to_string()]);
        let direct =
            resolve_domain_set("query", &store, Some("rs"), false, DomainFailure::QueryHint)
                .expect("direct alias");
        assert_eq!(direct, vec!["rust".to_string()]);
        let domain = resolve_domain_set(
            "query",
            &store,
            Some("rust"),
            false,
            DomainFailure::QueryHint,
        )
        .expect("domain");
        assert_eq!(domain, vec!["rust".to_string()]);
        std::fs::remove_dir_all(&root).ok();
    }

    /// A miss names the address the caller typed and lists the
    /// configured domains (the read-family shape).
    #[test]
    fn resolve_domain_set_miss_names_the_address_and_lists_available() {
        let (root, store) = alias_store("set-miss");
        let Err(failure) = resolve_domain_set(
            "query",
            &store,
            Some("nope"),
            false,
            DomainFailure::QueryHint,
        ) else {
            panic!("expected a miss");
        };
        assert!(
            failure
                .message
                .contains("Domain \"nope\" not found in config."),
            "{}",
            failure.message
        );
        assert!(
            failure.message.contains("Available domains: dev, rust"),
            "{}",
            failure.message
        );
        std::fs::remove_dir_all(&root).ok();
    }

    /// The single arm and the set arm are ONE law: same address, same
    /// shape, byte-identical failure text (the fold's whole point).
    #[test]
    fn single_and_set_arms_fail_identically() {
        let (root, store) = alias_store("set-identical");
        let Err(single) = ensure_domain("rank", &store, "nope", false, DomainFailure::RankHint)
        else {
            panic!("expected a miss");
        };
        let Err(set) =
            resolve_domain_set("rank", &store, Some("nope"), false, DomainFailure::RankHint)
        else {
            panic!("expected a miss");
        };
        assert_eq!(single.message, set.message);
        assert_eq!(single.envelope, set.envelope);
        std::fs::remove_dir_all(&root).ok();
    }

    /// The check-only arm (prime's pre-resolved names) validates
    /// membership without walking the alias map: a canonical passes,
    /// an alias name does NOT resolve here (the caller already did).
    #[test]
    fn ensure_canonical_checks_membership_without_resolving() {
        let (root, store) = alias_store("set-canonical");
        assert_eq!(
            ensure_canonical("prime", &store, "dev", false, DomainFailure::Prime {
                excluded: false,
            })
            .expect("canonical"),
            "dev"
        );
        assert!(
            ensure_canonical("prime", &store, "develop", false, DomainFailure::Prime {
                excluded: false,
            })
            .is_err(),
            "an alias name is not a configured domain"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    /// compact's one-line arm (mulch-8452): capital `D`, NO available
    /// list, no hint; the json envelope carries the bare sentence
    /// (the handled law strips the `Error: ` prefix).
    #[test]
    fn compact_arm_carries_its_own_one_line_shape() {
        let (root, store) = store_with("compact-arm", "version: '1'\ndomains:\n  dev: {}\n", true);
        let Err(plain) = resolve_domain_set(
            "compact",
            &store,
            Some("nope"),
            false,
            DomainFailure::Compact,
        ) else {
            panic!("expected a miss");
        };
        assert_eq!(plain.message, "Error: Domain \"nope\" not found in config.");
        assert_eq!(
            plain.envelope["error"],
            serde_json::json!("Domain \"nope\" not found in config.")
        );
        let Err(json) = resolve_domain_set(
            "compact",
            &store,
            Some("nope"),
            true,
            DomainFailure::Compact,
        ) else {
            panic!("expected a miss");
        };
        assert_eq!(json.message, "Domain \"nope\" not found in config.");
        assert_eq!(
            json.envelope["error"],
            serde_json::json!("Domain \"nope\" not found in config.")
        );
        std::fs::remove_dir_all(&root).ok();
    }
}
