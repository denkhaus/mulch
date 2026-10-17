# ADR-0001: Bootstrap posture — shared toolchain image and landing flow

- Status: Accepted
- Date: 2026-10-03
- Amended: 2026-10-17 (mulch-6817) — decision 2 reverts to plain git;
  the GitButler experiment ran 2026-10-03..2026-10-17. Amended again
  the same day (user directive): local work happens on `main`, the
  `iterate` branch removed — iterate is the skill, not a branch
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
2. **Landing flow — plain git (amended 2026-10-17, mulch-6817).**
   `main` IS the product line and the run-PR merge target; it keeps
   required-linear-history, so the only landing is a squash
   (`gh pr merge <n> --auto --squash`) — never a direct push to main.
   Local work happens ON `main` itself (amended 2026-10-17, user
   directive — the `iterate` branch is removed; iterate is the SKILL
   defining the dev flow, and other agents/runs still take their own
   branch, one per agent/line), committed with `git add` / `git commit`
   with `.seeds/` and `.mulch/` staged in the SAME commit (tracker and
   expertise stores must not open their own commits — `seeds sync` /
   `ml sync` stay out of the batch flow). Push: `git push origin main`
   through the gate, fast-forward only (`git fetch` + `git pull
   --ff-only` first); the squash landing above stays the run-PR path.
   The push gate runs as an explicit unpiped
   command in the same cell as every push (the lefthook pre-push hook is
   the operator-opt-in second layer).
   HISTORY: the 2026-10-03..2026-10-17 GitButler experiment replaced
   this with virtual branches driven by `but` (`but push` + `but pr new`
   + squash re-arm, workspace following main via `but pull`, all VCS
   writes through `but`). Its own revert condition — "if the experiment
   fails, the but-specific procedures revert to plain-git wording" —
   fired on user directive 2026-10-17: teardown ran, main was
   fast-forwarded to the iterate tip, and the but procedures plus the
   `gitbutler/*` refs were retired (mulch-6817).

## Consequences

- One shared image serves both repos; its evolution (e.g. the mulch
  binary at the self-hosting cutover, seed mulch-96ed) lands in the
  seeds repo.
- Local work is pushed straight to `main` (gated, fast-forward only) —
  this loop opens no PR; run PRs are unaffected (the engine merges into
  main itself) and stay the only reviewed landing path.
- The empty root commit `235a70d` (bootstrap, needed so gitbutler could
  set its target) stays a historical one-off — the experiment it served
  is retired; no process depends on it.
