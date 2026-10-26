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

use mulch::hooks::{HookEvent, HookExecution, HookResult, run_hooks};
use serde_json::{Map, Value};

use crate::output::Failure;

/// Forwards every execution's captured stderr to the process's stderr,
/// in invocation order — the reference writes each script's stderr
/// DURING the run; forwarding the collected bytes right after the
/// `run_hooks` call (before the command prints anything else) produces
/// the same byte stream.
fn forward_hook_stderr(executions: &[HookExecution]) {
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

/// One finished hook phase: the engine's result plus the channel work
/// (`run` already forwarded stderr). Consumers read the resolved
/// outcome instead of re-implementing run → forward → block →
/// adopt.
#[must_use = "a phase carries the block and adoption outcome; dropping it skips the gate"]
#[derive(Debug)]
pub(crate) struct HookPhase(HookResult);

impl HookPhase {
    /// Runs one hook event: resolves the working directory, calls the
    /// engine, and forwards every execution's stderr — in that order
    /// (the reference writes each script's stderr during the run; the
    /// caller must not print a block error or a warning before this
    /// returns).
    ///
    /// `command` only labels a cwd failure (the shared
    /// [`crate::commands::store::cwd`] law — the reference spawns
    /// scripts in the CLI's cwd, which may be a project subdirectory).
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
        let cwd = crate::commands::store::cwd(command)?;
        let result = run_hooks(event, payload, store, &cwd);
        forward_hook_stderr(&result.executions);
        Ok(Self(result))
    }

    /// The block reason when the event blocked, `fallback` standing in
    /// for an engine result that carried none. The result borrows
    /// either the engine's reason or `fallback` (the reference's `??`
    /// is nullish-only: an EMPTY reason still wins). Each caller
    /// renders its own `Failure` (the reference's five renderings are
    /// parity-pinned) and keeps its own fallback wording.
    pub(crate) fn blocked_reason<'a>(&'a self, fallback: &'a str) -> Option<&'a str> {
        self.0
            .blocked
            .then(|| self.0.block_reason.as_deref().unwrap_or(fallback))
    }

    /// Runs a warn-only event (post-*): an unresolvable cwd SKIPS the
    /// observation hook with a warning instead of failing the caller —
    /// the write already happened, and the reference's spawn failure
    /// is a warn-only hook failure too. Returns the warnings for the
    /// caller's channel; a skipped phase's warning carries the actual
    /// failure message, not a guessed cause.
    pub(crate) fn run_warn_only(
        event: HookEvent,
        payload: Value,
        store: &mulch::StoreFiles,
        command: &str,
    ) -> Vec<String> {
        match Self::run(event, payload, store, command) {
            Ok(phase) => phase.into_warnings(),
            Err(failure) => vec![format!("{} hook skipped: {}", event.key(), failure.message)],
        }
    }

    /// The engine's warnings (warn-only failures and ignored
    /// mutations), in invocation order — the caller decides the
    /// channel.
    pub(crate) fn warnings(&self) -> &[String] {
        &self.0.warnings
    }

    /// Consumes the phase for its warnings: the hand-off sites that
    /// OWN the warning list (record's report, the post-* loop) move it
    /// instead of cloning.
    pub(crate) fn into_warnings(self) -> Vec<String> {
        self.0.warnings
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
        let Some(mutated) = truthy_payload_field(&self.0.payload, "record") else {
            return Adopt::Keep;
        };
        if let mulch::schema::FullVerdict::Invalid { subs, .. } =
            mulch::schema::full_verdict(registry, mutated)
        {
            return Adopt::Invalid(invalid_hook_record_text("pre-record", &subs));
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
#[must_use = "an invalid adoption is a hook rejection; dropping it writes the wrong record"]
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

/// One hook-adoption gate's shared HALF (mulch-81ef): the truthy
/// payload field and the invalid-record message. The truthiness law
/// is [`mulch::jsval::truthy`] — both gates hand-rolled it before.
/// The LAW around a gate stays with its caller: pre-record adopts
/// verbatim, pre-compact backfills `supersedes` and re-stamps the id
/// (ADR-0003) — neither belongs here.
pub(crate) fn truthy_payload_field<'a>(payload: &'a Value, field: &str) -> Option<&'a Value> {
    let value = payload.get(field)?;
    mulch::jsval::truthy(value).then_some(value)
}

/// The invalid-adoption message: `<phase> hook produced an invalid
/// record: <subs>` — sub-errors joined with SUB_SEP (the reference's
/// pre-compact path spells the joiner `"; "`, byte-equal to SUB_SEP;
/// no type hint on either surface, probe-pinned ml 0.10.7).
pub(crate) fn invalid_hook_record_text(phase: &str, subs: &[mulch::schema::SubError]) -> String {
    format!(
        "{phase} hook produced an invalid record: {}",
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

    /// A blocked phase with an optional engine reason.
    fn blocked_phase(reason: Option<&str>) -> HookPhase {
        HookPhase(HookResult {
            ran_any:      true,
            blocked:      true,
            block_reason: reason.map(str::to_string),
            warnings:     Vec::new(),
            payload:      Value::Null,
            executions:   Vec::new(),
        })
    }

    fn registry() -> mulch::Registry {
        mulch::Registry::builtins()
    }

    #[test]
    fn blocked_reason_prefers_the_engine_reason_and_falls_back() {
        assert_eq!(phase(false, Value::Null).blocked_reason("fallback"), None);
        assert_eq!(
            blocked_phase(Some("boom")).blocked_reason("fallback"),
            Some("boom")
        );
        // The reference's `??` is nullish-only: an EMPTY reason is a
        // reason and beats the fallback.
        assert_eq!(blocked_phase(Some("")).blocked_reason("fallback"), Some(""));
        assert_eq!(
            blocked_phase(None).blocked_reason("fallback"),
            Some("fallback")
        );
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
        // The rest of the JS truthiness table: false, zero and the
        // empty string never adopt.
        for falsy in [json!(false), json!(0), json!("")] {
            let skipped = phase(true, json!({"record": falsy}));
            assert!(matches!(skipped.adopt_record(&registry()), Adopt::Keep));
        }
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
                // The adapter never stamps the write-time id: the flag
                // path stamps after adopting, batch leaves it to the
                // write.
                assert!(!record.contains_key("id"), "adopted record carries an id");
            }
            other => panic!("expected Replace, got {other:?}"),
        }
    }

    #[test]
    fn adopt_rejects_an_invalid_hook_record() {
        let broken = phase(true, json!({"record": {"type": "pattern"}}));
        match broken.adopt_record(&registry()) {
            Adopt::Invalid(reason) => {
                // The parity-pinned shape: ajv's sub-errors joined with
                // "; " after the fixed prefix.
                assert!(
                    reason.starts_with("pre-record hook produced an invalid record: "),
                    "unexpected text: {reason}"
                );
                assert!(reason.contains("must have required property"));
                assert!(reason.contains("; "), "sub-errors not joined: {reason}");
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
