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
- No toolchain image recipe here: runs use the SHARED
  `ghcr.io/denkhaus/seeds-toolchain` image (owned by denkhaus/seeds)
  through the server-managed `mulch-toolchain` environment. Environment-
  level changes land in the seeds repo, never here.

The Rust toolchain is owned by rustup (pinned `nightly-2026-09-22`);
mise owns just/bun/nushell/ripgrep and the bootstrap-phase jayminwest
ml CLI. The tracker is the EXTERNAL `seeds` binary (denkhaus/seeds,
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

- **Session start:** run `ml prime`.
- Before finishing a task, record durable insights (`ml record <domain>
  --type <convention|pattern|failure|decision|reference|guide>
  --description "..."`); skip when nothing surfaced. Upserts by `--name`
  merge outcomes — amend the existing record instead of filing a second
  one for the same lesson.
- **Under GitButler NEVER run `ml sync`** (it issues its own plain git
  commit behind the workspace's back) — commit `.mulch/` changes
  explicitly via `but commit` in the same batch.

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

## Version control (GitButler experiment, user directive 2026-10-03)

This checkout is a GitButler workspace (sits on `gitbutler/workspace`).
The TARGET is `origin/main` — main IS the product line and stays the
merge target of run PRs. Local work lives on APPLIED VIRTUAL BRANCHES
(one per agent/line); gitbutler refuses applying the target itself, so
no virtual branch is ever named `main`.

- **All write operations go through `but`** (commit, push, branch,
  history edit, PR). Never run `git add/commit/push/checkout/merge/
  rebase/stash/cherry-pick` here. Read-only git inspection (`git log`,
  `git show`, `git diff`, `git fetch`) stays allowed.
- `but status` replaces `git branch --show-current` — the workspace is
  healthy when the status parses and no commit is conflicted; any
  number of applied virtual branches is normal (multi-agent).
- **Landing local work:** `but push <branch>`, then a PR into `main` —
  never a direct push to main. Until the operator completes GitButler
  forge auth (`but config forge auth`, interactive), the PR leg runs
  through `gh` (`gh pr create --head <branch> --base main` +
  `gh pr merge <n> --auto --squash` — the fabro-experiment precedent);
  with forge auth, `but pr new` / `but pr auto-merge` take over. Run PRs
  land on main the engine way; the local workspace follows with
  `but pull` (integrates the new target state, rebases applied
  branches, removes merged-upstream ones).
- Update a specific virtual branch from its own remote with
  `but branch update <name>`; fetch target state before reading
  tracker state when another machine may have moved it (the tracker
  view is branch-local). Never apply/unapply/update branches or pull
  while a background cargo runs (the tree is rewritten).
- One-time bootstrap exception (root commit `235a70d`): gitbutler needs
  an origin HEAD before `but setup` can set its target, so the EMPTY
  root commit was created and pushed via plain git once — no content
  ever lands that way again.
- The experiment exists to run several agents as separate virtual
  branches/stacks in one workspace WITHOUT git worktrees. Report
  friction in the journal so the go/no-go stays factual. If the
  experiment fails, revert the but-specific procedures to git wording.

## Push gate (two layers)

`but push` does NOT run git pre-push hooks — the gate is a COMMAND run
explicitly, unpiped, in the same cell as the push:

1. `nu .fabro/scripts/push-gate.nu` (exit 0 = open: no running
   conductor/develop/revisor pass on https://mirtuell.net AND no open
   run-PR on denkhaus/mulch; quota-parked runs do not refuse), then
2. `but push <branch>` (the virtual branch you are landing).

Never pipe the gate (`gate | tail && but push` reads the pipe member's
exit code, not the gate's). `lefthook.yml` additionally wires the same
gate as a git pre-push hook for plain-git pushes (operator escape
`git push --no-verify` stays documented); operators activate it once per
checkout with `lefthook install`.

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
