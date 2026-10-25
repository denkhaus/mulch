//! The CLI surface adapters over the lib hook engine (mulch-7f49,
//! ADR-0003): the engine never prints, so the channel policy and the
//! phase ritual live here — one owner for the working directory,
//! stderr forwarding, the `Warning:` lines, and the mutation-adoption
//! law every `pre-record` consumer shares.
//!
//! [`HookPhase::run`] is the whole ritual: resolve cwd → call the
//! engine → forward every execution's stderr, before the caller may
//! print a block error or a warning. Consumers then keep only their
//! payload shape, their channel gate, and their `Failure` rendering.

use std::path::PathBuf;

use mulch::hooks::{HookEvent, HookExecution, HookResult, run_hooks};
use serde_json::{Map, Value};

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
fn hook_cwd(command: &str) -> Result<PathBuf, Failure> {
    std::env::current_dir()
        .map_err(|source| Failure::handled(command, format!("resolving cwd: {source}")))
}

/// One finished hook phase: the engine's result plus the channel work
/// (`run` already forwarded stderr). Consumers read the resolved
/// outcome instead of re-implementing run → forward → block →
/// adopt.
#[derive(Debug)]
pub(crate) struct HookPhase(HookResult);

impl HookPhase {
    /// Runs one hook event: resolves the working directory, calls the
    /// engine, and forwards every execution's stderr — in that order
    /// (the reference writes each script's stderr during the run; the
    /// caller must not print a block error or a warning before this
    /// returns).
    ///
    /// `command` only labels a cwd failure (each command keeps its own
    /// handled-error shape).
    ///
    /// # Errors
    ///
    /// [`Failure`] when the process cwd is unreadable. A warn-only
    /// consumer (post-*) treats that as a skipped observation hook.
    pub(crate) fn run(
        event: HookEvent,
        payload: Value,
        store: &mulch::StoreFiles,
        command: &str,
    ) -> Result<Self, Failure> {
        let cwd = hook_cwd(command)?;
        let result = run_hooks(event, payload, store, &cwd);
        forward_hook_stderr(&result.executions);
        Ok(Self(result))
    }

    /// The block reason when the event blocked, `fallback` standing in
    /// for an engine result that carried none. Each caller renders its
    /// own `Failure` (the reference's four shapes are parity-pinned)
    /// and keeps its own fallback wording.
    pub(crate) fn blocked_reason<'a>(&'a self, fallback: &'a str) -> Option<&'a str> {
        self.0
            .blocked
            .then(|| self.0.block_reason.as_deref().unwrap_or(fallback))
    }

    /// Runs a warn-only event (post-*): an unresolvable cwd SKIPS the
    /// observation hook with a warning instead of failing the caller —
    /// the write already happened, and the reference's spawn failure
    /// is a warn-only hook failure too. Returns the warnings for the
    /// caller's channel.
    pub(crate) fn run_warn_only(
        event: HookEvent,
        payload: Value,
        store: &mulch::StoreFiles,
        command: &str,
    ) -> Vec<String> {
        match Self::run(event, payload, store, command) {
            Ok(phase) => phase.warnings().to_vec(),
            Err(_) => vec![format!(
                "{} hook skipped: the working directory is unavailable",
                event.key()
            )],
        }
    }

    /// The engine's warnings (warn-only failures and ignored
    /// mutations), in invocation order — the caller decides the
    /// channel.
    pub(crate) fn warnings(&self) -> &[String] {
        &self.0.warnings
    }

    /// The (possibly mutated) payload; input when no mutable hook
    /// changed it.
    pub(crate) fn payload(&self) -> &Value {
        &self.0.payload
    }

    /// Whether at least one script ran — the mutation gate. A phase
    /// with no executions must not adopt an unmutated payload.
    pub(crate) fn ran_any(&self) -> bool {
        self.0.ran_any
    }

    /// Reads a `pre-record` mutation into an adoption decision. The
    /// reference's guard is object IDENTITY (`mutated !== record`),
    /// always true for a parsed hook answer: an echo is adopted too —
    /// key order and all — so this never compares values.
    ///
    /// The adapter does NOT stamp the write-time id: the flag path
    /// stamps after adopting, the batch path leaves ids to the write.
    pub(crate) fn adopt_record(&self, registry: &mulch::Registry) -> Adopt {
        if !self.0.ran_any {
            return Adopt::Keep;
        }
        let Some(mutated) = truthy_payload_record(&self.0.payload) else {
            return Adopt::Keep;
        };
        if let mulch::schema::FullVerdict::Invalid { subs, .. } =
            mulch::schema::full_verdict(registry, mutated)
        {
            return Adopt::Invalid(invalid_mutation_text(&subs));
        }
        // A Valid verdict implies an object (the non-object arm is
        // Invalid); the fallback only guards the type refinement.
        mutated
            .as_object()
            .cloned()
            .map_or(Adopt::Keep, Adopt::Replace)
    }
}

/// A `pre-record` adoption decision.
#[derive(Debug)]
pub(crate) enum Adopt {
    /// No mutation (or an untruthy one): keep the record the caller
    /// built.
    Keep,
    /// The hook's record, validated — the caller writes this one
    /// (stamping the write-time id only where its path does).
    Replace(Map<String, Value>),
    /// The hook's answer failed the schema: the rendered reason for
    /// the caller's own error surface.
    Invalid(String),
}

/// The mutation gate's `payload.record` read (`payload?.record`):
/// JSON truthiness — absent, null, false, zero, and the empty string
/// never adopt; any other value (objects, arrays, `true`, non-empty
/// scalars) is adopted after validation.
fn truthy_payload_record(payload: &Value) -> Option<&Value> {
    let record = payload.get("record")?;
    let truthy = !matches!(record, Value::Null | Value::Bool(false))
        && !record.as_f64().is_some_and(|number| number == 0.0)
        && record.as_str() != Some("");
    truthy.then_some(record)
}

/// The invalid-mutation error text: the reference joins ajv's
/// `instancePath message` sub-errors with `"; "` — no type hint on
/// this surface (probe-pinned, ml 0.10.7).
fn invalid_mutation_text(subs: &[mulch::schema::SubError]) -> String {
    format!(
        "pre-record hook produced an invalid record: {}",
        mulch::schema::render_subs(subs).join(mulch::schema::SUB_SEP),
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// A phase around a hand-built engine result — the adapter needs
    /// no store for its decision table.
    fn phase(ran_any: bool, payload: Value) -> HookPhase {
        HookPhase(HookResult {
            ran_any,
            blocked: false,
            block_reason: None,
            warnings: Vec::new(),
            payload,
            executions: Vec::new(),
        })
    }

    fn registry() -> mulch::Registry {
        mulch::Registry::builtins()
    }

    #[test]
    fn adopt_keeps_a_record_without_a_mutation() {
        // No script ran → nothing to adopt, even with a payload.
        let untouched = phase(
            false,
            json!({"record": {"type": "pattern", "description": "d"}}),
        );
        assert!(matches!(untouched.adopt_record(&registry()), Adopt::Keep));
        // Scripts ran but the payload carries no truthy record.
        let absent = phase(true, json!({}));
        assert!(matches!(absent.adopt_record(&registry()), Adopt::Keep));
        let null = phase(true, json!({"record": null}));
        assert!(matches!(null.adopt_record(&registry()), Adopt::Keep));
    }

    #[test]
    fn adopt_replaces_with_a_valid_hook_record() {
        let hooked = phase(
            true,
            json!({"record": {
                "type": "pattern", "classification": "tactical",
                "recorded_at": "2026-10-25T10:00:00.000Z",
                "name": "n", "description": "d"
            }}),
        );
        match hooked.adopt_record(&registry()) {
            Adopt::Replace(record) => {
                assert_eq!(record.get("name").and_then(Value::as_str), Some("n"));
            }
            other => panic!("expected Replace, got {other:?}"),
        }
    }

    #[test]
    fn adopt_rejects_an_invalid_hook_record() {
        let broken = phase(true, json!({"record": {"type": "pattern"}}));
        match broken.adopt_record(&registry()) {
            Adopt::Invalid(reason) => {
                assert!(
                    reason.starts_with("pre-record hook produced an invalid record: "),
                    "unexpected text: {reason}"
                );
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn adopt_takes_an_echo_in_the_hooks_key_order() {
        // The reference's guard is identity, not equality: a hook that
        // re-emits the record is adopted, so the written line keeps
        // the hook's key order (serde_json preserves it).
        let echoed = phase(
            true,
            json!({"record": {
                "name": "n", "type": "pattern", "classification": "tactical",
                "recorded_at": "2026-10-25T10:00:00.000Z", "description": "d"
            }}),
        );
        match echoed.adopt_record(&registry()) {
            Adopt::Replace(record) => {
                let keys: Vec<&str> = record.keys().map(String::as_str).collect();
                assert_eq!(keys, [
                    "name",
                    "type",
                    "classification",
                    "recorded_at",
                    "description"
                ]);
            }
            other => panic!("expected Replace, got {other:?}"),
        }
    }
}
