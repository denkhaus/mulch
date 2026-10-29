# ADR-0007: Self-hosting cutover — this repo runs on its own binary

Date: 2026-10-29 · Status: accepted · Seeds: mulch-96ed (sprint 57) ·
Grilling: user-approved 2026-10-29 (all three recommended options)

## Context

Format core and CLI parity hold (mulch-0e39 closed at sprint 56: the
command surface mirrors ml 0.10.7 name-for-name, position-for-
position; every command has a differential battery). The bootstrap
phase still drives this repo's own `.mulch/` store through the
jayminwest npm CLI (`npm:@os-eco/mulch-cli` 0.10.7 pinned in
`.mise.toml`) — the tool builds expertise it does not eat.

## Decision

**The loop cuts over completely, repo-side, in sprint 57.**

1. **Scope**: everything in THIS repo lands in the sprint — `just
   install` (already present), everyday use of `mulch`, the ml-ref
   mechanism for the batteries, AGENTS.md/skill wording. The RUN-
   environment piece (the shared seeds-toolchain image gains the
   mulch binary additively) is cross-repo through denkhaus/seeds and
   escalates to the user as its own unassigned seed — it does not
   block the repo-side cutover.
2. **Reference mechanics (ml-ref)**: the differential battery reads
   the reference through a hardened discovery, replacing "first `ml`
   on PATH" (the recurring 0.11.0 hazard): (a) `MULCH_REFERENCE_ML`
   env var with an absolute path, (b) an `ml-ref` shim on PATH
   (`scripts/ml-ref`, resolving the pinned install via mise or its
   well-known directory), (c) a bare `ml` as last-resort
   compatibility fallback — and every hit is version-guarded: a
   reference that does not answer `--version` with `0.10.7` reads as
   absent (batteries skip loudly instead of comparing against the
   wrong binary).
3. **The npm pin leaves `.mise.toml`** (the seed's "bootstrap entry
   removal"). This is also a correctness requirement, not just
   hygiene: the npm package ships a `mulch` binary of its own, and
   mise's shims sit ahead of `~/.cargo/bin` on PATH — with the pin
   in place, `just install` would install our binary and the shell
   would still run the REFERENCE under the name `mulch`. After the
   removal, `mulch` resolves to the cargo-installed binary and the
   reference is reachable only through the ml-ref discovery.
4. **Dogfooding is immediate and full**: AGENTS.md, the iterate
   skill, and the session workflow call `mulch prime` / `mulch
   record` / `mulch search` from the cutover on; the repo's
   `.mulch/` store is maintained by this binary (the parity proof —
   the seeds-repo precedent, seeds-3791). Git remains the safety
   net: everything the loop's memory persists in is versioned; the
   ml-ref reference stays available as a forensic fallback.

## Consequences

- The session depends on our own binary for its memory — a broken
  release degrades the loop (the risk we accept deliberately; the
  battery suite gates every commit).
- The reference source path (`mise where npm:@os-eco/mulch-cli`)
  keeps working while the install remains on the machine, but the
  .mise.toml pin no longer provisions it on fresh checkouts:
  batteries without a reference skip loudly, `scripts/ml-ref`
  explains how to provision one.
- Run sandboxes keep the npm CLI until the seeds image ships the
  mulch binary (escalated seed, user-owned timing).
- Batteries no longer pass or fail on PATH luck: the version guard
  turns wrong-binary setups into visible skips.
