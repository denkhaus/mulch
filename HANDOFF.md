# HANDOFF — session 2026-10-17 → next agent

Overwritten at every session close. Last session: sprint 31 CLOSED
(d524 + b9d4 + 5e38), gates caught up, notes rotation ported.

## Read first (standing context)

- `AGENTS.md` (repo rules: but-only VCS, push gate, seeds/mulch protocol)
- `.agents/skills/iterate/SKILL.md` (the local loop this session runs in)
- Session start: `ml prime` + `seeds prime`; inspect with `mise x -- but status`
  (plain `but` is not on PATH — mise backend)

## Where things stand

- **Workspace**: GitButler; local loop lives on the `iterate` virtual
  branch — local-only (no PRs, no `--assignee fabro`, no `.fabro/` edits,
  never `ml sync`, never plain `git` writes). Lane pushed through the
  gate at close: `origin/iterate` = 9a4c56f.
- **Sprint counter** (`.iterate-state.json`): 31 completed / 31
  reflected, `last_reflection` 2026-10-17. Next arch gate due at sprint
  **33** (`last_arch_review_at_sprint` = 30).
- **Sprint 31 CLOSED**: d524 (custom-type-aware record/edit flag path),
  b9d4 (static-registry residue kill), 5e38 (one UTF-16 truncate law).
  Commits db5a3a8 / ee4772d / 34752d3 / 946a9c6 / ff8830f + review fold
  ebd86a1. Both review axes green; closeout 9a4c56f.
- **Loop tooling landed**: sprint-notes rotation (brightctl port) —
  state keeps the newest 10 notes, older ones archive to
  `.iterate-state-archive.jsonl`; run `just rotate-notes` after every
  sprint close when notes > 10 (the skill documents it).
- **nu 0.116.1**: compat verified green (all 27 scripts; nushell-pro
  reference), pin deliberately NOT switched — the shared seeds
  toolchain image pins 0.115.0 and this repo's `.mise.toml` mirrors it.
  Switch order: seeds `Dockerfile.toolchain` + seeds `.mise.toml`
  first (environment change belongs in denkhaus/seeds), then this
  mirror. Tooling record mx-... has the details.

## Next steps (order)

1. ORIENT as usual (`ml prime`, `seeds prime`, lane-staleness check —
   `but branch update iterate` if origin moved; watch for the
   remote-rewrite `{conflicted}` case: containment-check, then
   `but discard`).
2. **Sprint 32 = mulch-b1cb** (aliases) + the C6 rider (config-error
   single owner, arch-gate 30 C6 — Speculative, folds into b1cb).
   Probe the reference alias law FIRST (`mise where
   npm:@os-eco/mulch-cli` → src; record.ts/config). Battery surface:
   `tests/cli_parity/custom_types.rs` + the registry tests.
3. After b1cb: the **16da wave** (read/render: query/prime/search —
   render seam from sprint 28 + registry view from sprint 30 feed it),
   then 4088/b6d5 as 7c55 prep. Arch gate falls due after sprint 33.
4. Reviews: code-review skill, both axes PARALLEL, hand reviewers GIT
   hashes. Collect ALL hunk IDs per file when committing
   (`but diff` — one ID per hunk; the box-header grep is unreliable,
   verify with `git show --stat` after).

## Known ephemera

- `/tmp/arch-gate-30-notes.md` may be gone — content survives in
  commit 4c575c4's message and the seeds.
- Style-guide drift-check done 2026-10-17: intentional-diff-only
  (local header + sprint-15 match-order bullet), no refresh needed.
