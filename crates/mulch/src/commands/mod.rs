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

use mulch::Error;
/// Store lookup: the filesystem seam lives in the library
/// ([`mulch::StoreFiles`]); the CLI only matches its outcome.
pub(crate) use mulch::Located as StoreLocation;

use crate::cli::{Cli, Command};
use crate::output::Failure;

/// The handled-error message for a missing store (status et al.).
pub(crate) const NO_STORE_MESSAGE: &str = "No .mulch/ directory found. Run `mulch init` first.";

/// The no-store message of the read-modify-write commands: the
/// reference's config reader throws it (with the `Error: ` prefix the
/// command adds).
pub(crate) const NO_STORE_CONFIG_MESSAGE: &str =
    "Error: No .mulch/ directory found. Run `mulch init` to set up this project.";

/// The reference's second wording, thrown by its config reader when
/// `.mulch/` exists without a config (we render it as a clean error).
pub(crate) const NO_CONFIG_MESSAGE: &str =
    "No .mulch/ directory found. Run `mulch init` to set up this project.";

/// Locates the store for the parity commands.
pub(crate) fn locate(root: &Path) -> Result<mulch::Located, mulch::Error> {
    mulch::StoreFiles::locate(root)
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

/// The shared unknown-id message (reference text).
pub(crate) fn record_not_found_text(id: &str) -> String {
    format!("Error: Record \"{id}\" not found. Run `mulch query` to see record IDs.")
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
