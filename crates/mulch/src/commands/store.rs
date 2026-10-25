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
        Err(source) => Err(match &source {
            mulch::Error::CustomTypes { message } => Failure::config_error(message),
            _ => Failure::handled(command, chain_message_from(&source)),
        }),
    }
}

/// The crate-local alias for the output module's chain renderer.
fn chain_message_from(error: &mulch::Error) -> String {
    crate::output::chain_message(error)
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
/// singular `outcome` key, the strict reader normalizes it away — but
/// the reference's in-memory copy keeps the key (an undefined-valued
/// property after its normalization, the raw value otherwise), and
/// its ajv gate reports the record as having an additional property.
/// `line` is the target's physical (1-based) line number.
pub(crate) fn updated_record_verdict(
    store: &mulch::StoreFiles,
    domain: &str,
    line: usize,
    updated: &serde_json::Value,
) -> mulch::schema::FullVerdict {
    let raw_had_outcome = store.read_lenient(domain).into_iter().any(|finding| {
        matches!(&finding,
            mulch::LenientLine::Record { line: at, record }
                if *at == line
                    && record
                        .as_object()
                        .is_some_and(|object| object.contains_key("outcome")))
    });
    if !raw_had_outcome {
        return mulch::schema::full_verdict(store.registry(), updated);
    }
    // Key presence alone decides the additional-property report; the
    // reference's undefined value is unrepresentable in JSON, and
    // null behaves identically there.
    let mut augmented = updated.clone();
    if let Some(object) = augmented.as_object_mut() {
        object.insert("outcome".into(), serde_json::Value::Null);
    }
    mulch::schema::full_verdict(store.registry(), &augmented)
}

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
    let domains = store.domains();
    if !domains.iter().any(|d| d == domain) {
        return Err(unknown_domain_failure(
            command, domain, &domains, opts.json, shape,
        ));
    }
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

#[cfg(test)]
mod updated_record_verdict_tests {
    use serde_json::json;

    use super::*;

    /// A throwaway store with one live domain `d` and the given raw
    /// domain body (byte laws, not parsed records).
    fn temp_store(tag: &str, body: &str) -> (std::path::PathBuf, mulch::StoreFiles) {
        let dir = std::env::temp_dir().join(format!("mulch-store-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(dir.join(".mulch").join("expertise")).expect("temp store");
        std::fs::write(
            dir.join(".mulch").join("mulch.config.yaml"),
            "version: '1'\ndomains:\n  d: {}\n",
        )
        .expect("config");
        std::fs::write(dir.join(".mulch").join("expertise").join("d.jsonl"), body)
            .expect("domain body");
        let store = match mulch::StoreFiles::locate(&dir).expect("store opens") {
            mulch::StoreLocation::Open(store) => store,
            other => panic!("store must open, got {other:?}"),
        };
        (dir, store)
    }

    fn valid_updated() -> serde_json::Value {
        json!({
            "type": "pattern",
            "classification": "tactical",
            "recorded_at": "2026-10-20T10:00:00.000Z",
            "name": "Kept",
            "description": "kept body",
            "id": "mx-123456",
        })
    }

    /// Without a singular `outcome` key on the target line the verdict
    /// is the plain full-record verdict.
    #[test]
    fn no_singular_outcome_key_takes_the_plain_verdict() {
        let body = format!("{}\n", valid_updated());
        let (dir, store) = temp_store("plain", &body);
        assert!(matches!(
            updated_record_verdict(&store, "d", 1, &valid_updated()),
            mulch::schema::FullVerdict::Valid
        ));
        // An invalid update stays invalid for its own reason (the
        // augmentation adds nothing).
        let mut missing = valid_updated();
        missing
            .as_object_mut()
            .expect("object")
            .remove("description");
        let mulch::schema::FullVerdict::Invalid { subs, .. } =
            updated_record_verdict(&store, "d", 1, &missing)
        else {
            panic!("a record without description must be invalid");
        };
        let rendered = mulch::schema::render_subs(&subs).join(mulch::schema::SUB_SEP);
        assert!(
            rendered.contains("description"),
            "the missing field is reported: {rendered}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A raw `outcome` key on the target line makes the in-memory copy
    /// carry it (the reference's undefined property), so the verdict
    /// reports an additional property even though the strict reader
    /// normalized it away.
    #[test]
    fn singular_outcome_key_augments_the_verdict() {
        let body = format!(
            "{}\n",
            json!({
                "type": "pattern",
                "classification": "tactical",
                "recorded_at": "2026-10-20T10:00:00.000Z",
                "name": "Kept",
                "description": "kept body",
                "id": "mx-123456",
                "outcome": {"status": "success"},
            })
        );
        let (dir, store) = temp_store("outcome", &body);
        let mulch::schema::FullVerdict::Invalid { subs, .. } =
            updated_record_verdict(&store, "d", 1, &valid_updated())
        else {
            panic!("the singular outcome key must fail the record");
        };
        let rendered = mulch::schema::render_subs(&subs).join(mulch::schema::SUB_SEP);
        assert!(
            rendered.contains("must NOT have additional properties"),
            "additional-property finding, got {rendered}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Key presence alone decides (the reference's `undefined` value is
    /// unrepresentable in JSON): a null value augments identically.
    #[test]
    fn null_singular_outcome_augments_like_a_value() {
        let mut raw = valid_updated();
        raw.as_object_mut()
            .expect("object")
            .insert("outcome".into(), serde_json::Value::Null);
        let body = format!("{raw}\n");
        let (dir, store) = temp_store("outcome-null", &body);
        assert!(matches!(
            updated_record_verdict(&store, "d", 1, &valid_updated()),
            mulch::schema::FullVerdict::Invalid { .. }
        ));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// The augmentation is line-exact: an `outcome` key on ANOTHER
    /// physical line never leaks onto the target.
    #[test]
    fn another_lines_outcome_key_does_not_leak() {
        let other = json!({
            "type": "pattern",
            "classification": "tactical",
            "recorded_at": "2026-10-19T10:00:00.000Z",
            "name": "Other",
            "description": "other body",
            "id": "mx-654321",
            "outcome": {"status": "success"},
        });
        let body = format!("{}\n{other}\n", valid_updated());
        let (dir, store) = temp_store("line-exact", &body);
        assert!(matches!(
            updated_record_verdict(&store, "d", 1, &valid_updated()),
            mulch::schema::FullVerdict::Valid
        ));
        // The line carrying the key is the one that fails.
        assert!(matches!(
            updated_record_verdict(&store, "d", 2, &valid_updated()),
            mulch::schema::FullVerdict::Invalid { .. }
        ));
        std::fs::remove_dir_all(&dir).ok();
    }
}
