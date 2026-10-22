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
//!
//! Config writes always go through the canonical YAML serializer with
//! defaults backfilled (`Config::to_yaml`), and domain removal keeps the
//! config's domain order.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::records::{LenientLine, LineRecord, read_strict, write_records};

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
    /// # Errors
    ///
    /// [`Error::Write`] on I/O.
    pub fn write_config(&self) -> Result<()> {
        let path = self.root.join("mulch.config.yaml");
        std::fs::write(&path, self.config.to_yaml()).map_err(|source| Error::Write { path, source })
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
        std::fs::write(&path, bytes).map_err(|source| Error::Write { path, source })
    }
}

/// Valid domain names (reference `src/utils/config.ts:204`).
fn valid_domain(domain: &str) -> bool {
    let mut chars = domain.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}
