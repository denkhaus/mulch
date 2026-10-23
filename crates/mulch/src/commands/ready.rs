//! `mulch ready` — recently added or updated expertise records
//! (mulch-16da; implementation lands with the sprint-35 child
//! delivery).

use crate::cli::GlobalOpts;
use crate::output::Failure;

pub(crate) fn run(
    _opts: &GlobalOpts,
    _limit: &str,
    _domain: Option<&str>,
    _since: Option<&str>,
) -> Result<(), Failure> {
    todo!("mulch-16da: `ready` lands with the sprint-35 child delivery")
}
