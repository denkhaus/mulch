//! `mulch archive` — soft-archive live records by id (mulch-7c55,
//! sprint-39 child A replaces this stub).

use crate::cli::GlobalOpts;
use crate::output::Failure;

/// Runs `archive`.
pub(super) fn run(
    _opts: &GlobalOpts,
    _domain: &str,
    _id: Option<&str>,
    _records: Option<&str>,
    _reason: &str,
    _dry_run: bool,
) -> Result<(), Failure> {
    todo!("mulch-7c55 child A lands archive")
}
