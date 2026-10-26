//! `mulch update` — the reference's hidden deprecated stub
//! (update.ts, 19 lines): prints the migration warning and exits 1,
//! nothing else. Ported with the umbrella sweep (mulch-0e39) so a
//! user typing the retired name gets the pointer to `mulch upgrade`
//! instead of a clap parse error. Hidden like the reference's
//! `{ hidden: true }` — it never appears in --help or the
//! completions word list (the honest-help pins hold).

use crate::cli::GlobalOpts;
use crate::output::{Failure, print_line};

/// The one sentence the stub prints on every invocation
/// (update.ts:13/15; `--check` is accepted and ignored — its only
/// job is keeping old invocations past flag validation).
const MESSAGE: &str = "'mulch update' is deprecated. Use 'mulch upgrade' instead.";

/// Runs `update`: plain prints the `!`-prefixed warning line on
/// STDOUT (the reference's `printWarning`, no ANSI when piped), json
/// renders the error envelope on STDERR; exit 1 in both modes
/// (probe-pinned vs ml 0.10.7).
pub(super) fn run(opts: &GlobalOpts, _check: bool) -> Result<(), Failure> {
    if opts.json {
        return Err(Failure::handled_on_stderr("update", MESSAGE));
    }
    print_line(false, &format!("! {MESSAGE}"));
    Err(Failure::rendered_exit("update"))
}
