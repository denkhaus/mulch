//! Command dispatch, one module per parity-slice command. The
//! shared seams live in their own modules (mulch-3326):
//! [`store`] (open/read/write), [`failure`] (error shapes +
//! render helpers), [`input`] (flag and prompt adapters), and
//! [`render`] (the format families).

mod active_work;
mod add;
mod archive;
mod compact;
mod delete;
mod delete_domain;
mod doctor;
mod edit;
mod failure;
mod git_context;
mod init;
mod input;
mod learn;
mod move_cmd;
mod outcome;
mod prime;
mod prune;
mod query;
mod rank;
mod ready;
mod record;
mod render;
mod restore;
mod search;
mod status;
mod store;
mod validate;

use crate::cli::{Cli, Command};
use crate::output::Failure;

/// Runs the parsed command.
pub(crate) fn dispatch(cli: &Cli, command: &Command) -> Result<(), Failure> {
    let _ = &cli.command;
    match command {
        Command::Init => init::run(&cli.opts),
        Command::Status => status::run(&cli.opts),
        Command::Validate => validate::run(&cli.opts),
        Command::Doctor { fix } => doctor::run(&cli.opts, *fix),
        Command::Prune(args) => prune::run(&cli.opts, args),
        Command::Archive {
            domain,
            id,
            records,
            reason,
            dry_run,
        } => archive::run(
            &cli.opts,
            domain,
            id.as_deref(),
            records.as_deref(),
            reason,
            *dry_run,
        ),
        Command::Restore { id } => restore::run(&cli.opts, id),
        Command::Compact(args) => compact::run(&cli.opts, args),
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
        } => delete::run(
            &cli.opts,
            id.as_deref(),
            records.as_deref(),
            all_except.as_deref(),
            domain,
            *dry_run,
        ),
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
        Command::Query(args) => query::run(&cli.opts, args),
        Command::Search(args) => search::run(&cli.opts, args),
        Command::Rank {
            domain,
            record_type,
            limit,
            min_score,
        } => rank::run(
            &cli.opts,
            domain.as_deref(),
            record_type.as_deref(),
            limit,
            min_score,
        ),
        Command::Ready {
            limit,
            domain,
            since,
        } => ready::run(
            &cli.opts,
            limit.as_str(),
            domain.as_deref(),
            since.as_deref(),
        ),
        Command::Learn { since } => learn::run(&cli.opts, since),
        Command::Prime(args) => prime::run(&cli.opts, args),
    }
}
