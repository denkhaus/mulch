//! The lifecycle hook engine (mulch-7f49, ADR-0002): the
//! config-driven shell scripts a store's `hooks` block schedules
//! around a command's load→write→render cycle.
//!
//! One owner for the law the reference implements in
//! `utils/hooks.ts`: five events (`pre-record`, `post-record`,
//! `pre-prime`, `pre-prune`, `pre-compact`), each an ordered list of
//! `sh -c` commands run in `cwd` with `MULCH_HOOK=1` and the current
//! payload as compact JSON (`{"event":…,"payload":…}`) on stdin.
//! The four `pre-*` events BLOCK on a non-zero exit (or timeout);
//! `post-record` only warns. `pre-record`, `pre-prime` and
//! `pre-compact` are MUTABLE — a hook's stdout JSON replaces the
//! payload for the next script (a `{ "payload": … }` wrapper
//! unwraps); `pre-prune` is block-or-allow only. A hook that
//! overruns `hook_settings.timeout_ms` (default 5 s) is SIGKILLed
//! with its whole process group and counts as exit 124.
//!
//! The engine never prints: hook stderr is returned per execution
//! and forwarded by the CALLER (right after each `run_hooks` call,
//! before the command prints anything else — byte-order identical to
//! the reference's in-run forwarding, and embedders stay in control
//! of diagnostics passthrough). The crate's unsafe policy: one
//! sanctioned exception (ADR-0002), the process-group kill below,
//! isolated behind a safe wrapper.

use std::io::Write as _;
use std::os::unix::process::CommandExt as _;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::config::Config;
use crate::store_files::StoreFiles;

/// The reference's `DEFAULT_HOOK_TIMEOUT_MS` (schemas/config.ts).
pub const DEFAULT_HOOK_TIMEOUT_MS: u64 = 5_000;

/// Poll granularity of the timeout watchdog (the reference's timer
/// is event-driven; 5 ms is below any observable hook duration).
const WATCHDOG_POLL_MS: u64 = 5;

/// A lifecycle event a store's `hooks` block can schedule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HookEvent {
    /// Before each record write (mutable, blocking).
    PreRecord,
    /// After each actual record write (warn-only).
    PostRecord,
    /// Before prime renders (mutable, blocking).
    PrePrime,
    /// Between prune's preview and write phases (block-or-allow).
    PrePrune,
    /// Before a compaction merge (mutable, blocking).
    PreCompact,
}

impl HookEvent {
    /// The config key under `hooks:` (the reference's spelling).
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Self::PreRecord => "pre-record",
            Self::PostRecord => "post-record",
            Self::PrePrime => "pre-prime",
            Self::PrePrune => "pre-prune",
            Self::PreCompact => "pre-compact",
        }
    }

    /// Whether a hook's stdout JSON may replace the payload
    /// (`MUTABLE_EVENTS`; `pre-prune` is deliberately absent).
    #[must_use]
    pub fn is_mutable(self) -> bool {
        matches!(self, Self::PreRecord | Self::PrePrime | Self::PreCompact)
    }

    /// Whether a non-zero exit (or timeout) blocks the parent
    /// command (`BLOCKING_EVENTS` — every `pre-*` event).
    #[must_use]
    pub fn is_blocking(self) -> bool {
        !matches!(self, Self::PostRecord)
    }
}

/// One script invocation's diagnostics, in invocation order.
#[derive(Clone, Debug)]
pub struct HookExecution {
    /// The `sh -c` command as configured.
    pub command:     String,
    /// The exit code; 124 marks a timeout kill.
    pub exit_code:   i32,
    /// Collected stdout (raw).
    pub stdout:      String,
    /// Collected stderr (raw) — the caller forwards it.
    pub stderr:      String,
    /// Wall-clock runtime in milliseconds.
    pub duration_ms: u64,
    /// Whether the timeout killed the process group.
    pub timed_out:   bool,
}

/// The engine's outcome for one event.
#[derive(Clone, Debug)]
pub struct HookResult {
    /// Whether at least one script ran.
    pub ran_any:      bool,
    /// A blocking event saw a non-zero exit (or timeout); the caller
    /// MUST abort the parent command. Later scripts were skipped.
    pub blocked:      bool,
    /// The human-readable block reason (script + exit code/timeout).
    pub block_reason: Option<String>,
    /// Warnings from warn-only failures and ignored mutations.
    pub warnings:     Vec<String>,
    /// The (possibly mutated) payload — equal to the input when no
    /// mutable hook changed it. A hook printing `null` or a non-
    /// object replaces the payload with that value; consumers'
    /// `payload.<field>` reads then see nothing, like `?.` in the
    /// reference.
    pub payload:      Value,
    /// Per-script diagnostics in invocation order.
    pub executions:   Vec<HookExecution>,
}

/// SIGKILL to a whole process group (no safe std API exists).
///
/// The crate's unsafe policy (ADR-0002, mirroring the style guide):
/// project-written unsafe is banned workspace-wide; this call is the
/// one sanctioned exception, isolated behind this safe wrapper. The
/// reference kills `-pid` too — killing only the immediate `sh`
/// would leave a detached descendant (`sleep 30 & wait`) holding the
/// output pipes open (the reference's own R-02 stress finding).
// The narrow override of the workspace-wide deny: this function only.
#[allow(
    unsafe_code,
    reason = "the sanctioned ADR-0002 exception: no safe API kills a process group"
)]
fn kill_process_group(pgid: u32) {
    let Ok(pgid) = i32::try_from(pgid) else {
        return;
    };
    // SAFETY: `libc::kill` only reads its integer arguments; the pid
    // is the child's own process group id (assigned at spawn via
    // `process_group(0)`), and ESRCH for an already-dead group is
    // ignored — no invariant beyond the valid SIGKILL constant.
    let _ = unsafe { libc::kill(-pgid, libc::SIGKILL) };
}

/// The reference's `getHookList`: the event's scripts in declaration
/// order — non-string entries and blank strings dropped, the
/// original (untrimmed) string kept otherwise; a non-list value
/// reads as no hooks.
fn hook_scripts(config: &Config, event: HookEvent) -> Vec<String> {
    config
        .extra("hooks")
        .and_then(|hooks| hooks.get(event.key()))
        .and_then(serde_yaml::Value::as_sequence)
        .map_or_else(Vec::new, |scripts| {
            scripts
                .iter()
                .filter(|script| script.as_str().is_some_and(|raw| !raw.trim().is_empty()))
                .filter_map(|script| script.as_str().map(str::to_string))
                .collect()
        })
}

/// The reference's `resolveSettings`: `hook_settings.timeout_ms`
/// when a positive number, else the 5 s default.
fn hook_timeout(config: &Config) -> u64 {
    config
        .extra("hook_settings")
        .and_then(|settings| settings.get("timeout_ms"))
        .and_then(serde_yaml::Value::as_f64)
        .filter(|ms| *ms > 0.0)
        // Fractional milliseconds are legal in the reference's
        // setTimeout; the caller works in whole milliseconds, and an
        // out-of-range value falls back to the default.
        .map_or(DEFAULT_HOOK_TIMEOUT_MS, |ms| {
            Duration::try_from_secs_f64(ms / 1_000.0)
                .ok()
                .and_then(|timeout| u64::try_from(timeout.as_millis()).ok())
                .unwrap_or(DEFAULT_HOOK_TIMEOUT_MS)
        })
}

/// One `sh -c` run: payload JSON on stdin (closed after), stdout and
/// stderr collected, the process group SIGKILLed at the deadline.
fn run_one(command: &str, stdin_json: &str, cwd: &Path, timeout_ms: u64) -> HookExecution {
    let started = Instant::now();
    let child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(cwd)
        .env("MULCH_HOOK", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn();

    // The reference's `error` event resolves to exit code 1 with no
    // output — a missing `sh` behaves like a failed script.
    let Ok(mut child) = child else {
        return HookExecution {
            command:     command.to_string(),
            exit_code:   1,
            stdout:      String::new(),
            stderr:      String::new(),
            duration_ms: elapsed_ms(started),
            timed_out:   false,
        };
    };

    let stdin = child.stdin.take();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    // The script may exit without reading stdin — write in a thread
    // and ignore a broken pipe (the reference catches the same).
    let stdin_json = stdin_json.to_string();
    let stdin_thread = std::thread::spawn(move || {
        if let Some(mut stdin) = stdin {
            let _ = stdin.write_all(stdin_json.as_bytes());
        }
    });
    let stdout_thread = drain_pipe(stdout);
    let stderr_thread = drain_pipe(stderr);

    let (exit_code, timed_out) = wait_with_deadline(&mut child, timeout_ms);

    let _ = stdin_thread.join();
    let stdout = stdout_thread.join().unwrap_or_default();
    let stderr = stderr_thread.join().unwrap_or_default();

    HookExecution {
        command: command.to_string(),
        exit_code,
        stdout,
        stderr,
        duration_ms: elapsed_ms(started),
        timed_out,
    }
}

/// Whole milliseconds since `started` (saturating — a run cannot
/// exceed `u64::MAX` ms).
fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Drains a child pipe on its own thread (a full pipe would
/// deadlock the run otherwise), lossy like the reference's Buffer
/// string conversion.
fn drain_pipe(
    pipe: Option<impl std::io::Read + Send + 'static>,
) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

/// Polls the child until it exits or the deadline passes; on the
/// deadline the whole process group is SIGKILLed (and reaped) and
/// the run counts as exit 124.
fn wait_with_deadline(child: &mut Child, timeout_ms: u64) -> (i32, bool) {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return (status.code().unwrap_or(0), false),
            Ok(None) => {
                if Instant::now() >= deadline {
                    kill_process_group(child.id());
                    // Reap the killed group leader; a signal death has
                    // no exit code, but the timeout's 124 stands.
                    let _ = child.wait();
                    return (124, true);
                }
                std::thread::sleep(Duration::from_millis(WATCHDOG_POLL_MS));
            }
            // A failed wait mirrors the reference's error event.
            Err(_) => return (1, false),
        }
    }
}

/// Runs every script the store's `hooks` block declares for `event`,
/// in declaration order, threading the payload: each script reads
/// `{"event":…,"payload":…}` on stdin; a MUTABLE event's script may
/// answer with JSON on stdout that replaces the payload for the next
/// script (a `{"payload":…}` wrapper unwraps; empty stdout keeps it;
/// non-JSON keeps it and warns). A BLOCKING event's non-zero exit
/// (or timeout) stops the run and reports [`HookResult::blocked`] —
/// the caller must abort the parent command; later scripts are
/// skipped. `post-record` failures only warn and every script runs.
///
/// Hook stderr is collected per execution, never printed here — the
/// caller forwards it (see the module docs for the ordering law).
/// The config comes from the store; the scripts run in `cwd` (the
/// reference spawns in the CLI's working directory, which may be a
/// project subdirectory).
#[must_use]
pub fn run_hooks(event: HookEvent, payload: Value, store: &StoreFiles, cwd: &Path) -> HookResult {
    let config = store.config();
    let scripts = hook_scripts(config, event);
    if scripts.is_empty() {
        return HookResult {
            ran_any: false,
            blocked: false,
            block_reason: None,
            warnings: Vec::new(),
            payload,
            executions: Vec::new(),
        };
    }

    let timeout_ms = hook_timeout(config);
    let mutable = event.is_mutable();
    let blocking = event.is_blocking();

    let mut warnings = Vec::new();
    let mut executions = Vec::new();
    let mut current_payload = payload;

    for command in scripts {
        let stdin_json = serde_json::to_string(&serde_json::json!({
            "event": event.key(),
            "payload": current_payload,
        }))
        .unwrap_or_default();
        let execution = run_one(&command, &stdin_json, cwd, timeout_ms);

        if execution.exit_code != 0 {
            let reason = if execution.timed_out {
                format!(
                    "hook `{command}` timed out after {timeout_ms}ms",
                    command = execution.command,
                )
            } else {
                format!(
                    "hook `{command}` exited with code {code}",
                    command = execution.command,
                    code = execution.exit_code,
                )
            };
            executions.push(execution);
            if blocking {
                return HookResult {
                    ran_any: true,
                    blocked: true,
                    block_reason: Some(reason),
                    warnings,
                    payload: current_payload,
                    executions,
                };
            }
            warnings.push(reason);
            continue;
        }

        if mutable {
            let trimmed = execution.stdout.trim();
            if !trimmed.is_empty() {
                match serde_json::from_str::<Value>(trimmed) {
                    // A `{"payload": …}` wrapper unwraps (the
                    // reference's `"payload" in parsed`); any other
                    // JSON — object, array, scalar, null — replaces
                    // the payload whole.
                    Ok(parsed) => match parsed {
                        Value::Object(object) if object.contains_key("payload") => {
                            current_payload = object["payload"].clone();
                        }
                        other => current_payload = other,
                    },
                    Err(error) => {
                        // DEVIATION (README): the reason carries
                        // serde's wording where the reference carries
                        // V8's — the same documented class as the
                        // malformed-JSONL reasons.
                        warnings.push(format!(
                            "hook `{command}` printed non-JSON on stdout \
                             (mutation ignored): {error}",
                            command = execution.command,
                        ));
                    }
                }
            }
        }
        executions.push(execution);
    }

    HookResult {
        ran_any: true,
        blocked: false,
        block_reason: None,
        warnings,
        payload: current_payload,
        executions,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::json;

    use super::*;
    use crate::StoreLocation;

    /// A scratch store with one configured domain and a raw config
    /// body (the hooks/hook_settings blocks under test).
    fn hook_store(tag: &str, extra_config: &str) -> (PathBuf, StoreFiles) {
        let root = std::env::temp_dir().join(format!("mulch-hooks-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(root.join(".mulch")).expect("temp dir");
        std::fs::write(
            root.join(".mulch").join("mulch.config.yaml"),
            format!("version: '1'\ndomains:\n  dev: {{}}\n{extra_config}"),
        )
        .expect("writable config");
        match StoreFiles::locate(&root).expect("store opens") {
            StoreLocation::Open(store) => (root, store),
            other => panic!("expected an open store, got {other:?}"),
        }
    }

    /// A command that writes its stdin and the MULCH_HOOK flag to
    /// files next to the store (paths the test can read back). No
    /// single quotes — the YAML list entries are single-quoted.
    fn probe_command(stdin_path: &Path, flag_path: &Path) -> String {
        format!(
            "cat > {}; printf %s \"$MULCH_HOOK\" > {}",
            stdin_path.display(),
            flag_path.display(),
        )
    }

    #[test]
    fn no_hooks_is_a_passthrough() {
        let (root, store) = hook_store("none", "");
        let result = run_hooks(HookEvent::PreRecord, json!({"a": 1}), &store, &root);
        assert!(!result.ran_any);
        assert!(!result.blocked);
        assert_eq!(result.payload, json!({"a": 1}));
        assert!(result.executions.is_empty());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn stdin_carries_event_and_payload_with_env_flag() {
        let stdin_path =
            std::env::temp_dir().join(format!("mulch-hooks-stdin-{}.json", std::process::id()));
        let flag_path =
            std::env::temp_dir().join(format!("mulch-hooks-flag-{}", std::process::id()));
        let (root, store) = hook_store(
            "stdin",
            &format!(
                "hooks:\n  pre-record:\n    - '{}'\n",
                probe_command(&stdin_path, &flag_path),
            ),
        );
        let result = run_hooks(HookEvent::PreRecord, json!({"a": 1}), &store, &root);
        assert!(result.ran_any);
        assert!(!result.blocked);
        // Compact JSON, exactly JSON.stringify's shape, and the flag
        // env var the hooks contract names.
        assert_eq!(
            std::fs::read_to_string(&stdin_path).expect("stdin captured"),
            r#"{"event":"pre-record","payload":{"a":1}}"#
        );
        assert_eq!(
            std::fs::read_to_string(&flag_path).expect("flag captured"),
            "1"
        );
        std::fs::remove_dir_all(&root).ok();
        std::fs::remove_file(&stdin_path).ok();
        std::fs::remove_file(&flag_path).ok();
    }

    #[test]
    fn mutable_chain_threads_and_unwraps_the_payload_wrapper() {
        let (root, store) = hook_store(
            "chain",
            "hooks:\n  pre-record:\n    - 'printf %s \"{\\\"payload\\\":{\\\"a\\\":2}}\"'\n    - 'printf %s \"{\\\"b\\\":3}\"'\n",
        );
        let result = run_hooks(HookEvent::PreRecord, json!({"a": 1}), &store, &root);
        assert_eq!(result.payload, json!({"b": 3}));
        assert_eq!(result.executions.len(), 2);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn bare_json_replaces_the_payload_whole() {
        let (root, store) = hook_store("bare", "hooks:\n  pre-prime:\n    - 'printf %s null'\n");
        let result = run_hooks(HookEvent::PrePrime, json!({"domains": []}), &store, &root);
        // null replaces the payload; consumers' field reads then see
        // nothing (the reference's `payload?.domains` on null).
        assert_eq!(result.payload, serde_json::Value::Null);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn empty_stdout_keeps_the_payload() {
        let (root, store) = hook_store(
            "empty",
            "hooks:\n  pre-record:\n    - 'true'\n    - 'printf %s \"  \"'\n",
        );
        let result = run_hooks(HookEvent::PreRecord, json!({"a": 1}), &store, &root);
        assert_eq!(result.payload, json!({"a": 1}));
        assert_eq!(result.warnings, Vec::<String>::new());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn non_json_stdout_warns_and_keeps_the_payload() {
        let (root, store) = hook_store(
            "nonjson",
            "hooks:\n  pre-record:\n    - 'printf %s not-json'\n",
        );
        let result = run_hooks(HookEvent::PreRecord, json!({"a": 1}), &store, &root);
        assert_eq!(result.payload, json!({"a": 1}));
        assert_eq!(result.warnings.len(), 1);
        // The prefix is the law; the reason text carries serde's
        // wording (DEVIATIONS: V8 phrases differ) — not pinned.
        assert!(
            result.warnings[0].starts_with(
                "hook `printf %s not-json` printed non-JSON on stdout \
                     (mutation ignored): "
            ),
            "warning was: {}",
            result.warnings[0]
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn blocking_event_stops_at_the_first_failure() {
        let (root, store) = hook_store(
            "block",
            "hooks:\n  pre-record:\n    - 'exit 3'\n    - 'printf %s \"{}\"'\n",
        );
        let result = run_hooks(HookEvent::PreRecord, json!({"a": 1}), &store, &root);
        assert!(result.blocked);
        assert_eq!(
            result.block_reason.as_deref(),
            Some("hook `exit 3` exited with code 3")
        );
        // The later script never ran.
        assert_eq!(result.executions.len(), 1);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn post_record_failures_only_warn_and_all_run() {
        let (root, store) = hook_store(
            "post",
            "hooks:\n  post-record:\n    - 'exit 5'\n    - 'exit 0'\n",
        );
        let result = run_hooks(HookEvent::PostRecord, json!({}), &store, &root);
        assert!(!result.blocked);
        assert_eq!(result.warnings, vec!["hook `exit 5` exited with code 5"]);
        assert_eq!(result.executions.len(), 2);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn timeout_kills_the_group_and_reports_124() {
        // The detached descendant is the point: without the group
        // kill this run would hang for the full sleep.
        let (root, store) = hook_store(
            "timeout",
            "hooks:\n  pre-record:\n    - 'sleep 30 & wait'\nhook_settings:\n  \
             timeout_ms: 150\n",
        );
        let started = Instant::now();
        let result = run_hooks(HookEvent::PreRecord, json!({}), &store, &root);
        assert!(result.blocked);
        assert_eq!(
            result.block_reason.as_deref(),
            Some("hook `sleep 30 & wait` timed out after 150ms")
        );
        let execution = &result.executions[0];
        assert_eq!(execution.exit_code, 124);
        assert!(execution.timed_out);
        assert!(execution.duration_ms >= 150);
        // The group kill must have worked: far below the 30 s sleep
        // (and generous enough for a loaded machine).
        assert!(started.elapsed().as_secs() < 10);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn blank_and_non_string_entries_are_dropped() {
        let (root, store) = hook_store(
            "filter",
            "hooks:\n  pre-record:\n    - ''\n    - '   '\n    - 42\n    - 'printf %s \
             \"{\\\"payload\\\":{\\\"kept\\\":true}}\"'\n",
        );
        let result = run_hooks(HookEvent::PreRecord, json!({}), &store, &root);
        assert!(result.ran_any);
        assert_eq!(result.executions.len(), 1);
        assert_eq!(result.payload, json!({"kept": true}));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn stderr_is_captured_per_execution() {
        let (root, store) = hook_store(
            "stderr",
            "hooks:\n  post-record:\n    - 'printf boomed >&2; exit 1'\n",
        );
        let result = run_hooks(HookEvent::PostRecord, json!({}), &store, &root);
        assert_eq!(result.executions[0].stderr, "boomed");
        // The engine never prints — forwarding is the caller's law.
        std::fs::remove_dir_all(&root).ok();
    }
}
