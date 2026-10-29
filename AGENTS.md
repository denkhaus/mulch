# AGENTS.md — mulch

Native Rust implementation of the mulch structured-expertise format
(the `ml` CLI, `@os-eco/mulch-cli`). Format-compatibility contract and
product direction live in `README.md`; the deciding record is ADR-0023
in denkhaus/fabro. Sister repo of denkhaus/seeds (this loop was cloned
from the seeds pattern).

## Build and test

- `cargo build --workspace` — build
- `cargo nextest run --workspace` — all tests
- `cargo nextest run -p mulch -- <test_name>` — single test
- `cargo +nightly-2026-09-22 fmt --check --all` — format check (pinned
  nightly; install with `rustup toolchain install nightly-2026-09-22
  --profile minimal --component clippy,rustfmt`)
- `cargo +nightly-2026-09-22 clippy --workspace --all-targets -- -D warnings`
- `just qualitygate` — the develop loop's touched-crates gate
- Reference `ml` TS source — HISTORICAL (ADR-0008 dissolved parity;
  read it for format questions only, no tool runs it):
  `~/.local/share/mise/installs/npm-os-eco-mulch-cli/0.10.7/
  node_modules/@os-eco/mulch-cli/src` while an install remains
- No toolchain image recipe here: runs use the SHARED
  `ghcr.io/denkhaus/seeds-toolchain` image (owned by denkhaus/seeds)
  through the server-managed `mulch-toolchain` environment. Environment-
  level changes land in the seeds repo, never here.

The Rust toolchain is owned by rustup (pinned `nightly-2026-09-22`);
mise owns just/bun/nushell/ripgrep; this repo's OWN `mulch` binary
(`just install`, crates.io release 0.1.0) is the expertise surface
(ADR-0007/0008). The tracker is the EXTERNAL `seeds` binary (denkhaus/seeds,
installed via its GitHub Releases install script or
`cargo install --locked seeds`) — this repo does not build it.

## Issue tracking (Seeds)

Work is tracked in Seeds (the `seeds` binary, git-native in `.seeds/`),
not GitHub Issues. Seed ids carry the prefix `mulch-`.

- **Session start:** run `seeds prime`.
- **Filers file UNASSIGNED.** Agents that file seeds create them without
  `--assignee` — new seeds land unassigned in the backlog.
- **The develop line only works on seeds assigned to `fabro`.** The
  planner lists candidates with `seeds ready --assignee fabro --limit 200`.
  Assignment is the user's ownership switch (veto: reassign or unassign).
- **Claim:** `seeds update <id> --status in_progress --assignee fabro`.
- **Close:** never by hand from a run — the deterministic Closeout step
  closes approved seeds; the planner's one exception is the superseded
  close with a mandatory `--reason`.
- Supported read path: `seeds show <id> --format json`. Never parse
  `.seeds/issues.jsonl` by hand.
- **Never parse raw tracker files; never invent seeds flags.**

## Expertise (Mulch)

- **Self-hosting (ADR-0007/0008): this repo runs on ITS OWN `mulch`
  binary** (`just install` puts it on PATH); the retired `ml` CLI has
  no role — parity with it is dissolved, the format contract (ml-
  readable stores, additive changes only) is pinned by our own
  schema tests.
- **Session start:** run `mulch prime`.
- Before finishing a task, record durable insights (`mulch record <domain>
  --type <convention|pattern|failure|decision|reference|guide>
  --description "..."`); skip when nothing surfaced. Upserts by `--name`
  merge outcomes — amend the existing record instead of filing a second
  one for the same lesson.
- **Commit `.mulch/` (and `.seeds/`) together with the change they
  document** — `mulch sync` / `seeds sync` open their own commits, so stage
  the tracker + expertise paths explicitly in the same `git add` /
  `git commit` batch (`seeds sync --status` / `--dry-run` remains the
  pending-change check).

## Workflow assets

The develop workflow lives in `.fabro/workflows/develop/` (graph, prompts,
scripts, schemas) plus `.fabro/scripts/` and `.fabro/skills/` (vendored
rust-style-guide, improve-codebase-architecture — the only skills a run's
agent stages may load). All loop-asset evolution happens through the
develop line itself (ADR-0012/0013 in denkhaus/fabro): report friction in
the journal, never fix loop assets in-pass outside a seed that targets
them. Run PRs integrate into `main` — there is no upstream mirror.

`.fabro/`, `.seeds/`, `.mulch/`, `scripts/`, and `justfile` are fs_hide
bound for file tools in runs; the shell reads and writes them normally
(grep, sed, cat, python3 heredocs), and `seeds`, `ml`, `just` keep
working.

## Version control (plain git; GitButler experiment retired 2026-10-17)

This is a plain-git checkout — the 2026-10-03..2026-10-17 GitButler
experiment (virtual branches driven by `but`) ended on user directive,
exercising its own documented revert condition (ADR-0001). `main` IS the
product line, the merge target of run PRs, AND the branch local work
happens on — there is no separate loop lane (the `iterate` branch was
removed 2026-10-17 on user directive: iterate is the SKILL that defines
the dev flow, not a branch). Other agents/runs still take their own
branch, one per agent/line.

- Normal git applies: `git add` / `git commit` / `git switch` /
  `git fetch` / `git rebase`. Read-only inspection (`git log`,
  `git show`, `git diff`, `git fetch`) is always fine.
- **Pushing the work:** local commits sit on `main` and go out with
  `git push origin main` through the push gate below — fast-forward
  only: `git fetch` + `git pull --ff-only` first (main keeps
  required-linear-history, so a diverged main is never force-pushed).
  Run PRs land on main the engine way (squash); this loop opens no PRs
  of its own.
- `git fetch` before reading tracker state when another machine may have
  moved `main` (the tracker view is branch-local), and compare
  `.iterate-state.json` against `origin/main`'s top commit before
  trusting the worktree. WAIT for any background cargo to finish before
  you pull/rebase/switch branches — it rewrites the tree under you.
- The multi-agent setup runs on plain branches (one per agent/line). The
  `but` CLI and the `gitbutler/*` branches are retired — nothing may
  depend on them.

## Push gate (two layers)

Every push runs the gate as an explicit, unpiped COMMAND in the same
cell as the push — the lefthook pre-push hook is operator opt-in (one
`lefthook install` per checkout), so the command is the reliable layer:

1. `nu .fabro/scripts/push-gate.nu` (exit 0 = open: no running
   conductor/develop/revisor pass on https://mirtuell.net AND no open
   run-PR on denkhaus/mulch; quota-parked runs do not refuse), then
2. `git push origin <branch>` (for this loop: `main`, fast-forward only).

Never pipe the gate (`gate | tail && git push` reads the pipe member's
exit code, not the gate's). `lefthook.yml` additionally wires the same
gate as a git pre-push hook (operator escape `git push --no-verify`
stays documented); operators activate it once per checkout with
`lefthook install`.

INVARIANT: run sandboxes NEVER install hooks — a toolchain/bootstrap
`lefthook install` or a `core.hooksPath` override would stall the whole
develop line (every stage push would hit the gate seeing its own active
run). Never add hook installation to any image or CI config.

## Clone layout (runs)

The petri engine mounts this repository at `/workspace` in run
containers; the shared toolchain image's mise trust pins `/workspace`
(plus the seeds repo's legacy `/repos/denkhaus/seeds` compat path —
irrelevant here, mulch scripts must not rely on any `/repos/denkhaus/
mulch` path existing).

## Rust style

`.fabro/skills/rust-style-guide/SKILL.md` is the binding coding policy for
every Rust diff — read it before writing or reviewing Rust. The workspace
lints in `Cargo.toml` mirror it mechanically.

## Agent skills

### Issue tracker

Issues live in this repo's own Seeds tracker (`.seeds/`, `seeds` CLI, prefix `mulch-`) — not GitHub Issues. See `docs/agents/issue-tracker.md`.

### Triage labels

The five canonical triage labels are used as-is (`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`). See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `CONTEXT.md` and `docs/adr/` at the repo root; read before exploring, create lazily via `/domain-modeling`. See `docs/agents/domain.md`.

## Iterate loop (LOCAL)

Local development runs in sprints via the local `iterate` skill
(`.agents/skills/iterate/SKILL.md`): grilling-with-docs at decision points,
working mode (git commits + mulch + seeds + quality gates) always on,
code-review after every sprint, improve-codebase-architecture every 3rd
sprint, self-reflection evolving the skill at sprint close/session end.
Sprint counter: `.iterate-state.json`. This loop is INDEPENDENT of the fabro
develop line (user directive 2026-10-03, branch shape amended
2026-10-17): it runs DIRECTLY ON `main` — no loop-lane branch, no PRs of
its own, no `--assignee fabro`; all commits via `git`, with `.seeds/` +
`.mulch/` staged in the same commit (never `mulch sync`), pushed to
`origin/main` through the push gate at every sprint close. Local-loop
skills live under
`.agents/skills/` (iterate + repo-local rust-style-guide copy); the loop
never loads skills from `.fabro/skills/` — that tree belongs to the
autonomous fabro line.
