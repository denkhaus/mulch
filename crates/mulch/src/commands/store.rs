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

/// The same sentence without the prefix — the raw form the
/// reference's `readConfig` throws, for the callers that wrap it in
/// their own wording (sync's `Validation error:` prefix). ONE owner
/// for both spellings (mulch-375f rider).
pub(crate) const NO_STORE_PLAIN_MESSAGE: &str =
    "No .mulch/ directory found. Run `mulch init` to set up this project.";

/// The cwd resolution every command shares (mulch-375f): ONE channel
/// decision for the environmental failure — the `Error:`-prefixed
/// plain line on stderr, the prefix-stripped envelope beside it
/// (the [`Failure::error_line`] idiom). The wordings were
/// port-invented and spelled seven-plus ways with an accidental
/// stdout/stderr drift; nothing parity-pins them.
pub(crate) fn cwd(command: &str) -> Result<std::path::PathBuf, Failure> {
    std::env::current_dir().map_err(|source| cwd_failure(command, &source))
}

/// The failure law of [`cwd`], pure so the table can pin its bytes
/// without deleting the process working directory.
fn cwd_failure(command: &str, source: &std::io::Error) -> Failure {
    Failure::error_line(command, format!("resolving cwd: {source}"))
}

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
    let cwd = cwd(command)?;
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

/// The addressing table's single-domain arm (mulch-8452; the D3
/// prologue half of mulch-0e87): resolve the address onto its
/// canonical domain (alias chains included, ADR-0006), require
/// membership, and render the command's failure shape. The open stays
/// with [`open_store`]/[`open_store_mut`] (callers differ on lock and
/// timing); order-pinned commands interleave their own checks around
/// this. Every single-domain entry (archive, delete, move's two ends,
/// record's write path, edit/outcome through [`locate_record`]) speaks
/// through this, and the set arm wraps it.
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

/// The READ VIEW's mtime (mulch-7112): the newest file among the
/// canonical and its alias-named contributors — the timestamp half of
/// [`read_domain_view`], for section headers that label the view
/// (mulch-1c32's shared read behind search/query/prime; status maps
/// its own through the chain renderer). With no alias block this is
/// the per-file mtime, byte-compatible with the reads' pre-alias
/// behavior.
///
/// # Errors
///
/// [`Failure::error_line`] when a file's metadata cannot be read.
pub(crate) fn view_modified_ts(
    command: &'static str,
    store: &mulch::StoreFiles,
    domain: &str,
) -> Result<Option<jiff::Timestamp>, Failure> {
    modified_ts(command, store.view_modified(domain))
}

/// The mtime→Timestamp mapping the two modified seams share.
fn modified_ts(
    command: &'static str,
    mtime: mulch::Result<Option<std::time::SystemTime>>,
) -> Result<Option<jiff::Timestamp>, Failure> {
    match mtime {
        Ok(Some(time)) => jiff::Timestamp::try_from(time)
            .map(Some)
            .map_err(|source| Failure::error_line(command, source)),
        Ok(None) => Ok(None),
        Err(source) => Err(Failure::error_line(command, source)),
    }
}

/// The canonical's READ VIEW with its failure mapped onto the
/// command's handled shape (mulch-7112): the canonical file's records
/// plus every alias-named contributor, config order — the read
/// family's SET view. Writers and per-file laws keep
/// [`read_domain_records`]: merging files is the destructive healing
/// ADR-0006 forbids, and line-addressed consumers need physical
/// per-file lines.
///
/// # Errors
///
/// [`read_domain_records`]' errors, from whichever file fails.
pub(crate) fn read_domain_view(
    command: &'static str,
    store: &mulch::StoreFiles,
    domain: &str,
    allow_unknown: bool,
) -> Result<Vec<mulch::LineRecord>, Failure> {
    store
        .read_view(domain, allow_unknown)
        .map_err(|source| Failure::handled_on_stderr(command, render_core_error(&source)))
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

/// The two-open write ritual's LOCKED phase (mulch-7f49 ordering,
/// named by mulch-73b5): the pre-phase ran OUTSIDE (hooks, gates,
/// verdicts, the output record); this opens the writer lock, re-reads
/// the target domain STRICTLY under it, hands both to the mutation,
/// and drops the lock when it returns — the caller's post-record
/// phase then fires outside every lock. Three spellings collapsed
/// into this owner (record's flag path, record --batch's pass, move's
/// source rewrite): they differed only in the mutation, never in the
/// ritual. Callers keep their Failure renderings (ADR-0003) and own
/// the persist/append calls — the write SHAPES differ per command,
/// the open→re-read→mutate→drop order does not.
///
/// # Errors
///
/// [`open_store_mut`]' no-store/lock failures, then
/// [`read_domain_records`]' strict-read failures, then the mutation's
/// own [`Failure`]s — all under the command's name.
pub(crate) fn locked_write_phase<T>(
    command: &'static str,
    config_message: bool,
    domain: &str,
    allow_unknown: bool,
    mutate: impl FnOnce(&mulch::StoreFiles, Vec<mulch::LineRecord>) -> Result<T, Failure>,
) -> Result<T, Failure> {
    let locked = open_store_mut(command, config_message)?;
    let existing = read_domain_records(command, &locked, domain, allow_unknown)?;
    mutate(&locked, existing)
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

    /// The cwd law (mulch-375f): ONE channel decision — the plain
    /// line carries the `Error:` prefix on stderr, the envelope the
    /// prefix-stripped sentence beside it. Catches a drift back to
    /// the stdout channel (the seven pre-fold spellings) and a
    /// double-prefixing.
    #[test]
    fn cwd_failure_is_the_error_line_idiom_on_stderr() {
        let source = std::io::Error::other("gone");
        let failure = cwd_failure("sync", &source);
        assert_eq!(failure.message, "Error: resolving cwd: gone");
        assert_eq!(
            failure.envelope["error"],
            serde_json::json!("resolving cwd: gone"),
            "the envelope carries no prefix"
        );
        assert_eq!(failure.envelope["command"], serde_json::json!("sync"));
        assert!(
            failure.envelope_to_stderr,
            "environmental errors ride stderr"
        );
        assert!(!failure.rendered);
    }

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

    /// A miss names the typed domain and lists the configured
    /// domains in config order (the read-family shape). An alias can
    /// never be a miss (it resolves to a configured canonical), so the
    /// typed name IS the canonical here — the assertion pins the
    /// wording and the list, not the address law.
    #[test]
    fn resolve_domain_set_miss_names_the_typed_domain_and_lists_available() {
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

    /// The seam opens from the CWD like every command: these tests
    /// chdir into the scratch store, serialized by one mutex and
    /// restored after, so they stay safe under plain `cargo test`
    /// (one process) as well as nextest (process per test).
    fn in_scratch_cwd<T>(root: &std::path::Path, body: impl FnOnce() -> T) -> T {
        static CWD: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = CWD
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let previous = std::env::current_dir().expect("cwd readable");
        std::env::set_current_dir(root).expect("chdir into the scratch store");
        let result = body();
        std::env::set_current_dir(previous).expect("chdir back");
        result
    }

    /// The two-open ritual's owner (mulch-73b5): the writer lock is
    /// HELD across the mutation — a contender on another thread
    /// cannot take it while the closure runs — and released when the
    /// phase returns, so the post-phase hooks run outside every lock.
    #[test]
    fn locked_write_phase_holds_the_lock_across_the_mutation() {
        let (root, _store) = alias_store("phase-lock");
        let mulch_dir = root.join(".mulch");
        in_scratch_cwd(&root, || {
            // Boundary timeout, never a sleep: the contender signals
            // ONLY after acquiring; while the mutation holds the lock
            // the channel must stay silent.
            let (held_tx, held_rx) = std::sync::mpsc::channel();
            let contender =
                locked_write_phase("record", false, "dev", false, |_locked, existing| {
                    assert!(existing.is_empty(), "the strict re-read is delivered");
                    let contender_dir = mulch_dir.clone();
                    let contender = std::thread::spawn(move || {
                        // The 5s acquire budget waits out this closure's
                        // lock; the send happens only on acquisition.
                        drop(mulch::lock::WriterLock::acquire(&contender_dir));
                        held_tx.send(()).expect("sender alive");
                    });
                    assert!(
                        held_rx
                            .recv_timeout(std::time::Duration::from_millis(150))
                            .is_err(),
                        "the writer lock is held during the mutation"
                    );
                    // The handle rides out of the phase for the join below.
                    Ok(contender)
                })
                .expect("phase runs");
            // The lock dropped with the phase: the contender acquires
            // within its budget and signals.
            held_rx
                .recv_timeout(std::time::Duration::from_secs(2))
                .expect("the lock is released after the phase");
            contender.join().expect("contender thread");
        });
        std::fs::remove_dir_all(&root).ok();
    }

    /// The seam's re-read is the STRICT one under the command's
    /// rendering (mulch-73b5): a malformed line aborts the phase
    /// BEFORE the mutation runs, with the shared chain wording.
    #[test]
    fn locked_write_phase_fails_the_strict_read_before_the_mutation() {
        let (root, _store) = alias_store("phase-strict");
        std::fs::create_dir_all(root.join(".mulch/expertise")).expect("dir");
        std::fs::write(root.join(".mulch/expertise/dev.jsonl"), "not json\n").expect("bad line");
        in_scratch_cwd(&root, || {
            let Err(failure) = locked_write_phase(
                "record",
                false,
                "dev",
                false,
                |_, _| -> Result<(), Failure> {
                    Err(Failure::handled("record", "the mutation must not run"))
                },
            ) else {
                panic!("expected the strict read to fail");
            };
            assert!(
                failure.message.contains("dev.jsonl"),
                "names the file: {}",
                failure.message
            );
            assert!(
                failure.message.contains("JSON Parse error"),
                "the chain wording: {}",
                failure.message
            );
        });
        std::fs::remove_dir_all(&root).ok();
    }

    /// The closure receives the LOCKED store (mulch-73b5): a persist
    /// through it lands in the domain file — the mutation owns the
    /// write shape, the seam owns the open→read→drop order.
    #[test]
    fn locked_write_phase_hands_the_locked_store_to_the_mutation() {
        let (root, _store) = alias_store("phase-write");
        let record = serde_json::json!({
            "type": "pattern",
            "classification": "tactical",
            "recorded_at": "2026-10-26T10:00:00.000Z",
            "name": "through the seam",
            "description": "d",
        });
        in_scratch_cwd(&root, || {
            locked_write_phase("record", false, "dev", false, |locked, _| {
                persist("record", locked, "dev", std::slice::from_ref(&record))
            })
            .expect("phase runs");
        });
        let text = std::fs::read_to_string(root.join(".mulch/expertise/dev.jsonl")).expect("file");
        assert!(text.contains("through the seam"), "{text}");
        std::fs::remove_dir_all(&root).ok();
    }
}
