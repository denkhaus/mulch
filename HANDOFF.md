# HANDOFF — session 2026-10-17 (evening) → next agent

Overwritten at every session close. Last session: sprints 31 + 32
CLOSED, notes rotation ported, nu 0.116.1 checked.

## Read first (standing context)

- `AGENTS.md` (repo rules: but-only VCS, push gate, seeds/mulch protocol)
- `.agents/skills/iterate/SKILL.md` (the local loop)
- Session start: `ml prime` + `seeds prime`; `mise x -- but status`

## Where things stand

- **Sprint counter** (`.iterate-state.json`): 32 completed / 32
  reflected, `last_reflection` 2026-10-17. Next arch gate due at
  sprint **33** (`last_arch_review_at_sprint` = 30).
- **Sprint 31 CLOSED** (d524 custom-type-aware flag path + b9d4
  residue kill + 5e38 truncate law; commits db5a3a8/ee4772d/34752d3/
  946a9c6/ff8830f + fold ebd86a1, closeout 9a4c56f).
- **Sprint 32 CLOSED** (b1cb custom_types aliases: 9-rule validation
  law, apply_aliases at every read seam, raw-bytes legacy-outcome
  corner; commits b67f5dc + fold 582bf39, closeout ors).
- **PUSH PENDING**: the push gate refused at close (2 active runs on
  mirtuell.net). Retry `nu .fabro/scripts/push-gate.nu` then
  `mise x -- but push iterate` when open — the lane carries
  fbb0679..ors (rotation port + sprints 31/32). Everything is
  committed locally; only origin/iterate lags.
- **Workspace gotchas hit this session** (recipes recorded in ml):
  remote-rewrite `{conflicted}` → containment-check + `but discard`;
  commit file/hunk IDs from `but status -fv` (NOT grepping but diff
  box headers — they swallow files).

## Next steps (order)

1. Push the lane when the gate opens (see above), then ORIENT as
   usual.
2. **Sprint 33 = the 16da WAVE** (query/prime/search — read+render:
   render seam from sprint 28, registry view from sprint 30/b1cb,
   scoring slice from 228c feed it). Probe the reference TS first
   (commands/query.ts, prime surfaces, utils/bm25.ts). Consider
   splitting into lane subagents — it is the largest remaining
   surface.
3. **ARCH GATE @ 33** falls due after sprint 33 closes
   (improve-codebase-architecture, HTML report on localhost).
4. Reviews: both axes PARALLEL, hand reviewers GIT hashes, collect
   ALL hunk IDs via `but status -fv`.

## Known ephemera

- Reference probe configs need `version: '1'` (QUOTED) at
  .mulch/mulch.config.yaml; domain files at .mulch/expertise/.
- Rust string continuations (`\` line end) EAT the next line's
  leading whitespace — keep YAML children on one source line in test
  fixtures (hit twice this session).
- Style-guide drift-check 2026-10-17: intentional-diff-only, no
  refresh. nu 0.116.1 compat verified; pin switch waits for the
  seeds image bump (seeds repo owns it).
