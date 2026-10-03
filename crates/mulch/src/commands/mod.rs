//! Command implementations, one module per parity-slice command.

mod add;
mod delete;
mod delete_domain;
mod doctor;
mod edit;
mod init;
mod move_cmd;
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

    /// Removes a domain from the config (canonical rewrite, comments
    /// stripped) — the file effects stay with the caller.
    pub(crate) fn remove_domain(&mut self, domain: &str) -> Result<(), Error> {
        self.config.remove_domain(domain);
        std::fs::write(self.root.join("mulch.config.yaml"), self.config.to_yaml()).map_err(
            |source| Error::Write {
                path: self.root.join("mulch.config.yaml"),
                source,
            },
        )
    }

    /// Registers a domain: canonical (comment-free) config rewrite plus
    /// an empty expertise file (reference `add`/auto-create behavior).
    /// Invalid names are rejected here so every registration path (add
    /// and record auto-create) validates identically.
    pub(crate) fn add_domain(&mut self, domain: &str) -> Result<(), Error> {
        if !valid_domain(domain) {
            return Err(Error::InvalidDomain {
                domain: domain.into(),
            });
        }
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
        Command::Delete {
            domain,
            id,
            records,
            all_except,
            dry_run,
        } => {
            let mode =
                delete::Mode::from_args(id.as_deref(), records.as_deref(), all_except.as_deref());
            delete::run(&cli.opts, &mode, domain, *dry_run)
        }
        Command::DeleteDomain {
            domain,
            yes,
            dry_run,
        } => delete_domain::run(&cli.opts, domain, *yes, *dry_run),
        Command::MoveRecord {
            source_domain,
            id,
            target_domain,
            dry_run,
            force,
        } => move_cmd::run(
            &cli.opts,
            source_domain,
            id,
            target_domain,
            *dry_run,
            *force,
        ),
    }
}

/// The current instant as reference-format `recorded_at`
/// (ISO-8601, millisecond precision, `Z`).
pub(crate) fn now_iso() -> String {
    jiff::Timestamp::now()
        .strftime("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

/// Valid domain names (reference `src/utils/config.ts:204`).
pub(crate) fn valid_domain(domain: &str) -> bool {
    let mut chars = domain.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Reads a domain's physical lines (blank lines included); a missing
/// file reads as empty, real I/O failures propagate.
pub(crate) fn read_domain_lines(
    store_root: &Path,
    domain: &str,
) -> Result<Vec<String>, std::io::Error> {
    match std::fs::read_to_string(domain_file(store_root, domain)) {
        Ok(text) => Ok(text.lines().map(String::from).collect()),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(source) => Err(source),
    }
}

/// Writes a domain's lines back (LF-terminated).
pub(crate) fn write_domain_lines(
    store_root: &Path,
    domain: &str,
    lines: &[String],
) -> Result<(), std::io::Error> {
    // An empty survivor set leaves a 0-byte file (reference contract:
    // the file is never removed).
    let text = if lines.is_empty() {
        String::new()
    } else {
        let mut text = lines.join("\n");
        text.push('\n');
        text
    };
    std::fs::write(domain_file(store_root, domain), text)
}

/// Finds the physical line index of the record with `id`.
pub(crate) fn find_by_id(lines: &[String], id: &str) -> Option<usize> {
    lines.iter().position(|line| {
        serde_json::from_str::<serde_json::Value>(line)
            .ok()
            .and_then(|r| {
                r.get("id")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
            })
            .is_some_and(|existing| existing == id)
    })
}

/// The shared domain-not-found failure (reference text).
pub(crate) fn domain_not_found(command: &str, domain: &str, available: &[String]) -> Failure {
    Failure::handled(
        command,
        format!(
            "Error: domain \"{domain}\" not found in config.\nAvailable domains: {}",
            available.join(", ")
        ),
    )
}

/// The shared unknown-id failure (reference text).
pub(crate) fn record_not_found(command: &str, id: &str) -> Failure {
    Failure::handled(
        command,
        format!("Error: Record \"{id}\" not found. Run `mulch query` to see record IDs."),
    )
}

/// The required-fields hint line content for a record type.
pub(crate) fn hint_fields(record_type: &str) -> String {
    mulch::payload_fields(record_type).join(", ")
}

/// The record summary the reference prints in delete/move messages
/// (payload id-key field value; truncation length is unpinned).
pub(crate) fn record_summary(record: &serde_json::Value) -> String {
    let kind = record
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("convention");
    let field = match kind {
        "pattern" | "reference" | "guide" => "name",
        "failure" => "description",
        "decision" => "title",
        _ => "content",
    };
    record
        .get(field)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// The parsed-record line set of a domain, keeping raw survivor lines
/// and dropping non-record lines (reference rewrite semantics).
pub(crate) fn parsed_lines(lines: &[String]) -> Vec<(usize, serde_json::Value, String)> {
    lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            serde_json::from_str::<serde_json::Value>(line)
                .ok()
                .map(|value| (index, value, line.clone()))
        })
        .collect()
}

/// Reads a one-line answer from stdin (the delete-domain prompt).
pub(crate) fn read_confirmation() -> std::io::Result<String> {
    use std::io::BufRead as _;
    let mut buffer = String::new();
    std::io::stdin().lock().read_line(&mut buffer)?;
    Ok(buffer)
}
