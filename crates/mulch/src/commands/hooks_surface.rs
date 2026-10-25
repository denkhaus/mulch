//! The CLI surface adapters over the lib hook engine (mulch-7f49):
//! the engine never prints, so the channel policy lives here — one
//! owner for stderr forwarding, the `Warning:` lines, and the working
//! directory every consumer spawns its scripts in.

use std::path::PathBuf;

use mulch::hooks::HookExecution;

use crate::output::Failure;

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

/// Prints the engine's warnings as `Warning: …` lines on stderr — the
/// one owner of that format. The CALLER owns the channel policy: each
/// command decides whether the json envelope replaces this channel
/// (`!opts.json` for record/prime/prune/compact-apply; compact's
/// `--auto` gates on `--quiet` instead, like its own detail lines).
#[allow(clippy::print_stderr, reason = "the warning channel is stderr")]
pub(crate) fn print_hook_warnings(warnings: &[String]) {
    for warning in warnings {
        eprintln!("Warning: {warning}");
    }
}

/// The hooks' working directory: the reference spawns scripts in the
/// CLI's cwd, which may be a project subdirectory. ONE policy across
/// all consumers — a cwd failure is a handled error of the calling
/// command, never an empty path (an empty `current_dir` fails at
/// spawn, reads as a script failure, and would spuriously BLOCK a
/// `pre-*` event).
///
/// # Errors
///
/// [`Failure`] (handled, envelope-shaped) when the process cwd is
/// unreadable — reachable when the directory was removed under a
/// running process.
pub(crate) fn hook_cwd(command: &str) -> Result<PathBuf, Failure> {
    std::env::current_dir()
        .map_err(|source| Failure::handled(command, format!("resolving cwd: {source}")))
}
