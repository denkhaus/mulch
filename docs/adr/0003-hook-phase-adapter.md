# ADR-0003: One CLI hook-phase adapter over the lib engine

- Status: Accepted
- Date: 2026-10-25
- Deciders: user + local iterate loop (sprint 43, seed mulch-c7ca)
- Context: ADR-0002 put the hook engine in the lib and left it
  deliberately printless: it returns per-execution diagnostics and
  every caller owns forwarding. Sprint 42 then wired five consumers
  (record single + batch, prime, prune, compact `--auto` and
  `--apply`), which produced seven call sites that each re-implement
  the same ritual — `run_hooks` → forward stderr → blocked? →
  fallback reason → (for the mutable pre-record events) validate and
  adopt the hook's record. The channel half was already folded
  (`forward_hook_stderr`, `print_hook_warnings`, `hook_cwd`), but the
  ritual itself still lives per site, the mutation-adoption gate has
  TWO homes (`pre_record_phase` and the inline batch loop), and each
  site spells the cwd policy differently (`Result` in record,
  `unwrap_or_default` in prime/compact — an empty path that fails at
  spawn and would spuriously block a `pre-*` event — and
  `unwrap_or_else(".")` in prune).

## Decision

The CLI grows one adapter module, `commands/hooks_surface.rs`, which
owns the whole ritual:

```text
HookPhase::run(event, payload, store, command) -> Result<Self, Failure>
HookPhase::run_warn_only(event, payload, store, command) -> Vec<String>
HookPhase::blocked_reason(fallback: &str)       -> Option<&str>
HookPhase::warnings()                           -> &[String]
HookPhase::payload()                            -> &Value
HookPhase::ran_any()                            -> bool
HookPhase::adopt_record(registry)               -> Adopt
Adopt = Keep | Replace(Map<String, Value>) | Invalid(String)
```

`run` resolves the working directory (one policy, its failure is a
handled `Failure` of the calling command), calls the engine, and
forwards every execution's stderr itself before returning. The
consumers keep exactly two things: their payload shape and their
channel gate (plain-vs-json warnings, batch's `Record {i}:` prefix,
compact's group-warning line), plus their own `Failure` rendering.

`run_warn_only` is the post-* half of the same ritual: a warn-only
event never fails the parent command, so an unresolvable cwd becomes
a `"<event> hook skipped: …"` warning instead of a `Failure` (the
write already happened). This also closes a divergence the fold
exposed — the batch path propagated a cwd `Failure` where the flag
path warned.

The adoption law (`truthy_payload_record`, the identity guard, the
validity verdict, the joined sub-error text) moves into the adapter;
`adopt_record` is the ONE reader of a `pre-record` mutation. The
adapter never stamps the write-time id — the flag path calls
`ensure_write_id` after adopting, the batch path does not — and never
prints a `Failure` for a block: it returns the reason, the caller
renders. Compact's `payload.replacement` law (supersedes backfill,
id regeneration) is a different gate and stays in `compact.rs`.

## Rationale

- ADR-0002's boundary is "the lib never prints". Moving the channel
  policy into a CLI module keeps that boundary exactly as it is; the
  engine stays embeddable and printless.
- The ritual is one law with one ordering rule (stderr before the
  block error, before the warnings). Seven copies mean seven chances
  to reorder it.
- The adoption gate is the sharpest case: change the guard, the
  verdict call, or the invalid-mutation text once and you must
  remember the second copy. Sprint 42's spec review found exactly
  that class of divergence (identity vs value-equality adoption); one
  owner makes the next such change structural instead of cultural.
- The deliberate divergence between the two record paths (flag
  stamps an id, batch leaves ids to write time) stays visible at the
  call site rather than disappearing behind a policy parameter the
  adapter has no business knowing.

## Alternatives considered

- Consumer-side forwarding kept (status quo): every call site stays
  responsible for the ordering rule and the cwd spelling — rejected
  (that is the duplication this ADR removes).
- A stamping policy parameter (`StampPolicy::Flag | Batch`): one
  place to read, but the adapter then knows write-phase details of
  one command; rejected (user decision 2026-10-25).
- A `Failure` factory (closure) inside the adapter: shorter call
  sites, but three commands need three different failure shapes
  (record's two-channel envelope, prime/prune's plain
  `handled_on_stderr`, compact's json-gated `compact_failure`);
  rejected in favour of returning the bare reason.
- Move the adoption law into the lib next to `full_verdict`: it is a
  CLI-phase law (it reads a `HookResult`), and the lib would need a
  hook-aware API to host it — out of scope for this deepening.

## Consequences

- `hook_cwd` becomes private to the adapter; no command resolves a
  hook cwd by itself any more.
- A new hook consumer costs one payload plus a channel gate, not a
  copy of the ritual.
- `HookPhase` is constructible in unit tests from a `HookResult`, so
  the adoption table (keep / replace / invalid / echo) is testable
  without a store or a CLI run.
- Post-* consumers (warn-only) treat `run`'s `Result` as a diagnostic:
  an unresolvable cwd skips the observation hook with a warning, it
  never fails a command whose write already happened.
