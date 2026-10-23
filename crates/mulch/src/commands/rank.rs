//! `mulch rank` — records by confirmation-frequency score
//! (mulch-16da; implementation lands with the sprint-35 child
//! delivery).

use crate::cli::GlobalOpts;
use crate::output::Failure;

pub(crate) fn run(
    _opts: &GlobalOpts,
    _domain: Option<&str>,
    _record_type: Option<&str>,
    _limit: &str,
    _min_score: &str,
) -> Result<(), Failure> {
    todo!("mulch-16da: `rank` lands with the sprint-35 child delivery")
}
