---
name: iterate
description: 'The mulch LOCAL development loop (independent of the fabro develop line). Drive the project forward in sprints: grilling sessions with docs at decision points, implementation in working mode (git commits, mulch, seeds, quality gates), automatic code-review after every sprint, improve-codebase-architecture after every 3rd sprint, and a self-reflection phase that evolves this skill from session learnings. Runs when the user says "iterate", "weiter", "mach weiter", or asks to advance the project.'
---

# Iterate — the mulch local development loop

One iteration = one **sprint**: a coherent unit of work (milestone step, ticket,
review-fix round). Sprints are counted per project (state file below).

**mulch's own development loop** (user directives 2026-10-03 and
2026-10-06): mulch is a STANDALONE tool whose lib crate fabro natively
embeds — this loop develops mulch on its own account, directly on
`main`, committing via git, opening no PRs, assigning no
seeds to `fabro`, and never touching `.fabro/workflows`. After every
sprint closeout `main` IS PUSHED to `origin/main` (through the push gate,
fast-forward only) so the work is available on other machines — pushing
is not reviewing: run PRs stay the engine's business and land only on an
explicit user order. Remember the
crate is an integration surface for fabro: lib API changes are
integration-relevant.

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
- Start from a clean tree (`git status`): leftover changes from earlier
  sessions leak into sprint diffs and reviews — commit or discard them on
  their own branch first. Never pull/rebase/switch branches while a
  background cargo runs (the tree is rewritten under you).
- **MAIN-STALENESS CHECK (the sprint-29 "lane staleness" lesson)**: a
  clean `git status` does NOT mean the worktree is current — another
  machine may have moved `origin/main`. `git fetch`, then compare the
  state file's `sprints_completed` against `origin/main`'s top commit
  message BEFORE trusting the tree; when behind: `git pull --ff-only`
  (or `git rebase origin/main` when you already carry local commits),
  never while a background cargo runs and never on a dirty worktree —
  commit or stash first.
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
  ticket (`seeds update <id> --append-description "$(cat /tmp/x.txt)"` —
  `seeds update` has `--append-description`/`--set-description`/
  `--replace-in-description`, never `--description`; long text through a
  file, per the quoted-argument rule) so the rationale survives the
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
  committed via git.

## 3. Sprint — working mode (ALWAYS on, every sprint)

- **THREE SEEDS PER SPRINT (user directive 2026-10-24)**: a sprint
  claims and closes at least THREE seeds — otherwise we review more
  than we build. Plan the bundle at ORIENT (pick a main seed plus
  riders: preps, folds, small bugs from the same family); a sprint
  with fewer than three closable seeds is a planning defect, not a
  small sprint. Big waves still count as one seed of the three
  (their riders come from the same family).
- **Comments are ENGLISH ONLY (user directive 2026-10-24)**: code
  comments, doc comments, commit messages, seeds texts, and mulch
  records — every durable artifact. Session conversation may be
  German; what the repo keeps is not.
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
  writes via plain git: `git add <explicit paths>` (never a repo-wide
  `-A`) + `git commit -m "<msg>"`, batching `.seeds/` + `.mulch/` into
  the same commit per AGENTS.md (never `ml sync`; `seeds sync --status`
  / `--dry-run` is the pending-change check).
  **Long or quoted arguments go through a FILE, never inline**: commit
  with `git commit -F /tmp/msg.txt`, and pass `seeds close --reason` /
  `ml record --rationale` as `"$(cat /tmp/x.txt)"` (one expansion, the
  content is never re-evaluated). An inline double-quoted string runs
  every backtick it contains AS A COMMAND (sprint 25: a commit message
  lost its quoted parts; 2026-10-17 close: backticked words inside an
  `ml record --rationale` executed a foreign CLI, corrupted both
  records, and fired an UNGATED push).
- **Commit-integrity check (sprint 20 lesson)**: a file can be invisible
  at commit time (the but-era `but status` under-reported the dirty
  worktree — records.rs never entered the commit while the message
  claimed it). List the intended files first (`git status --short` plus
  `git diff --name-only`), stage them by explicit path, and after the
  commit verify the real file list with `git show --stat <hash>`. Never
  write counts into a message ("7 sites collapsed") without counting the
  COMMITTED diff — spec reviewers check exactly that claim.
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
(no commits, no writes, no pushes): findings come back as the reply, the host
commits.

**Hand reviewers the COMMIT hash**: give them `git log --grep="<seed>"
--format=%h -1` or "the sprint's single doctor/… commit", so the review
starts on the right diff (sprint-20/21 lesson: an id `git show` cannot
resolve cost both reviewers a hunt).

Aggregate both axes, fix findings, commit via git, then close the ticket
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
6. Push `main` through the push gate exactly as AGENTS.md specifies it
   (two layers, one unpiped cell, fast-forward only) — the sprint's
   closeout commit must reach `origin/main` so the work is available on
   other machines. Run PRs stay out of scope (explicit user order only).

This skill is a LOCAL loop asset owned by this loop (user directive
2026-10-03): it evolves through its own reflection procedure. The
`.fabro/**` develop-line rule (loop assets evolve through seeds, friction
goes to the journal) is untouched — never edit `.fabro/` assets in-pass.

## Delegation (user directive 2026-10-03, extended 2026-10-11)

Delegate worthwhile, self-contained work to SUBAGENTS instead of doing
everything in the host session: both code-review axes (always parallel),
the arch-gate explorer, mechanical workspace-wide sweeps (renames, doc
consistency greps), research (reference `ml` behaviour probes), and
independent implementation chunks. Spawn children in parallel, collect
their replies, fold results in one pass.

**Parallel-chain sprints (user directive 2026-10-11, proven sprint 22;
the GitButler lane machinery behind it retired 2026-10-17):** a
multi-seed sprint's chain links run as SIMULTANEOUS subagents in this
one worktree. Binding protocol: DISJOINT file ownership per child —
shared files (`tests/`, `README.md`, `.seeds/`, `.mulch/`) are
host-folded from child deliveries under `/tmp/sprintNN/<child>/`;
children NEVER touch git (no add/commit/pull/push/`ml sync`, no branch of
their own): each child writes only its OWNED files, delivers the rest to
its /tmp dir, and the HOST commits each delivery as it arrives
(`git add <that child's paths> && git commit` — one commit per child,
explicit paths only — see the commit-integrity check); children run NO
workspace fmt/clippy/qualitygate (host gates at integration) and wrap
every long command in `timeout`; compile errors in files a child does
not own are sibling WIP — wait 60-120s, retry (max 2), never fix foreign
files. Host integration: fmt+clippy over the folded tree, battery folds,
ONE final integration commit (verify the file list with `git show --stat
<hash>`). Reviewers stay strictly read-only; children write only their
owned files + /tmp deliverables. Sprint-23 additions: (a) when seeds
share ONE cross-chain constructor, the HOST pre-lands it on `main`
before spawning (children consume the landed API — no child-to-child
dependencies) — but only lib-side constructors (`pub` items are never
dead code): a `pub(crate)` CLI helper is dead until its adopter lands
and fails the pre-land's clippy gate, so it goes in the ADOPTING
child's brief instead (sprint 37: flag_value_failure); (b) delivered battery fixtures must be clippy-clean BY
CONSTRUCTION — `-D warnings` covers tests, children cannot run clippy,
and hand-rolled string building in fixtures cost four gate iterations
(push_str(&format!) -> format_collect -> write! -> writeln!): prefer
`writeln!`/prebuilt strings, and have each child run
`cargo clippy -p mulch --all-targets -- -D warnings` on a SCRATCH
battery copy before delivering.

## State file (sprint counter)

**The notes ARE the handoff.** There is no HANDOFF file (removed
2026-10-17, user directive): seeds are the only source of issues, the
state notes carry session state + a trailing `NEXT:` line, mulch
carries knowledge, AGENTS.md + this skill carry the rules. Anything
found in a handoff-shaped file belongs in one of those four.

`.iterate-state.json` in the repo root (committed via git):

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

**Notes rotate (ported from brightctl 2026-10-17):** the state file keeps
the **last 10** notes; older ones move to `.iterate-state-archive.jsonl`
as JSONL (history stays git-diffable, one `{"note": …}` record per line,
oldest first). Deterministic and idempotent: `nu
scripts/rotate-sprint-notes.nu` (just: `just rotate-notes`, `-dry` to
preview). Run it after every sprint close when `notes > 10` — 41 notes
had grown the state file to 33 KB, read whole at every session start.
The script needs no entry in the reflection procedure: it changes the
file, not the loop.

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
