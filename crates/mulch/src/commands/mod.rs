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

use std::path::Path;

/// Store lookup: the filesystem seam lives in the library
/// ([`mulch::StoreFiles`]); the CLI only matches its outcome.
pub(crate) use mulch::StoreLocation;
use mulch::{Error, ResolveError};

use crate::cli::{Cli, Command};
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
    let cwd = std::env::current_dir()
        .map_err(|source| Failure::handled(command, format!("resolving cwd: {source}")))?;
    match locate(&cwd) {
        Ok(StoreLocation::Open(store)) => Ok(store),
        Ok(StoreLocation::Missing | StoreLocation::NoConfig) => {
            let message = if config_message {
                NO_STORE_CONFIG_MESSAGE
            } else {
                NO_STORE_MESSAGE
            };
            Err(Failure::handled_on_stderr(command, message))
        }
        Err(source) => Err(Failure::handled(command, chain_message_from(&source))),
    }
}

/// The crate-local alias for the output module's chain renderer.
fn chain_message_from(error: &mulch::Error) -> String {
    crate::output::chain_message(error)
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
            let mode = delete::Mode::from_args(
                cli.opts.json,
                id.as_deref(),
                records.as_deref(),
                all_except.as_deref(),
            )?;
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

/// Which unknown-domain surface a command renders — the reference
/// hand-writes four variants; this carries them as data.
#[derive(Clone, Copy)]
pub(crate) enum DomainFailure {
    /// Two-line plain (lowercase `domain`), one-line json (capital D).
    Standard,
    /// `move`'s single-line plain keeps the capital D.
    Move,
    /// `delete-domain`'s plain carries a Hint instead of the list.
    WithHint,
}

/// The one unknown-domain failure: the available-list join and every
/// message shape live here, not in per-command copies (mulch-9e70).
pub(crate) fn unknown_domain_failure(
    command: &str,
    domain: &str,
    available: &[String],
    json: bool,
    shape: DomainFailure,
) -> Failure {
    let list = if available.is_empty() {
        "(none)".to_string()
    } else {
        available.join(", ")
    };
    let json_line = format!("Domain \"{domain}\" not found in config. Available domains: {list}");
    let message = if json {
        json_line
    } else {
        match shape {
            DomainFailure::Standard => {
                format!(
                    "Error: domain \"{domain}\" not found in config.\nAvailable domains: {list}"
                )
            }
            DomainFailure::Move => format!("Error: {json_line}"),
            DomainFailure::WithHint => format!(
                "Error: domain \"{domain}\" not found in config.\nHint: Run `mulch add {domain}` to create it, or check `mulch status` for existing domains."
            ),
        }
    };
    Failure::handled_on_stderr(command, message)
}

/// The shared identifier-resolution failure: the reference's
/// not-found and ambiguous texts (identical across the record
/// commands; `command` selects the envelope's command field).
pub(crate) fn resolve_failure(command: &str, error: ResolveError) -> Failure {
    match error {
        ResolveError::NotFound(identifier) => Failure::handled_on_stderr(
            command,
            format!(
                "Error: Record \"{identifier}\" not found. Run `mulch query` to see record IDs."
            ),
        ),
        ResolveError::Ambiguous {
            identifier,
            count,
            ids,
        } => Failure::handled_on_stderr(
            command,
            format!(
                "Error: Ambiguous identifier \"{identifier}\" matches {count} records: {}. Use more characters to disambiguate.",
                ids.join(", ")
            ),
        ),
    }
}

/// The required-fields hint line content for a record type.
pub(crate) fn hint_fields(record_type: &str) -> String {
    mulch::payload_fields(record_type).join(", ")
}

/// Reads a one-line answer from stdin (the delete-domain prompt).
pub(crate) fn read_confirmation() -> std::io::Result<String> {
    use std::io::BufRead as _;
    let mut buffer = String::new();
    std::io::stdin().lock().read_line(&mut buffer)?;
    Ok(buffer)
}

/// Renders a format-core read error the way the reference does (the
/// malformed-line and unknown-type contracts are byte-pinned).
pub(crate) fn render_core_error(error: &Error) -> String {
    match error {
        Error::MalformedLine {
            path,
            line,
            preview,
            reason,
        } => format!(
            "Error: Malformed JSONL at {}:{line}: {reason}. Line: {preview}",
            path.display()
        ),
        Error::UnknownRecordType {
            path,
            line,
            id,
            record_type,
        } => {
            let id_part = id
                .as_ref()
                .map_or_else(String::new, |id| format!(" (id={id})"));
            format!(
                "Error: Unknown record type \"{record_type}\" at {}:{line}{id_part}. Register it under custom_types in mulch.config.yaml, remove the record, or pass --allow-unknown-types to bypass.",
                path.display()
            )
        }
        other => format!("Error: {other}"),
    }
}

#[cfg(test)]
mod domain_failure_tests {
    use super::*;

    #[test]
    fn shapes_render_the_reference_surfaces() {
        let available = vec!["d".to_string()];
        let standard =
            unknown_domain_failure("delete", "x", &available, false, DomainFailure::Standard);
        assert_eq!(
            standard.message,
            "Error: domain \"x\" not found in config.\nAvailable domains: d"
        );
        let json = unknown_domain_failure("delete", "x", &available, true, DomainFailure::Standard);
        assert_eq!(
            json.message,
            "Domain \"x\" not found in config. Available domains: d"
        );
        let moved = unknown_domain_failure("move", "x", &available, false, DomainFailure::Move);
        assert_eq!(
            moved.message,
            "Error: Domain \"x\" not found in config. Available domains: d"
        );
        let hint = unknown_domain_failure(
            "delete-domain",
            "x",
            &available,
            false,
            DomainFailure::WithHint,
        );
        assert_eq!(
            hint.message,
            "Error: domain \"x\" not found in config.\nHint: Run `mulch add x` to create it, or check `mulch status` for existing domains."
        );
        let none = unknown_domain_failure("delete", "x", &[], false, DomainFailure::Standard);
        assert!(none.message.ends_with("Available domains: (none)"));
    }
}
