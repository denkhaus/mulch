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
