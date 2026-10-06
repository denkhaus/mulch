---
name: iterate
description: 'The mulch LOCAL development loop (independent of the fabro develop line). Drive the project forward in sprints: grilling sessions with docs at decision points, implementation in working mode (but-commits, mulch, seeds, quality gates), automatic code-review after every sprint, improve-codebase-architecture after every 3rd sprint, and a self-reflection phase that evolves this skill from session learnings. Runs when the user says "iterate", "weiter", "mach weiter", or asks to advance the project.'
---

# Iterate — the mulch local development loop

One iteration = one **sprint**: a coherent unit of work (milestone step, ticket,
review-fix round). Sprints are counted per project (state file below).

**LOCAL ONLY, and strictly separate from the fabro develop line** (user
directive 2026-10-03): this loop works in this checkout on the `iterate`
virtual branch, commits via `but`, and never pushes, never opens PRs, never
assigns seeds to `fabro`, and never touches `.fabro/workflows`. Landing local
work to `main` happens only through the engine flow on an explicit user order.

This skill is deliberately THIN. All process knowledge lives cross-referenced
in **Seeds** (issues, decisions, milestones) and **Mulch** (domain expertise,
patterns, failures). This skill only orchestrates and points there.

**Code policy: the rust-style-guide skill** (repo-local copy at
`.agents/skills/rust-style-guide/SKILL.md` — the repo's binding style
policy, per AGENTS.md) governs all Rust code here — planning, writing, testing, and reviewing all load it
(steps 2–4).

## The loop

```
        ┌────────────────────────────────────────────────────────┐
        │ 1. ORIENT      ml prime + seeds prime + seeds ready      │
        │                (state lives in mulch + seeds)            │
        ▼                                                       │
        │ 2. DECIDE      decision point? -> GRILLING + DOCS       │
        │                (grilling skill; ADRs via domain-modeling)│
        ▼                                                       │
        │ 3. SPRINT      claim seeds ticket -> implement          │
        │                working mode below ALWAYS ON              │
        ▼                                                       │
        │ 4. REVIEW      code-review skill (both axes, always)    │
        │                fix findings -> commit -> close ticket    │
        ▼                                                       │
        │ 5. GATE        sprint count % 3 == 0 ?                  │
        │                -> improve-codebase-architecture          │
        │                   (HTML report served on localhost;     │
        │                    work candidates as a chain)           │
        ▼                                                       │
        │ 6. REFLECT     after EVERY sprint close (short) +        │
        │                session end (full): self-reflection (below)│
        └────────────────────────────────────────────────────────┘
```

## 1. Orient

- `ml prime` + `seeds prime`
- Start from a clean tree (`but status`): leftover changes from earlier
  sessions leak into sprint diffs and reviews — commit or discard them on
  their own lane first. Never apply/unapply/update branches or `but pull`
  while a background cargo runs (the tree is rewritten under you).
- **REFLECTION GATE (hard)**: if the state file has
  `sprints_completed > sprints_reflected`, run the pending reflection
  (step 6) NOW, before `seeds ready`. A forgotten reflection blocks the next
  sprint — reflection is a MUST, never silently deferred.
- **INFRASTRUCTURE ESCALATION (ask EARLY, not after sprints of working
  around it)**: at ORIENT (and again whenever a sprint's value depends on
  it), check whether the work depends on missing external resources —
  registry access, toolchains, the reference `ml` CLI, binaries, hardware.
  If something is missing or suboptimal, surface the request to the user
  IMMEDIATELY with options; do NOT silently script around it for sprints.
- Run `seeds ready` (it lists assigned tickets too) and pick the
  highest-priority unblocked ticket from the UNASSIGNED ones.
  **Ownership rule**: seeds assigned to `fabro` belong
  to the develop line — leave them; this loop claims tickets WITHOUT
  `--assignee`. The user's explicit pick outranks priority order. Several
  ready and the pick is not obvious? Record pick + reason on the CHOSEN
  ticket (`seeds update <id> --description`) so the rationale survives the
  session.
- No ticket fits the frontier? Create one via a grilling session (step 2) or
  from an out-of-scope finding (working mode).

## 2. Decide — grilling sessions WITH docs

Trigger a grilling round whenever a **load-bearing decision** is open (scope,
architecture direction, milestone content, go/no-go):

- Use the **grilling** skill (rounds, whole frontier, recommended answers).
- Rust design decisions (API shape, errors, async, traits, newtypes) start
  from the **rust-style-guide**: load the relevant guideline pages FIRST —
  policy pages constrain the options a grilling round may weigh. A decision
  that contradicts a guideline must either follow it or amend it explicitly
  in the ADR.
- Documentation is part of the decision, not an afterthought: pair with
  **domain-modeling** (grill-with-docs pattern) — decisions land as ADRs in
  `docs/adr/`, vocabulary in `CONTEXT.md` (create lazily). Amending an ADR
  means amending the DECISION BODY (with date + seed ref), not just the
  consequences list — a stale decision body contradicts shipped code.
- A grilling round that settles a DIRECTION (milestone, architecture shape,
  Grundsatz) gets its ADR in the SAME session — parking it seed-only costs
  the user a prompt.
- Record outcomes: seeds ticket update/close, map/decision update, ADR
  committed via `but`.

## 3. Sprint — working mode (ALWAYS on, every sprint)

- **Claim**: `seeds update <id> --status in_progress` — never with
  `--assignee fabro` (that is the user's ownership switch for the develop
  line).
- **Code policy while writing**: the rust-style-guide governs every Rust
  edit — load `guidelines.md` and only the pages the task needs (routing
  table in the skill) BEFORE writing code, and NAME the loaded pages in the
  session before the first code edit.
- **Edit-verification guard**: after EVERY insert-with-anchor edit or
  mechanical rewrite (python/sed over multiple sites), grep-read the touched
  region BEFORE the next step — and compose anchors from the file as it
  reads NOW: a `cargo fmt` pass between cells rewraps macro args, so an
  anchor built from what a previous cell WROTE stops matching (hit in
  sprints 5 and 6) — (a) the anchor item's `///` doc block still
  sits flush above its OWN item (insertions love the gap between a doc and
  its fn), (b) comments that travelled with rewritten arguments still
  annotate the argument they explain, (c) consumers of any REPLACED
  explicit error-match still hold under the replacement's empty/default
  return. (d) after DELETING or RENAMING a concept (module, flag, variant,
  error name), grep the WORKSPACE docs — wording survives in files the diff
  never touched. (e) after introducing a NEW CROSS-CUTTING signal or handle,
  grep every COUNTERPART WAIT/CONSUMER of the thing it must reach and wire
  each one — or write down why not.
- **Commit regularly** — after each coherent unit, never large WIP. All
  writes via `but` (`but commit -b iterate -m "<msg>" <file-ids>` with
  explicit file IDs from `but diff`); NEVER `git add/commit/push/...`.
  **NEVER `ml sync`** under GitButler — it issues a plain git commit behind
  the workspace's back. Instead: `seeds sync` (report-only under
  `vcs_manager: gitbutler`) shows pending tracker paths, and `.mulch/`
  changes land as explicit `but commit`s in the same batch.
- **Mulch is the domain brain**: `ml record <domain> --type
  <convention|pattern|failure|decision|reference|guide>` for every learning
  (API dead-ends, format quirks, gate outcomes). Evidence:
  `--evidence-commit`, `--evidence-seeds`.
- **Seeds is the only tracker**: all work, bugs, feature demands, deferred
  findings become seeds tickets. Out-of-scope findings during a sprint ->
  `seeds create` + `label needs-triage` immediately (never silently
  absorbed). Commit messages are NOT trackers: a deliberate behaviour
  deviation discovered mid-sprint gets its seed BEFORE the commit that
  contains it; the commit then links the seed id.
- **Quality gates (before every commit round)**: `timeout 600 just
  qualitygate` — it runs fmt --check (workspace), clippy `-D warnings`
  and nextest (touched crates) nushell-side, where no bash pipe can mask
  a gate result. At sprint end additionally one full-workspace battery:
  `timeout 600 cargo nextest run --workspace`. The raw cargo commands
  stay documented in AGENTS.md (single source of truth for them). No
  coverage percentage gate exists yet — when one lands as a seed, it
  joins this list (nu-agent precedent: >= 80% lines).
- **Hang guard**: run full suites under a TIMEOUT (e.g. `timeout 420 cargo
  nextest ...`) — a hung test must fail fast, not stall the session.
- **PIPE-TRUTH**: a gate piped through `tail`/`grep` returns the PIPE's
  exit code, not the gate's; a trailing `; echo EXIT=$?` resets even a
  pipefail'd script to exit 0. Gate shape that holds: `set -o pipefail; cmd
  2>&1 | tee <log>; rc=${PIPESTATUS[0]}; echo GATE_RC=$rc; exit $rc` —
  never trust a piped or echo-followed gate's exit. (Retro 2026-10-06:
  two masked gate failures in one sprint — hence the `just qualitygate`
  recipe above as the default; this rule binds any manually typed gate,
  e.g. the sprint-end workspace battery.)

## 4. Review — ALWAYS after each sprint

Run the **code-review** skill autonomously after every larger coding session:
standards + spec reviewers as PARALLEL SUBAGENTS. Scope the diff to the
SPRINT's own commits: fixed point = the last commit of the previous sprint
on the `iterate` lane (sprint-close bookkeeping commits land between sprints
and confuse reviewers otherwise). The STANDARDS reviewer treats the
rust-style-guide as the primary documented-standards source — pass its
guideline pages (plus AGENTS.md, CONTEXT.md, ADRs) into the reviewer prompt.

**Probe hygiene for every reviewer subagent**: every command under
`timeout`, NO nohup/background daemons, kill what you spawn, reads are
BOUNDED (`sed -n`/`head`, never an unbounded cat of a large file), a stuck
tool is ABANDONED after one retry — the verdict forms from what is already
read — and SEND THE REPLY as soon as the verdict is formed: a reviewer that
cannot complete SAYS so instead of hanging. Reviewer subagents are READ-ONLY
(no `but`, no writes, no pushes): findings come back as the reply, the host
commits.

Aggregate both axes, fix findings, commit via `but`, then close the ticket
with `seeds close --reason` referencing the commits. (AGENTS.md's "never
close by hand" binds the fabro develop line's runs — this local loop closes
its own tickets explicitly.) Before the fold commit:
grep-verify EVERY claimed fix against the code in the same cell — the
claim-without-land class (an assert-abort mid-batched-python-cell lands the
EARLIER writes and silently kills the later ones) extends to every claimed
write of ANY multi-replace cell, not only folds. Clean up reviewer children.
A sprint is NOT closed until its SHORT reflection (step 6) ran and
`sprints_reflected` caught up.

## 5. Architecture gate — every 3rd sprint

After every **3 completed sprints** (sprint count from the state file), run
the **improve-codebase-architecture** skill (global): explorer subagent, HTML report served on a localhost URL
(never xdg-open, never a file path — user policy), work the Strong
candidates as a chain (grill the chosen order), fold judgement calls
opportunistically. Every non-Speculative candidate becomes a seed IMMEDIATELY
(Strong AND Worth exploring alike); Speculative ones stay referenced by their
existing seed/ADR. Reset nothing — sprints keep counting.

## 6. Reflect — self-evolution of this skill

Reflection is a MUST, on two cadences, enforced by the state file — never
skip it, never silently defer it:

- **SHORT, after every sprint close** (part of closing, same session): scan
  the sprint's learnings, apply surgical skill changes if the LOOP needs
  them, set `sprints_reflected = sprints_completed`.
- **FULL, at session end**: same procedure across the whole session, plus
  updating `last_reflection`.

If you cannot run it now (session dying), leave `sprints_reflected` behind —
the next ORIENT blocks until it runs.

Procedure (driven by Mulch; do not bloat the skill):

1. `ml search` this session's learnings (patterns/failures with
   `--evidence-commit`s).
2. Ask per finding: does the LOOP need to change (ordering, gates,
   triggers), or is the learning already captured in mulch/seeds (then no
   skill change)?
3. Apply only surgical changes to this file: adjust triggers/gates, fix a
   step that repeatedly caused friction. Everything else stays referenced,
   not inlined.
4. Record the skill change itself in mulch (`ml record dev-loop --type
   decision`, evidence = the commit touching this skill) so future sessions
   see WHY the loop looks like this.
5. Update the sprint counter and last-reflection pointer in the state file.

This skill is a LOCAL loop asset owned by this loop (user directive
2026-10-03): it evolves through its own reflection procedure. The
`.fabro/**` develop-line rule (loop assets evolve through seeds, friction
goes to the journal) is untouched — never edit `.fabro/` assets in-pass.

## Delegation (user directive 2026-10-03)

Delegate worthwhile, self-contained work to SUBAGENTS instead of doing
everything in the host session: both code-review axes (always parallel),
the arch-gate explorer, mechanical workspace-wide sweeps (renames, doc
consistency greps), research (reference `ml` behaviour probes), and
independent implementation chunks. Subagents NEVER run git write commands,
never push, never `but commit` — they produce findings/files/replies
(reviewers are strictly read-only; explorer/sweep children may write
files); the host owns every `but` mutation on the `iterate` lane. Spawn children in
parallel, collect their replies, fold results in one pass.

## State file (sprint counter)

`.iterate-state.json` in the repo root (committed via `but`):

```json
{
  "sprints_completed": <int>,
  "sprints_reflected": <int>,
  "last_arch_review_at_sprint": <int|null>,
  "last_reflection": "<date>",
  "notes": []
}
```

Increment `sprints_completed` when a sprint's ticket closes; set
`sprints_reflected = sprints_completed` when its reflection ran. Invariant:
`sprints_reflected >= sprints_completed - 1` after every session step, and
equal after a reflection. ORIENT refuses to start a sprint while
`sprints_reflected < sprints_completed`. Trigger step 5 when
`sprints_completed % 3 == 0` and `last_arch_review_at_sprint !=
sprints_completed`.

## Pointers (read, don't duplicate)

- Format contract + product direction: `README.md`; basis ADR-0023 in
  denkhaus/fabro
- Process decisions + milestones: seeds tickets (`seeds show`)
- Domain expertise: `.mulch/expertise/` (`ml prime`, `ml search`) — domains:
  rust, tooling, nushell, dev-loop, mulch-compat, testing
- Working-mode details: AGENTS.md, this skill, memories
- Code policy (Rust): `.agents/skills/rust-style-guide/SKILL.md` — binding
  for writing, planning, reviewing, and testing Rust in every step;
  repo-local copy of the `.fabro/skills/` original — when the fabro line
  updates its vendored copy, refresh this one (drift-check at reflection)
- Skills used by this loop: grilling, domain-modeling, code-review,
  codebase-design, research, improve-codebase-architecture (global);
  rust-style-guide (repo-local copy, see above)
