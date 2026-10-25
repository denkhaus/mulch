//! The CLI surface adapters over the lib hook engine (mulch-7f49):
//! the engine never prints, so hook stderr forwarding lives here — one
//! owner for the ordering law (right after each `run_hooks` call, in
//! invocation order, a missing trailing newline added — byte-identical
//! to the reference's in-run forwarding).

use mulch::hooks::HookExecution;

/// Forwards every execution's captured stderr to the process's stderr,
/// in invocation order — the reference writes each script's stderr
/// DURING the run; forwarding the collected bytes right after the
/// `run_hooks` call (before the command prints anything else) produces
/// the same byte stream.
pub(crate) fn forward_hook_stderr(executions: &[HookExecution]) {
    use std::io::Write as _;
    let mut stderr = std::io::stderr();
    for execution in executions {
        if execution.stderr.is_empty() {
            continue;
        }
        let _ = stderr.write_all(execution.stderr.as_bytes());
        if !execution.stderr.ends_with('\n') {
            let _ = stderr.write_all(b"\n");
        }
    }
}

/// Hook warnings ride stderr in plain mode only, with NO quiet gate
/// (the reference's unconditional `console.error` loop; `--json` never
/// carries them on this channel — the envelope owns that surface where
/// the reference has one).
#[allow(clippy::print_stderr, reason = "the warning channel is stderr")]
pub(crate) fn print_hook_warnings(warnings: &[String]) {
    for warning in warnings {
        eprintln!("Warning: {warning}");
    }
}
