# ADR-0001: Bootstrap posture — shared toolchain image and GitButler landing flow

- Status: Accepted
- Date: 2026-10-03
- Deciders: user (grilling rounds 1-2, 2026-10-03), agent (bootstrap execution)
- Related: ADR-0023 in denkhaus/fabro (parent record: native seeds & mulch), AGENTS.md (version control, push gate)

## Context

This repo was bootstrapped from the seeds pattern per ADR-0023. Two
structural decisions taken at bootstrap (2026-10-03, user-confirmed)
shape every day of work here and are not covered by the parent record.

## Decision

1. **Shared toolchain image — no Dockerfile of our own.** The
   server-managed `mulch-toolchain` environment (mirtuell) pins the
   shared `ghcr.io/denkhaus/seeds-toolchain:<sha12>` image, owned and
   built by denkhaus/seeds. Reuse over per-repo builds: environment-level
   needs (new tool, new pin) are cross-repo changes landing in the seeds
   repo plus an environment repin — never a mulch-side Dockerfile. This
   repo therefore has no run-images/image-release recipes.
2. **GitButler landing flow (experiment, user directive 2026-10-03).**
   The checkout is a GitButler workspace; the TARGET is `origin/main`
   (main IS the product line and stays the run-PR merge target). Local
   work lives on applied virtual branches (one per agent/line — the
   multi-agent, no-worktrees experiment) landing via
   `but push` + PR + auto-merge (SQUASH — linear history is required on
   main; `but pr auto-merge` engages with merge-method MERGE and must be
   followed by `gh pr merge <n> --auto --squash`). The workspace follows
   main via `but pull`. All VCS writes go through `but`;
   `seeds sync` / `ml sync` are forbidden (own plain-git commits behind
   the workspace) — tracker and expertise stores are committed
   explicitly via `but commit` in the same batch. The push gate runs as
   an explicit unpiped command in the same cell as the push. Revert
   condition: if the experiment fails, the but-specific procedures
   revert to plain-git wording.

## Consequences

- One shared image serves both repos; its evolution (e.g. the mulch
  binary at the self-hosting cutover, seed mulch-96ed) lands in the
  seeds repo.
- Local landings always produce a PR (never a direct main push); run PRs
  are unaffected (the engine merges into main itself).
- The one-time plain-git exception is the empty root commit (gitbutler
  requires an origin HEAD before `but setup`); no content ever lands
  via git again.
