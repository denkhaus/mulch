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
mod hooks_surface;
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

use crate::cli::{Command, GlobalOpts};
use crate::output::Failure;

/// An owned truthy flag string (`None` for empty/falsy) — the shared
/// adapter behind search/query's filter flags (mulch-1c32: two
/// byte-identical private copies).
pub(crate) fn truthy_owned(value: Option<&str>) -> Option<String> {
    mulch::truthy_str(value).map(ToString::to_string)
}

/// Runs the parsed command.
pub(crate) fn dispatch(opts: &GlobalOpts, command: &Command) -> Result<(), Failure> {
    match command {
        Command::Init => init::run(opts),
        Command::Status => status::run(opts),
        Command::Validate => validate::run(opts),
        Command::Doctor { fix } => doctor::run(opts, *fix),
        Command::Prune(args) => prune::run(opts, args),
        Command::Archive {
            domain,
            id,
            records,
            reason,
            dry_run,
        } => archive::run(
            opts,
            domain,
            id.as_deref(),
            records.as_deref(),
            reason,
            *dry_run,
        ),
        Command::Restore { id } => restore::run(opts, id),
        Command::Compact(args) => compact::run(opts, args),
        Command::Add { domain } => add::run(opts, domain.clone()),
        Command::Record(args) => record::run(opts, args),
        Command::Edit(args) => edit::run(opts, args),
        Command::Outcome {
            domain,
            id,
            outcome,
        } => outcome::run(opts, domain, id, outcome),
        Command::Delete {
            domain,
            id,
            records,
            all_except,
            dry_run,
        } => delete::run(
            opts,
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
        } => delete_domain::run(opts, domain, *yes, *dry_run),
        Command::MoveRecord {
            source_domain,
            id,
            target_domain,
            dry_run,
            force,
        } => move_cmd::run(opts, source_domain, id, target_domain, *dry_run, *force),
        Command::Query(args) => query::run(opts, args),
        Command::Search(args) => search::run(opts, args),
        Command::Rank {
            domain,
            record_type,
            limit,
            min_score,
        } => rank::run(
            opts,
            domain.as_deref(),
            record_type.as_deref(),
            limit,
            min_score,
        ),
        Command::Ready {
            limit,
            domain,
            since,
        } => ready::run(opts, limit.as_str(), domain.as_deref(), since.as_deref()),
        Command::Learn { since } => learn::run(opts, since),
        Command::Prime(args) => prime::run(opts, args),
    }
}
