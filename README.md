# mulch

Native Rust implementation of the [mulch](https://github.com/jayminwest/mulch)
structured-expertise format (the `ml` CLI).

**Format compatibility promise:** read+write drop-in compatible with
`@os-eco/mulch-cli` 0.10.7 (`.mulch/` directory: `mulch.config.yaml`,
`expertise/<domain>.jsonl` — one JSONL per domain — plus the soft-archive
store `archive/`). Unknown record fields are preserved on every write;
additive fields are the only sanctioned extension mechanism. The CLI
surface (`mulch init/add/record/edit/query/setup/prime/onboard/status/
validate/prune/archive/restore/search/rank/outcome/doctor/ready/sync/
delete/delete-domain/move/learn/compact/config/diff/upgrade/completions/
audit`) mirrors the reference tool until this repo's own line replaces it
(self-hosting cutover).

Freeze policy (ADR-0023 in denkhaus/fabro): no upstream following. The
format is frozen on our side; we extend additively for our own reasons.
The round-trip suite stays runnable against future upstream releases as
an alarm, never as an obligation.

Phase scope beyond parity: the canonical-domain layer — alias resolution
for fragmented domain names (develop/dev-loop/develop-loop/devloop/…)
— moves to this repo per ADR-0023; it lands as an additive resolution
step, never a destructive merge of existing domains.

Format credit: [jayminwest/mulch](https://github.com/jayminwest/mulch) —
this repository is an independent implementation of that format, not a
fork. The rewrite target's CLI is published as `@os-eco/mulch-cli`
(the `ml` binary).

## DEVIATIONS

Deliberate divergences from the reference `ml` 0.10.7, each with its own
expectation (the parity rule: never silently):

- **Crash paths render as clean errors.** The reference exits 1 with a
  raw Bun stack trace (no envelope) for (a) `.mulch/` without
  `mulch.config.yaml`, (b) `validate` with no `.mulch/` at all, and
  (c) malformed JSONL lines in `doctor`. We render the reference's own
  message as a handled error envelope (status-channel) or report the
  bad lines as `jsonl-integrity` findings. Expectation: same exit code
  1, clean text instead of a stack trace.
- **`status` reports instead of crashing on a malformed or
  unknown-type line** (the reference exits 1 with a runtime stack trace
  and prints nothing): the reporting commands keep the per-line view,
  the mutating commands abort (strict read).
- **Malformed-line reasons carry serde's wording** (V8/JSC phrases
  differ); the `Malformed JSONL at <path>:<line>: <reason>. Line:
  <preview>` template and the 80-char preview are byte-identical.
- **`move` reports inbound references in directory order**, like the
  reference's `readdir` walk (both are filesystem order; a lexically
  sorted list is not guaranteed).
- **The mutating commands read records strictly** (reference
  `readExpertiseFile`): malformed lines and unregistered types abort
  with the reference's messages, the legacy singular `outcome` field is
  normalized to `outcomes`, and rewrites re-serialize records compactly
  (ids are generated for id-less survivors).
- **Argument-parse errors use clap wording** (choice rejections,
  missing-argument hints): same channels and exit code 1, different
  text than commander's. The no-store paths of `add`, `record`,
  `edit`, `outcome` and the invalid-domain `add` render as clean
  handled errors where the reference crashes or prints stack traces —
  same family as the crash-path bullet below.
- **`move` appends a newline when the target file lacks a trailing
  one.** The reference concatenates (`appendFile`), merging the moved
  record onto the last line — a corrupt store. Expectation: the moved
  record starts on its own line.
- **Config rewrites normalize nested sequences to serde_yaml's
  indentation** (`- pattern` indented 4 spaces under a 4-space key
  level; the reference's js-yaml emits 6). Scalar fields, key order,
  domain order, and the governance/shelf-life backfill match the
  reference byte-for-byte.
- **`delete-domain` cancels on EOF** (closed stdin) instead of blocking
  forever; `--yes` and `--json` behave like the reference.
- **`--format` does not exist yet.** It belongs to the
  record-rendering commands (`query`, `prime`, `search`, …) and joins
  with its parity slice (`mulch-16da`). Per-command `--help` wording
  also differs (clap renderer; the reference reuses its root template
  with the command name swapped in) — help text lists only
  implemented commands either way.
- **Seven doctor checks evaluate as unconditional pass** on clean
  stores only: `duplicates`, `orphaned-domains`, `file-anchors`,
  `governance`, `domain-violations`, `domain-rules-compatibility`,
  `decay-config`. Their warn/fail semantics (real duplicates, orphaned
  files, governance overruns) land with the later CLI-parity slices;
  until then they hold the reference's clean-store output shape.
- **`doctor` upgrade check is inert.** The reference warns when the npm
  registry offers a newer version. The native binary has no npm
  registry; the `upgrade` check reports pass with a cargo hint.
  Expectation: check name and order stay, status does not.
- **`--version` reports this binary's version**, not the reference's.
- **`doctor --fix` removes malformed JSONL lines** instead of crashing
  (the reference crashes on them before fixing anything); stale records
  are pruned, schema-invalid and malformed records removed — all
  hard-deleted, the domain file left empty, the domain stays
  registered. Expectation: fixes print as `Pruned`/`Removed` lines and
  the exit code stays the PRE-fix check result.

## Status

Bootstrap. The work is tracked in this repo's own `.seeds/` tracker
(denkhaus/seeds format): format core + round-trip suite, CLI parity,
and the self-hosting cutover (the `mulch` binary replacing `ml` for this
repo's own `.mulch/` store) are the bootstrap seeds. The develop loop
runs on the fabro platform against `origin/main` — no upstream mirror.

## Toolchain

This repo deliberately owns NO toolchain image (reuse is key): runs
execute on the server-managed `mulch-toolchain` environment pinning the
shared `ghcr.io/denkhaus/seeds-toolchain:<sha12>` image (owned and built
by denkhaus/seeds). Environment-level changes land in the seeds repo.

## Build & test

```
cargo build --workspace
cargo nextest run --workspace
cargo +nightly-2026-09-22 fmt --check --all
cargo +nightly-2026-09-22 clippy --workspace --all-targets -- -D warnings
```

License: MIT OR Apache-2.0.
