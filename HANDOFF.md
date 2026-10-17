# HANDOFF — session 2026-10-17 (post-teardown) → next agent

Overwritten at every session close. Last session: sprint 33
(mulch-6817, the post-gitbutler doc rework) CLOSED with both review
axes green; the checkout is PLAIN GIT now.

## Version control = plain git (the GitButler experiment is over)

The experiment ended 2026-10-17 (teardown; main fast-forwarded to the
iterate tip 247f9ac). SAME DAY, later user directive: **no `iterate`
BRANCH — work happens directly on `main`** (merged 4e43dd3, local
`iterate` deleted; iterate is the SKILL that defines the dev flow).
ADR-0001 decision 2 carries the amended decision body; AGENTS.md's
Version control + Push gate sections are the binding wording. What
changed for the next agent:

- Session start: `ml prime` + `seeds prime` + `git status` — NOT
  `but status` (the `but` binary errors: "not currently on a
  gitbutler/* branch"). Main staleness: `git fetch`, compare
  `.iterate-state.json` against `origin/main`'s top commit, then
  `git pull --ff-only` (clean tree, no background cargo).
- Commits: plain `git add <paths> && git commit`, with `.seeds/` +
  `.mulch/` batched into the same commit; long messages via
  `git commit -F /tmp/msg.txt`; verify the landed file list with
  `git show --stat <hash>` (the but-era hunk-id collection and
  `but status -fv` recipes retired with the tool).
- Push: the AGENTS.md two-layer command in ONE unpiped cell —
  `nu .fabro/scripts/push-gate.nu` (exit 0), then `git push origin
  main` (fast-forward only).
- DELETED at this close (containment-checked first): local
  `gitbutler/workspace` (tip 66bed33 is an empty commit),
  `gitbutler/target` (contained in main), `chore/but-fork-6` (only the
  obsolete but pin). STILL PRESENT, deliberately untouched — deleting
  them is a GATED push and was not in the seed's list:
  `origin/gitbutler/workspace`, `origin/mise-but-pin`,
  `origin/mise-pin-but-fork3`, `origin/mise-pin-but-fork5`,
  `origin/vcs/gitbutler-mode`. Also pending a gated push:
  **`origin/iterate` deletion** (content is contained in main) — the
  branch is gone locally as of the user directive above.

## Read first (standing context)

- `AGENTS.md` (repo rules: plain-git VCS, push gate, seeds/mulch
  protocol, iterate loop)
- `.agents/skills/iterate/SKILL.md` (the local loop)
- Session start: `ml prime` + `seeds prime` + `git status`

## Where things stand

- **Sprint counter** (`.iterate-state.json`): 33 completed / 33
  reflected, `last_reflection` 2026-10-17.
- **ARCH GATE @ 33 IS DUE** (`33 % 3 == 0`; `last_arch_review_at_sprint`
  = 30) — run improve-codebase-architecture next, grill the chain order.
- **Sprint 33 CLOSED** (mulch-6817): commits `08e9401` (the doc
  rework), `e60d916` (review folds), `71d2f9b` (ml record retirement) +
  the closeout commit. Follow-up seed filed: **mulch-e2df**.
- **PUSH PENDING**: the gate refused at this close (1 active run,
  `01M3AEHDGAT8BFZA0BZQJ6832H`). Retry `nu .fabro/scripts/push-gate.nu`
  then `git push origin main` when it opens — everything is committed
  locally, `origin/main` (at 2bd8bcd) lags the local sprint commits.

## Next steps (order)

1. Push `main` when the gate opens (two-layer command above), then
   ORIENT as usual.
2. **ARCH GATE @ 33** — improve-codebase-architecture (explorer
   subagent, HTML report on a localhost URL, grill the Strong-candidate
   chain order; every non-Speculative candidate becomes a seed).
3. **Sprint 34 = the 16da WAVE** (query/prime/search — read+render:
   render seam from sprint 28, registry view from sprint 30/b1cb,
   scoring slice from 228c feed it). Probe the reference TS first
   (commands/query.ts, prime surfaces, utils/bm25.ts). Consider
   splitting into chain subagents — the protocol is now: children own
   DISJOINT files, never touch git, the HOST commits each delivery.
4. **mulch-e2df**: rework `scripts/run_workflow.nu` + the justfile run
   comment off `but` (needs a live petri run to verify) and drop the
   leftover global `~/.config/mise/config.toml` `http:but` pin.
5. Reviews: both axes PARALLEL via `pig -p` (one process per axis, the
   diff passed INLINE in the prompt, 400-word caps) — the `claude -p`
   channel was credit-exhausted this session.

## Known ephemera

- Reference probe configs need `version: '1'` (QUOTED) at
  .mulch/mulch.config.yaml; domain files at .mulch/expertise/.
- Rust string continuations (`\` line end) EAT the next line's
  leading whitespace — keep YAML children on one source line in test
  fixtures (hit twice in sprint 32).
- Style-guide drift-check 2026-10-17: intentional-diff-only, no
  refresh. nu 0.116.1 compat verified; pin switch waits for the
  seeds image bump (seeds repo owns it).