//! Command implementations, one module per parity-slice command.

mod add;
mod doctor;
mod edit;
pub(crate) mod ids;
mod init;
mod outcome;
mod record;
pub(crate) mod schema;
pub(crate) mod stale;
mod status;
mod validate;

use std::path::{Path, PathBuf};

use mulch::{Config, Error};

use crate::cli::{Cli, Command};
use crate::output::Failure;

/// Where a store lookup landed.
pub(crate) enum StoreLocation {
    /// `.mulch/` absent.
    Missing,
    /// `.mulch/` exists but `mulch.config.yaml` does not (the reference
    /// crashes here with a Bun stack trace; we render a clean error —
    /// README DEVIATIONS).
    NoConfig,
    /// Config parsed; records are read leniently by each command.
    Open(ConfigStore),
}

/// A store reduced to what the parity commands need: the store root
/// and the parsed config. Domain files are read per command so
/// malformed lines surface as findings, never as open failures.
pub(crate) struct ConfigStore {
    /// The `.mulch` directory.
    pub root:   PathBuf,
    /// The parsed `mulch.config.yaml`.
    pub config: Config,
}

impl ConfigStore {
    /// Registered live domains, config order.
    pub(crate) fn domains(&self) -> Vec<String> {
        self.config
            .domains()
            .into_iter()
            .map(String::from)
            .collect()
    }

    /// Registers a domain: canonical (comment-free) config rewrite plus
    /// an empty expertise file (reference `add`/auto-create behavior).
    pub(crate) fn add_domain(&mut self, domain: &str) -> Result<(), Error> {
        self.config.add_domain(domain);
        std::fs::write(self.root.join("mulch.config.yaml"), self.config.to_yaml()).map_err(
            |source| Error::Write {
                path: self.root.join("mulch.config.yaml"),
                source,
            },
        )?;
        let file = domain_file(&self.root, domain);
        if !file.is_file() {
            std::fs::write(&file, "").map_err(|source| Error::Write { path: file, source })?;
        }
        Ok(())
    }
}

/// Locates the store at `root` without loading records and without
/// panicking on the reference's crash paths.
pub(crate) fn locate(root: &Path) -> Result<StoreLocation, Error> {
    let store_root = root.join(".mulch");
    if !store_root.is_dir() {
        return Ok(StoreLocation::Missing);
    }
    let config_path = store_root.join("mulch.config.yaml");
    if !config_path.is_file() {
        return Ok(StoreLocation::NoConfig);
    }
    let text = std::fs::read_to_string(&config_path).map_err(|source| Error::Read {
        path: config_path,
        source,
    })?;
    let config = Config::parse(&text)?;
    config.ensure_supported()?;
    Ok(StoreLocation::Open(ConfigStore {
        root: store_root,
        config,
    }))
}

/// The handled-error message for a missing store (status et al.).
pub(crate) const NO_STORE_MESSAGE: &str = "No .mulch/ directory found. Run `mulch init` first.";

/// The reference's second wording, thrown by its config reader when
/// `.mulch/` exists without a config (we render it as a clean error).
pub(crate) const NO_CONFIG_MESSAGE: &str =
    "No .mulch/ directory found. Run `mulch init` to set up this project.";

/// Live-record file of a domain inside a store root.
pub(crate) fn domain_file(store_root: &Path, domain: &str) -> PathBuf {
    store_root.join("expertise").join(format!("{domain}.jsonl"))
}

/// Runs the parsed command.
pub(crate) fn dispatch(cli: &Cli, command: &Command) -> Result<(), Failure> {
    let _ = &cli.command;
    match command {
        Command::Init => init::run(&cli.opts),
        Command::Status => status::run(&cli.opts),
        Command::Validate => validate::run(&cli.opts),
        Command::Doctor { fix } => doctor::run(&cli.opts, *fix),
        Command::Add { domain } => add::run(&cli.opts, domain.clone()),
        Command::Record(args) => record::run(&cli.opts, args),
        Command::Edit(args) => edit::run(&cli.opts, args),
        Command::Outcome {
            domain,
            id,
            outcome,
        } => outcome::run(&cli.opts, domain, id, outcome),
    }
}

/// The current instant as reference-format `recorded_at`
/// (ISO-8601, millisecond precision, `Z`).
pub(crate) fn now_iso() -> String {
    jiff::Timestamp::now()
        .strftime("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}
