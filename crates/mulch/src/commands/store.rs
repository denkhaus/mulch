//! The store seam of the parity commands (mulch-3326): locating
//! and opening the store (the no-store contracts, the writer
//! lock), the strict domain reads with identifier resolution,
//! the updated-record write gate, and the write-path law. One
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
/// unknown_domain_failure(...)` couple that lived in eight commands.
pub(crate) fn ensure_domain(
    command: &'static str,
    store: &mulch::StoreFiles,
    domain: &str,
    json: bool,
    shape: DomainFailure,
) -> Result<(), Failure> {
    let domains = store.domains();
    if domains.iter().any(|d| d == domain) {
        return Ok(());
    }
    Err(unknown_domain_failure(
        command, domain, &domains, json, shape,
    ))
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
pub(crate) fn locate_record(
    opts: &crate::cli::GlobalOpts,
    command: &'static str,
    domain: &str,
    id: &str,
    shape: DomainFailure,
    config_message: bool,
    exclusive: bool,
) -> Result<(mulch::StoreFiles, Vec<mulch::LineRecord>, usize), Failure> {
    // Mutating callers (edit, outcome) hold the writer lock across
    // the whole cycle; the read family (query, mulch-16da) passes
    // false (mulch-228d).
    let store = if exclusive {
        open_store_mut(command, config_message)?
    } else {
        open_store(command, config_message)?
    };
    ensure_domain(command, &store, domain, opts.json, shape)?;
    let (records, position) =
        read_and_resolve(command, &store, domain, id, opts.allow_unknown_types)?;
    Ok((store, records, position))
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
