# HANDOFF — session 2026-10-16 → next agent

Overwritten at every session close. Last session: sprint 31 WIP checkpoint
(mid-sprint), closed WITHOUT running the quality gates (explicit user
directive — see "Deferred gate" below).

## Read first (standing context)

- `AGENTS.md` (repo rules: but-only VCS, push gate, seeds/mulch protocol)
- `.agents/skills/iterate/SKILL.md` (the local loop this session runs in)
- Session start: `ml prime` + `seeds prime`; inspect with `but status`

## Where things stand

- **Workspace**: GitButler; local loop lives on the `iterate` virtual
  branch — local-only (no PRs, no `--assignee fabro`, no `.fabro/` edits,
  never `ml sync`, never plain `git` writes).
- **Sprint counter** (`.iterate-state.json`): 30 completed / 30 reflected,
  `last_reflection` 2026-10-16, session-close note appended. Next arch
  gate due at sprint **33** (`last_arch_review_at_sprint` = 30).
- **Sprint 31 is IN PROGRESS** — three seeds all `in_progress`:
  - `mulch-d524` — record/edit flag path custom-type aware (the sprint's
    main ticket; full demand + arch-gate riders in its description)
  - `mulch-b9d4` — prep fold C1: static-registry residue kill
  - `mulch-5e38` — prep fold C2: one truncate law (jsval UTF-16)
- **Landed on `iterate` for sprint 31** (git hashes — but ids don't resolve):
  - `db5a3a8` C1+C3 residue kill + CustomRow→Row fold
  - `ee4772d` C2 one truncate law (jsval)
  - `34752d3` custom-type-aware flag path (record/edit read merged registry)
  - `946a9c6` record.rs split (arch-gate 30 C5 decision: commands/record/)
  - `ff8830f` checkpoint folds: dynamic-flag help prefix ("update " on
    edit), args_os/lossy pre-parse + --json detection (non-UTF-8 argv),
    declared-flag drift-pin test, context_files staged/unstaged
    independence, jsval doc-block fix
  - bookkeeping `ced29db` (claims + 4 ml records), close `afac96f`
    (state/reflect). Worktree clean at close; lane pushed to
    `origin/iterate` through the push gate.

## Deferred gate (user directive 2026-10-16) — RUN FIRST

The quality gates were deliberately NOT run this session. Before touching
code, reviewing, or closing tickets:

1. `timeout 600 just qualitygate` (fmt --check, clippy -D warnings,
   nextest touched crates — nushell side, UNPIPED)
2. `timeout 600 cargo nextest run --workspace` (full battery)

If red: fix as the very first action, commit via `but`.

## Next steps (order)

1. **Gates** above (deferred), then a `but status` freshness check.
2. **Finish sprint 31**: diff `seeds show mulch-d524 --format json`
   (DEMAND/ACCEPTANCE) against the landed commits to find the remainder
   (registry-aware `--type` value_parser wording, custom-field flag
   collection/retry echo edges, battery differentials). Reference `ml` TS
   source first: `mise where npm:@os-eco/mulch-cli` → `<root>/node_modules/
   @os-eco/mulch-cli/src`. Probe before designing (house rule).
3. **Review**: code-review skill, both axes parallel, hand reviewers GIT
   hashes (never but ids), fix findings, commit.
4. **Closeout**: `seeds close <id> --reason` for d524/b9d4/5e38 with the
   commit hashes; short reflection → state 31/31; full battery green.
5. **Push**: `nu .fabro/scripts/push-gate.nu` (exit 0 = open, UNPIPED,
   same cell) then `but push iterate`. No PRs, no landing to main.

## Session learnings (already in ml — no skill change was needed)

- `mx-b8f7f1` mulch-compat/reference — custom-types flag-path law
- `mx-e29144` testing/pattern — pinned expectations must share the
  fixture's instant (3rd occurrence of the volatile-fixture class)
- `mx-b212ea` testing/failure — twin-probe masking: mask BOTH sides,
  optional whitespace, commit shas + temp paths; suspect the harness
- `mx-34c9a2` tooling/decision — jsval owns the UTF-16 truncate

## Known ephemera

- `/tmp/arch-gate-30-notes.md` (explorer notes) may be gone — the arch-gate
  30 candidate content survives in commit `4c575c4`'s message and the seeds.
- Roadmap order after sprint 31: `mulch-b1cb` (aliases, +C6) → 16da wave →
  4088/b6d5/b0f7 (see roadmap notes on `mulch-0e39`).
