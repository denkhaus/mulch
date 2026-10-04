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
use crate::records::{LineRecord, read_strict, write_records};

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
#[derive(Debug)]
pub struct StoreFiles {
    root:   PathBuf,
    config: Config,
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
        let root = project_root.join(".mulch");
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
        Ok(StoreLocation::Open(Self { root, config }))
    }

    /// The `.mulch` directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The parsed config.
    pub fn config(&self) -> &Config {
        &self.config
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
        let path = self.domain_path(domain);
        if !path.is_file() {
            std::fs::write(&path, "").map_err(|source| Error::Write { path, source })?;
        }
        Ok(())
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

    /// A domain's physical lines (blanks included); a missing file reads
    /// as empty, real I/O failures propagate.
    ///
    /// # Errors
    ///
    /// [`Error::Read`] for I/O failures other than a missing file.
    pub fn read_lines(&self, domain: &str) -> Result<Vec<String>> {
        let path = self.domain_path(domain);
        match std::fs::read_to_string(&path) {
            Ok(text) => Ok(text.lines().map(String::from).collect()),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(source) => Err(Error::Read { path, source }),
        }
    }

    /// A domain's records, STRICT (the reference `readExpertiseFile`):
    /// unparsable lines and unregistered types are typed errors, so a
    /// caller never mutates a store it could not read. `allow_unknown`
    /// is the CLI's `--allow-unknown-types` escape hatch (worktree/CI
    /// lag): it tolerates unregistered types but never malformed lines.
    ///
    /// # Errors
    ///
    /// [`Error::MalformedLine`] / [`Error::UnknownRecordType`] for bad
    /// lines, [`Error::Read`] for I/O failures.
    pub fn read_records(&self, domain: &str, allow_unknown: bool) -> Result<Vec<LineRecord>> {
        read_strict(&self.domain_path(domain), allow_unknown)
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

    /// Rewrites a domain compactly (canonical; 0 bytes when empty).
    ///
    /// # Errors
    ///
    /// [`Error::Write`] on I/O.
    pub fn rewrite_domain(&self, domain: &str, records: &[Value]) -> Result<()> {
        write_records(&self.domain_path(domain), records)
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
