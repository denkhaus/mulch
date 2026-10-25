# ADR-0002: The lifecycle hook engine lives in the lib

- Status: Accepted
- Date: 2026-10-25
- Deciders: user + local iterate loop (sprint 42, seed mulch-7f49)
- Context: mulch parity work reached the lifecycle hook surface (the
  reference `ml` 0.10.7 `hooks.ts`: five events — pre-record,
  post-record, pre-prime, pre-prune, pre-compact — driven by the
  store config, `sh -c` execution, `{event, payload}` JSON on stdin,
  `MULCH_HOOK=1`, process-group SIGKILL on timeout, stdout-JSON
  payload mutation for three events, block-on-non-zero for the four
  pre-* events, warn-only post-*).

## Decision

The hook engine is a lib module (`crates/mulch/src/hooks.rs`), not a
CLI-side module. The lib therefore gains process-spawning behavior
and a direct `libc` dependency (killpg for the timeout kill).

## Rationale

- The hook law is a format-level contract: the events, their
  mutability and blocking semantics, and the `hooks` /
  `hook_settings` config keys are part of the store format the repo
  promises to hold (README: read+write drop-in compatibility). Laws
  live in the lib — the same reasoning as anchors (mulch-5a3d) and
  the decay core.
- fabro embeds the lib as its native expertise engine and writes
  records through it; a repo whose config declares `pre-record`
  hooks must fire them for embedders too, or the embedded writer
  silently bypasses the project's lifecycle contract.
- One owner: the CLI commands map `HookResult` onto their surfaces
  (error envelopes, `Warning:` lines), exactly the commands-render /
  lib-decides split the README posture names.

## Consequences

- The lib spawns processes. The engine never prints: hook stderr is
  returned per execution and forwarded by the caller, so embedders
  (fabro) stay in control of diagnostics passthrough. Byte-order of
  forwarded stderr is preserved by forwarding right after each
  `run_hooks` call, before the command prints anything else.
- `libc` joins the lib's direct dependencies — used only for
  `kill(-pid, SIGKILL)` (process-group kill). Plain `Child::kill`
  would only reach the immediate `sh`, leaving detached descendants
  holding the pipes open (the reference's own R-02 stress finding);
  `std` has no group kill.
- The engine is synchronous (poll + try_wait loop with a sleep),
  matching the CLI's sync architecture; no async runtime enters the
  lib for this.
- Non-JSON stdout warnings carry serde's parse wording where the
  reference carries V8's — the same documented deviation class as
  the malformed-JSONL reasons (README DEVIATIONS).
- Miri is not applicable to the sanctioned `unsafe` block: the
  module's exercised path is `Command::spawn` / process-group
  signalling, which Miri does not emulate — the block is covered by
  the timeout tests (a real `sh` child in a real process group)
  instead of an interpreter run.

## Alternatives considered

- CLI-side engine: keeps the lib process-free, but splits the
  config law across layers and leaves fabro without hooks — rejected
  (user decision 2026-10-25).
- std-only child kill (no libc): smaller dep footprint, but a
  detached descendant survives the timeout and hangs the pipes — a
  real behavioral divergence on the timeout path — rejected (user
  decision 2026-10-25).
