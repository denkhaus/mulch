//! The filesystem-facing half of a `.mulch/` store: discovery, config
//! mutations, and the explicit read/write policies the commands declare.
//!
//! Read policies (reference semantics):
//! - Lenient (documented policy, not yet a type — mulch-00aa): status/
//!   validate/doctor: malformed lines and unregistered types are findings, not
//!   failures.
//! - Strict — every mutating command: unparsable lines and unregistered types
//!   are typed errors *before* any write (reference `readExpertiseFile`).
//!
//! Write policies:
//! - [`StoreFiles::rewrite_domain`] — compact canonical re-serialization with
//!   id generation (reference `writeExpertiseFile`); an empty record set leaves
//!   a 0-byte file.
//! - [`StoreFiles::append_domain_line`] — append verbatim, existing bytes
//!   preserved.
//! - [`StoreFiles::rewrite_archive`] / [`StoreFiles::archive_records`] — the
//!   soft-archive writes (reference `writeArchiveFile` / `archiveRecords`):
//!   banner + compact lines, no id assignment; an empty record set still writes
//!   the banner alone.
//! - [`StoreFiles::move_to_archive`] / [`StoreFiles::revive`] — the two-file
//!   transaction seam owning the live↔archive write ORDER (mulch-4cde);
//!   failures carry [`TransactionError`] naming the failed half.
//!
//! Config writes always go through the canonical YAML serializer with
//! defaults backfilled (`Config::to_yaml`), and domain removal keeps the
//! config's domain order.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::records::{LenientLine, LineRecord, read_strict};
use crate::upsert::write_records;

/// The soft-archive banner (reference `ARCHIVE_BANNER`, archive.ts).
/// DEVIATION (README): the reference promotes its own binary — `Run
/// \`ml restore <id>\`` — while our shipped strings name THIS binary
/// (user directive 2026-10-13, the same law as the record retry-echo),
/// so ours reads `mulch restore`.
const ARCHIVE_BANNER: &str = "# ARCHIVED — not for active use. Run `mulch restore <id>` to revive.";

/// Which half of a two-file store transaction failed (mulch-4cde).
/// The variants name the HALF, not the order — [`StoreFiles::revive`]
/// writes the archive half first, [`StoreFiles::move_to_archive`] the
/// live half. Callers map each half onto their own parity-pinned
/// failure surface (they differ per command); [`Self::as_error`]
/// exposes the underlying store error for chain rendering.
#[derive(Debug, thiserror::Error)]
pub enum TransactionError {
    /// The live domain rewrite failed.
    #[error("live rewrite failed: {0}")]
    Live(#[source] Error),
    /// The soft-archive write failed.
    #[error("archive write failed: {0}")]
    Archive(#[source] Error),
}

impl TransactionError {
    /// The underlying store error of the failed half (what the
    /// commands' chain renderer prints — the wrapper's own Display
    /// never reaches a user surface).
    #[must_use]
    pub fn as_error(&self) -> &Error {
        match self {
            Self::Live(source) | Self::Archive(source) => source,
        }
    }
}

/// Where a store lookup landed.
#[derive(Debug)]
pub enum StoreLocation {
    /// `.mulch/` absent.
    Missing,
    /// `.mulch/` exists but `mulch.config.yaml` does not (the reference
    /// crashes here with a runtime stack trace; callers render a clean
    /// error — README DEVIATIONS).
    NoConfig,
    /// Config parsed; domain files are read per command with the
    /// declared policy.
    Open(StoreFiles),
}

/// A `.mulch/` directory with its parsed config: the single owner of the
/// store's filesystem conventions.
///
/// A store is a session, not a shareable snapshot: sessions opened
/// through [`StoreFiles::locate_exclusive`] (mulch-228d) hold the
/// store writer lock until they drop, making the whole
/// load→mutate→save cycle atomic against other mulch processes.
/// Embedders that mutate a store must open it exclusively.
#[derive(Debug)]
pub struct StoreFiles {
    root:     PathBuf,
    config:   Config,
    /// The merged type registry (builtins + validated custom_types),
    /// built once at open (mulch-b4bd).
    registry: crate::ids::Registry,
    /// The writer lock held by exclusive sessions; `None` for
    /// read-only sessions. Held for its drop side effect — releasing
    /// the lock — never read.
    #[expect(dead_code, reason = "holding the lock is the field's whole purpose")]
    lock:     Option<crate::lock::WriterLock>,
}

impl StoreFiles {
    /// Locates the store under `project_root` (which holds `.mulch/`).
    ///
    /// # Errors
    ///
    /// [`Error::Read`] when the config exists but cannot be read,
    /// [`Error::ConfigParse`] / [`Error::UnsupportedVersion`] when it
    /// does not parse.
    pub fn locate(project_root: &Path) -> Result<StoreLocation> {
        Self::locate_inner(&project_root.join(".mulch"), None)
    }

    /// Locates the store under the writer lock (mulch-228d): the
    /// advisory exclusive lock on `.mulch/.lock` is acquired BEFORE
    /// the config is read and held until the store drops, so the
    /// whole load→mutate→save cycle of one session is atomic against
    /// other mulch processes. A concurrent writer waits up to the
    /// acquire timeout, then fails fast with [`Error::LockTimeout`].
    /// Every mutating command opens this way; read-only sessions use
    /// [`StoreFiles::locate`].
    ///
    /// # Errors
    ///
    /// [`Error::Lock`] / [`Error::LockTimeout`] when the lock cannot
    /// be taken, plus the [`StoreFiles::locate`] failures.
    pub fn locate_exclusive(project_root: &Path) -> Result<StoreLocation> {
        let root = project_root.join(".mulch");
        // The guard is load-bearing: acquiring on a missing store
        // would create `.mulch/.lock` (and report a lock error) where
        // every caller expects `Missing`.
        if !root.is_dir() {
            return Ok(StoreLocation::Missing);
        }
        let lock = crate::lock::WriterLock::acquire(&root)?;
        Self::locate_inner(&root, Some(lock))
    }

    /// The shared locate body: discovery, config read, and the
    /// optional pre-acquired writer lock.
    fn locate_inner(root: &Path, lock: Option<crate::lock::WriterLock>) -> Result<StoreLocation> {
        if !root.is_dir() {
            return Ok(StoreLocation::Missing);
        }
        let config_path = root.join("mulch.config.yaml");
        if !config_path.is_file() {
            return Ok(StoreLocation::NoConfig);
        }
        let text = std::fs::read_to_string(&config_path).map_err(|source| Error::Read {
            path: config_path,
            source,
        })?;
        let config = Config::parse(&text)?;
        config.ensure_supported()?;
        let registry = crate::ids::Registry::from_config(&config)?;
        Ok(StoreLocation::Open(Self {
            root: root.to_path_buf(),
            config,
            registry,
            lock,
        }))
    }

    /// Lenient per-line read of one domain file (reference validate/
    /// doctor raw loops): parse outcomes are findings — including
    /// `#` comment lines, which those loops flag as invalid JSON
    /// (only the strict reader skips them). Missing files read
    /// empty.
    pub fn read_lenient(&self, domain: &str) -> Vec<LenientLine> {
        let mut lines = crate::records::read_lenient(&self.domain_path(domain));
        // Aliases apply at this seam too (the reference's validate and
        // doctor checks parse their own lines but run `applyAliases`
        // before the schema verdict — same canonical view).
        for line in &mut lines {
            if let LenientLine::Record { record, .. } = line {
                crate::records::apply_aliases_for(&self.registry, record);
            }
        }
        lines
    }

    /// The `.mulch` directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The parsed config.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// The merged type registry (builtins + the config's
    /// custom_types, validated at open; mulch-b4bd).
    pub fn registry(&self) -> &crate::ids::Registry {
        &self.registry
    }

    /// Live domain names in config order.
    pub fn domains(&self) -> Vec<String> {
        self.config
            .domains()
            .into_iter()
            .map(String::from)
            .collect()
    }

    /// The live expertise file of a domain.
    pub fn domain_path(&self, domain: &str) -> PathBuf {
        self.root.join("expertise").join(format!("{domain}.jsonl"))
    }

    /// Registers a domain: name validation, canonical config rewrite and
    /// an empty expertise file (reference `add`/auto-create behavior).
    ///
    /// # Errors
    ///
    /// [`Error::InvalidDomain`] for a bad name, [`Error::Write`] on I/O.
    pub fn register_domain(&mut self, domain: &str) -> Result<()> {
        if !valid_domain(domain) {
            return Err(Error::InvalidDomain {
                domain: domain.into(),
            });
        }
        self.config.add_domain(domain);
        self.write_config()?;
        self.ensure_domain_file(domain).map(|_| ())
    }

    /// Adopts an on-disk domain into the config WITHOUT name
    /// validation and WITHOUT writing (the doctor orphan repair
    /// registers whatever `.jsonl` files exist — the reference's
    /// orphan fix adds them unvalidated; its `add` path is the one
    /// that validates). Callers batch their config rewrite.
    pub fn adopt_domain(&mut self, domain: &str) {
        self.config.add_domain(domain);
    }

    /// Creates a domain's missing expertise file (empty), reporting
    /// whether it had to (the doctor orphan repair's second half).
    ///
    /// # Errors
    ///
    /// [`Error::Write`] on I/O.
    pub fn ensure_domain_file(&self, domain: &str) -> Result<bool> {
        let path = self.domain_path(domain);
        if path.is_file() {
            return Ok(false);
        }
        std::fs::write(&path, "")
            .map(|()| true)
            .map_err(|source| Error::Write { path, source })
    }

    /// Removes a domain from the config (order preserved).
    ///
    /// # Errors
    ///
    /// [`Error::Write`] on I/O.
    pub fn remove_domain(&mut self, domain: &str) -> Result<()> {
        self.config.remove_domain(domain);
        self.write_config()
    }

    /// Deletes a domain entirely: config entry removed and the live
    /// expertise file deleted (the archive file stays).
    ///
    /// # Errors
    ///
    /// [`Error::Write`] on I/O.
    pub fn delete_domain(&mut self, domain: &str) -> Result<()> {
        self.remove_domain(domain)?;
        let path = self.domain_path(domain);
        if path.is_file() {
            std::fs::remove_file(&path).map_err(|source| Error::Remove { path, source })?;
        }
        Ok(())
    }

    /// Rewrites the config through the canonical serializer.
    ///
    /// Atomic (temp + rename): `record`'s pre-hook
    /// phase reads the config LOCK-FREE (mulch-7f49, the reference's
    /// order), so a concurrent domain auto-create must never expose a
    /// truncated file to a reader (mulch-228d parallel writers).
    ///
    /// # Errors
    ///
    /// [`Error::Write`] on I/O.
    pub fn write_config(&self) -> Result<()> {
        let path = self.root.join("mulch.config.yaml");
        atomic_write(&path, self.config.to_yaml().as_bytes())
    }

    /// A domain's records, STRICT (the reference `readExpertiseFile`):
    /// unparsable lines and unregistered types are typed errors, so a
    /// caller never mutates a store it could not read. Scalar and
    /// `null` lines are errors too (the reference reader crashes on
    /// them — README DEVIATIONS); blank and `#` comment lines are
    /// skipped, arrays pass as records. `allow_unknown` is the CLI's
    /// `--allow-unknown-types` escape hatch (worktree/CI lag): it
    /// tolerates unregistered types but never malformed lines.
    ///
    /// # Errors
    ///
    /// [`Error::MalformedLine`] for unparsable lines,
    /// [`Error::NotAnObject`] for scalar/`null` lines,
    /// [`Error::UnknownRecordType`] for unregistered types (unless
    /// `allow_unknown`), [`Error::Read`] for I/O failures.
    pub fn read_records(&self, domain: &str, allow_unknown: bool) -> Result<Vec<LineRecord>> {
        read_strict(&self.registry, &self.domain_path(domain), allow_unknown)
    }

    /// A domain file's modification time (a read need the reporting
    /// commands have; `None` when the file does not exist).
    ///
    /// # Errors
    ///
    /// [`Error::Read`] when the file exists but its metadata cannot be
    /// read.
    pub fn domain_modified(&self, domain: &str) -> Result<Option<std::time::SystemTime>> {
        let path = self.domain_path(domain);
        match std::fs::metadata(&path) {
            Ok(metadata) => metadata
                .modified()
                .map(Some)
                .map_err(|source| Error::Read { path, source }),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(Error::Read { path, source }),
        }
    }

    /// The soft-archive file of a domain (`.mulch/archive/
    /// <domain>.jsonl`; the archive command's write side — 7c55 —
    /// owns creating it, `restore` consumes it).
    pub fn archive_path(&self, domain: &str) -> PathBuf {
        self.root.join("archive").join(format!("{domain}.jsonl"))
    }

    /// Reads a domain's soft-archive (`readArchiveFile`: the same
    /// line law as live reads — `#` banner and blank lines skip,
    /// aliases apply — but ALWAYS unknown-type tolerant, the escape
    /// hatch hard-wired for archived records). `Ok(vec![])` when no
    /// archive exists.
    ///
    /// # Errors
    ///
    /// [`Error::MalformedLine`] for unparsable lines,
    /// [`Error::NotAnObject`] for scalar/`null` lines,
    /// [`Error::Read`] for I/O failures.
    pub fn read_archive(&self, domain: &str) -> Result<Vec<LineRecord>> {
        // `true` = the hard-wired unknown-type tolerance above.
        crate::records::read_strict(&self.registry, &self.archive_path(domain), true)
    }

    /// Rewrites a domain's soft-archive (`writeArchiveFile`): the
    /// banner line, then compact JSON lines, one trailing newline
    /// after the last record — so an EMPTY record set still writes
    /// the banner plus newline (never the 0-byte empty of
    /// [`StoreFiles::rewrite_domain`]). No id is assigned: archived
    /// records keep the ids they carried, including none. Creates
    /// the archive directory when missing; writes through a temp
    /// file + rename.
    ///
    /// # Errors
    ///
    /// [`Error::Write`] on I/O.
    pub fn rewrite_archive(&self, domain: &str, records: &[Value]) -> Result<()> {
        let path = self.archive_path(domain);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| Error::Write {
                path: path.clone(),
                source,
            })?;
        }
        let mut body = String::from(ARCHIVE_BANNER);
        body.push('\n');
        for (index, record) in records.iter().enumerate() {
            if index > 0 {
                body.push('\n');
            }
            body.push_str(&serde_json::to_string(record).unwrap_or_default());
        }
        if !records.is_empty() {
            body.push('\n');
        }
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let temp = PathBuf::from(format!("{}.tmp.{unique:x}", path.display()));
        std::fs::write(&temp, body).map_err(|source| Error::Write {
            path: temp.clone(),
            source,
        })?;
        std::fs::rename(&temp, &path).map_err(|source| {
            std::fs::remove_file(&temp).ok();
            Error::Write { path, source }
        })
    }

    /// Moves records into a domain's soft-archive (`archiveRecords`):
    /// stamps each record with the archive lifecycle fields (the
    /// private `stamp_archive_fields` law), merges the archive's
    /// existing records first, and rewrites — existing lines pass
    /// through byte-stably. Empty input is a no-op: no read, no
    /// write. The caller owns the source file's rewrite (and, in the
    /// reference,
    /// its lock).
    ///
    /// # Errors
    ///
    /// [`Error::Read`] / [`Error::Write`] on I/O,
    /// [`Error::MalformedLine`] / [`Error::NotAnObject`] for
    /// unparsable existing archive lines.
    pub fn archive_records(
        &self,
        domain: &str,
        records: &[Value],
        now: &str,
        reason: &str,
    ) -> Result<()> {
        if records.is_empty() {
            return Ok(());
        }
        let stamped: Vec<Value> = records
            .iter()
            .map(|record| stamp_archive_fields(record, now, reason))
            .collect();
        let mut merged: Vec<Value> = self
            .read_archive(domain)?
            .into_iter()
            .map(|line| line.record)
            .collect();
        merged.extend(stamped);
        self.rewrite_archive(domain, &merged)
    }

    /// Moves records live→archive in one transaction (mulch-4cde):
    /// rewrites the live domain file with `keep`, then merges
    /// `archived` into the soft archive under the stamp law
    /// (`now`/`reason`, existing-first, `??` spare). The projection
    /// to the record sets stays at the call site — it differs per
    /// command; this seam owns the ORDER and the failure window.
    /// Callers hold the store writer lock across the whole cycle.
    ///
    /// # Errors
    ///
    /// [`TransactionError::Live`] when the live rewrite fails (the
    /// archive is untouched); [`TransactionError::Archive`] when the
    /// archive write fails AFTER the live rewrite — the documented
    /// window: the live file is already rewritten, so the moved
    /// records are neither live nor archived (the reference's
    /// append-after-write has the same window; a rollback seam would
    /// change failure behaviour the parity laws pin).
    pub fn move_to_archive(
        &self,
        domain: &str,
        keep: &[Value],
        archived: &[Value],
        now: &str,
        reason: &str,
    ) -> std::result::Result<(), TransactionError> {
        self.rewrite_domain(domain, keep)
            .map_err(TransactionError::Live)?;
        self.archive_records(domain, archived, now, reason)
            .map_err(TransactionError::Archive)
    }

    /// Revives records archive→live in one transaction (mulch-4cde):
    /// rewrites the soft archive with `remaining` FIRST, then the
    /// live domain file with `restored` — the reverse order of
    /// [`StoreFiles::move_to_archive`], pinning the reference's
    /// restore sequence under one owner. The caller strips the
    /// archive lifecycle fields and computes both sets; the seam owns
    /// the order and the failure window (an archive-side success
    /// followed by a live failure leaves the record spliced out of
    /// the archive but not live — same window the reference has).
    ///
    /// # Errors
    ///
    /// [`TransactionError::Archive`] when the archive splice fails
    /// (the live file is untouched); [`TransactionError::Live`] when
    /// the live rewrite fails after the splice.
    pub fn revive(
        &self,
        domain: &str,
        remaining: &[Value],
        restored: &[Value],
    ) -> std::result::Result<(), TransactionError> {
        self.rewrite_archive(domain, remaining)
            .map_err(TransactionError::Archive)?;
        self.rewrite_domain(domain, restored)
            .map_err(TransactionError::Live)
    }

    /// Rewrites a domain compactly (canonical; 0 bytes when empty).
    ///
    /// # Errors
    ///
    /// [`Error::Write`] on I/O.
    pub fn rewrite_domain(&self, domain: &str, records: &[Value]) -> Result<()> {
        write_records(&self.registry, &self.domain_path(domain), records)
    }

    /// Appends one line verbatim (existing bytes preserved).
    ///
    /// # Errors
    ///
    /// [`Error::Read`] / [`Error::Write`] on I/O.
    pub fn append_domain_line(&self, domain: &str, line: &str) -> Result<()> {
        let path = self.domain_path(domain);
        let mut bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(source) => return Err(Error::Read { path, source }),
        };
        if !bytes.is_empty() && !bytes.ends_with(b"\n") {
            bytes.push(b'\n');
        }
        bytes.extend_from_slice(line.as_bytes());
        bytes.push(b'\n');
        atomic_write(&path, &bytes)
    }
}

/// Writes `body` through a unique scratch file and an atomic rename, so
/// a concurrent lock-free reader never observes a truncated half-file
/// (the pattern the record writer already uses in `upsert.rs`). The
/// scratch name carries the pid and a nanosecond stamp — two writers
/// never share one — and a failed rename cleans it up best-effort.
fn atomic_write(path: &Path, body: &[u8]) -> Result<()> {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let temp = PathBuf::from(format!(
        "{}.tmp.{}.{unique:x}",
        path.display(),
        std::process::id()
    ));
    std::fs::write(&temp, body).map_err(|source| Error::Write {
        path: temp.clone(),
        source,
    })?;
    std::fs::rename(&temp, path).map_err(|source| {
        std::fs::remove_file(&temp).ok();
        Error::Write {
            path: path.to_path_buf(),
            source,
        }
    })
}

/// Stamps one record with the soft-archive lifecycle fields
/// (`{ ...r, status: "archived", archived_at: r.archived_at ?? now,
/// archive_reason: r.archive_reason ?? reason }`): keys already on
/// the record keep their position — `status` always (its value is
/// replaced), `archived_at` / `archive_reason` while non-null (JS
/// `??` falls through on null AND absent, so a pre-existing stamp
/// from a restore rollback survives) — and absent keys append in
/// write order: status, archived_at, archive_reason.
///
/// A NON-OBJECT record coerces to an object of only the three stamps
/// (`{...r}` on a primitive boxes it, so the payload is lost) —
/// unreachable from the strict readers, which reject non-objects
/// before any write.
fn stamp_archive_fields(record: &Value, now: &str, reason: &str) -> Value {
    let mut object = record.as_object().cloned().unwrap_or_default();
    object.insert("status".into(), Value::String("archived".into()));
    for (key, fallback) in [("archived_at", now), ("archive_reason", reason)] {
        let survives = object.get(key).is_some_and(|value| !value.is_null());
        if !survives {
            object.insert(key.into(), Value::String(fallback.into()));
        }
    }
    Value::Object(object)
}

/// Valid domain names (reference `src/utils/config.ts:204`).
fn valid_domain(domain: &str) -> bool {
    let mut chars = domain.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// A scratch store with one configured domain (the reference
    /// probe's layout: `ml prune` over a two-record expertise file).
    fn scratch(tag: &str) -> (PathBuf, StoreFiles) {
        let root = std::env::temp_dir().join(format!("mulch-archive-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(root.join(".mulch")).expect("temp dir");
        std::fs::write(
            root.join(".mulch").join("mulch.config.yaml"),
            "version: '1'\ndomains:\n  dev: {}\n",
        )
        .expect("writable config");
        match StoreFiles::locate(&root).expect("store opens") {
            StoreLocation::Open(store) => (root, store),
            other => panic!("expected an open store, got {other:?}"),
        }
    }

    #[test]
    fn rewrite_archive_empty_is_banner_only() {
        let (root, store) = scratch("empty");
        store.rewrite_archive("dev", &[]).expect("write");
        let bytes = std::fs::read(root.join(".mulch/archive/dev.jsonl")).expect("archive exists");
        // 71 bytes: the probed 68-byte banner+newline (ml) plus the
        // three extra bytes of our `mulch` restore hint — never the
        // 0-byte empty of the live-file writer.
        assert_eq!(bytes.len(), 71);
        assert_eq!(
            String::from_utf8(bytes).expect("utf-8"),
            "# ARCHIVED — not for active use. Run `mulch restore <id>` to revive.\n"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn rewrite_archive_writes_banner_plus_compact_lines_without_ids() {
        let (root, store) = scratch("lines");
        // no archive directory exists yet — the write creates it
        store
            .rewrite_archive("dev", &[
                json!({"id": "a", "type": "pattern", "name": "one"}),
                json!({"type": "convention", "content": "no id"}),
            ])
            .expect("write");
        let text = std::fs::read_to_string(root.join(".mulch/archive/dev.jsonl")).expect("archive");
        assert_eq!(
            text,
            "# ARCHIVED — not for active use. Run `mulch restore <id>` to revive.\n\
             {\"id\":\"a\",\"type\":\"pattern\",\"name\":\"one\"}\n\
             {\"type\":\"convention\",\"content\":\"no id\"}\n"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn archive_records_stamps_appending_absent_and_sparing_present() {
        let (root, store) = scratch("stamp");
        store
            .archive_records(
                "dev",
                &[
                    json!({"id": "fresh", "type": "pattern", "name": "n", "recorded_at": "2020-01-02T03:04:05.678Z"}),
                    json!({"id": "re", "type": "pattern", "name": "m", "status": "active", "recorded_at": "2020-02-02T00:00:00.000Z", "archived_at": "2021-05-05T05:05:05.005Z", "archive_reason": "manual: rollback"}),
                    json!({"id": "nul", "type": "pattern", "name": "k", "archived_at": null}),
                ],
                "2026-10-24T11:02:49.753Z",
                "stale",
            )
            .expect("archive");
        let text = std::fs::read_to_string(root.join(".mulch/archive/dev.jsonl")).expect("archive");
        let lines: Vec<&str> = text.lines().skip(1).collect();
        // probed: absent keys APPEND at object end, in write order
        assert_eq!(
            lines[0],
            "{\"id\":\"fresh\",\"type\":\"pattern\",\"name\":\"n\",\"recorded_at\":\"2020-01-02T03:04:05.678Z\",\"status\":\"archived\",\"archived_at\":\"2026-10-24T11:02:49.753Z\",\"archive_reason\":\"stale\"}"
        );
        // probed: pre-existing stamps survive ??; status is replaced
        // IN PLACE (key position preserved, value overwritten)
        assert_eq!(
            lines[1],
            "{\"id\":\"re\",\"type\":\"pattern\",\"name\":\"m\",\"status\":\"archived\",\"recorded_at\":\"2020-02-02T00:00:00.000Z\",\"archived_at\":\"2021-05-05T05:05:05.005Z\",\"archive_reason\":\"manual: rollback\"}"
        );
        // ?? falls through on null too — the key keeps its position
        // and takes the fallback value
        assert_eq!(
            lines[2],
            "{\"id\":\"nul\",\"type\":\"pattern\",\"name\":\"k\",\"archived_at\":\"2026-10-24T11:02:49.753Z\",\"status\":\"archived\",\"archive_reason\":\"stale\"}"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn archive_records_merges_existing_first() {
        let (root, store) = scratch("merge");
        store
            .archive_records(
                "dev",
                &[json!({"id": "one", "type": "pattern", "name": "a"})],
                "2026-01-01T00:00:00.000Z",
                "stale",
            )
            .expect("first archive");
        store
            .archive_records(
                "dev",
                &[json!({"id": "two", "type": "pattern", "name": "b"})],
                "2026-01-02T00:00:00.000Z",
                "stale",
            )
            .expect("second archive");
        let text = std::fs::read_to_string(root.join(".mulch/archive/dev.jsonl")).expect("archive");
        let lines: Vec<&str> = text.lines().skip(1).collect();
        // probed: [...existing, ...stamped] — the first prune's line
        // passes through byte-stably, the new one appends
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("\"id\":\"one\""));
        assert!(lines[1].contains("\"id\":\"two\""));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn archive_records_empty_input_is_a_no_op() {
        let (root, store) = scratch("noop");
        store
            .archive_records("dev", &[], "2026-01-01T00:00:00.000Z", "stale")
            .expect("noop");
        // no read, no write — not even the archive directory appears
        assert!(!root.join(".mulch/archive/dev.jsonl").exists());
        std::fs::remove_dir_all(&root).ok();
    }

    /// The transaction seam names the failed half (mulch-4cde): one
    /// failure test per direction covers what per-command byte
    /// fixtures cannot — which write gave up.
    #[test]
    fn move_to_archive_names_the_failed_half() {
        // Live half: the domain file path occupied by a directory —
        // the rename fails, the archive is untouched.
        let (root, store) = scratch("tx-live");
        std::fs::create_dir_all(root.join(".mulch/expertise")).expect("expertise dir");
        std::fs::create_dir_all(root.join(".mulch/expertise/dev.jsonl")).expect("block");
        let error = store
            .move_to_archive(
                "dev",
                &[],
                &[json!({"id": "a"})],
                "2026-01-01T00:00:00.000Z",
                "stale",
            )
            .expect_err("live write fails");
        assert!(matches!(error, TransactionError::Live(_)));
        assert!(!root.join(".mulch/archive/dev.jsonl").exists());
        std::fs::remove_dir_all(&root).ok();

        // Archive half: `archive` exists as a regular file — the
        // archive half fails AFTER the live rewrite succeeded: the
        // documented window (live already rewritten, records moved
        // nowhere).
        let (root, store) = scratch("tx-archive");
        std::fs::create_dir_all(root.join(".mulch/expertise")).expect("expertise dir");
        std::fs::write(root.join(".mulch/archive"), b"not a dir").expect("block");
        let error = store
            .move_to_archive(
                "dev",
                &[],
                &[json!({"id": "a"})],
                "2026-01-01T00:00:00.000Z",
                "stale",
            )
            .expect_err("archive write fails");
        assert!(matches!(error, TransactionError::Archive(_)));
        assert_eq!(
            std::fs::metadata(root.join(".mulch/expertise/dev.jsonl"))
                .expect("live already rewritten")
                .len(),
            0
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn revive_names_the_failed_half() {
        // Archive half first: blocked archive — the live file is
        // never touched.
        let (root, store) = scratch("tx-revive-archive");
        std::fs::create_dir_all(root.join(".mulch/expertise")).expect("expertise dir");
        std::fs::write(root.join(".mulch/archive"), b"not a dir").expect("block");
        let error = store
            .revive("dev", &[], &[json!({"id": "a"})])
            .expect_err("archive splice fails");
        assert!(matches!(error, TransactionError::Archive(_)));
        assert!(!root.join(".mulch/expertise/dev.jsonl").exists());
        std::fs::remove_dir_all(&root).ok();

        // Live half: the domain file blocked — the archive splice
        // already landed (remaining set = banner only).
        let (root, store) = scratch("tx-revive-live");
        std::fs::create_dir_all(root.join(".mulch/expertise")).expect("expertise dir");
        std::fs::create_dir_all(root.join(".mulch/archive")).expect("archive dir");
        std::fs::write(
            root.join(".mulch/archive/dev.jsonl"),
            concat!(
                "# ARCHIVED — not for active use. Run `mulch restore <id>` to revive.\n",
                "{\"id\":\"a\"}\n",
            ),
        )
        .expect("archive seed");
        std::fs::create_dir_all(root.join(".mulch/expertise/dev.jsonl")).expect("block");
        let error = store
            .revive("dev", &[], &[json!({"id": "a"})])
            .expect_err("live write fails");
        assert!(matches!(error, TransactionError::Live(_)));
        assert_eq!(
            std::fs::read_to_string(root.join(".mulch/archive/dev.jsonl"))
                .expect("archive spliced"),
            "# ARCHIVED — not for active use. Run `mulch restore <id>` to revive.\n"
        );
        std::fs::remove_dir_all(&root).ok();
    }
}
