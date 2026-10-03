## PROJECT_FACTS — the repo-specific values this workflow runs on

This block is the ONE place the develop workflow carries facts about THIS
repository (the prompts that include this block stay project-agnostic).
Porting the workflow to another project means editing this file (plus the
workflow graph and settings), not the prompts. A stale value here is loop
friction: report it in the journal, never silently work around it.

- Primary code areas — where seed work normally lands: the Rust workspace
  under `crates/` (crate `crates/mulch`), plus this repo's own workflow
  assets under `.fabro/` when a seed explicitly targets the loop.
- Loop-asset paths — the dev loop's own machinery, hidden from FILE TOOLS by
  the per-node `fs_hide` envelope (ADR-0009 stage-envelope family in the
  fabro repo): `.fabro/`, `.seeds/`, `.mulch/`, `.agents/`, `scripts/`,
  `justfile`. FILE TOOLS (read_file, write_file, edit_file, glob discovery)
  fail on them — reads and writes both; the shell is unaffected (reads AND
  writes succeed through grep, sed, cat, python3 heredocs — the documented
  escape hatch). The `seeds`, `ml`, and `just` commands keep working through
  the shell.
- Repo wiring — visible, but never modify without the seed saying so
  explicitly: `AGENTS.md`, `docs/`, `Cargo.toml`, and the workspace
  manifests.
- Merge-target branch — the branch this seed loop's run PRs integrate
  into: `origin/main`. This repository has no upstream mirror; `main` IS
  the product line. Branch-sensitive checks (e.g. the implementer's
  duplicate-run preflight) must fetch and grep `origin/main`.
- Issue tracker: the `seeds` CLI (Seeds, git-native in `.seeds/`). The develop
  line works EXCLUSIVELY on seeds assigned to assignee `fabro` — the
  assignee is the ownership switch (see `AGENTS.md`). Seed ids carry the
  prefix `mulch-` (e.g. `mulch-70ed`). The supported read path is
  `seeds show <id> --format json`; never parse the raw tracker file
  (`.seeds/issues.jsonl`) by hand. Exact command reference (never invent
  flags):

| Command | Purpose |
|---|---|
| `seeds ready --assignee fabro --limit 200` | Unblocked open seeds ASSIGNED TO fabro — start here, and the ONLY candidate source: the develop line works exclusively on seeds the user assigned to fabro (assignee is the ownership switch, see `AGENTS.md`). If it answers the question, do NOT also run `seeds list`. ALWAYS pass `--limit 200`: the default limit 50 silently truncates lower-priority seeds out of the listing. |
| `seeds list --format json --assignee fabro --limit 200` | Full tracker picture, still filtered to fabro-assigned seeds only (only when `seeds ready` was not enough). Same limit rule as `seeds ready`. NEVER list without the `--assignee fabro` filter: unassigned or user-owned seeds are not the line's business. |
| `seeds show <id> --format json` | One seed in full (the supported path — never parse `.seeds/issues.jsonl` by hand). |
| `seeds update <id> --status in_progress --assignee fabro` | Claim (the exact claim form). Takes NO `--format` flag (observed failure: `unknown option '--format'`). |
| `seeds update <id> --description "<full corrected body>"` | Record a stale-spec correction when the basis RESOLVES but the seed's named path/target/details are wrong (see STALE-BASIS CHECK, step 3) — run it BEFORE the claim. `--description` replaces the body wholesale: re-emit the FULL corrected body including the existing `Basis:` line, appending/amending only the corrected facts. Takes NO `--format` flag. |
| `seeds close <id>` | NEVER yours — the deterministic Closeout step closes approved seeds — with exactly ONE exception: the planner's superseded-close `seeds close <id> --reason "superseded: fix landed in <sha>"` when a fix commit referencing the seed is already in base history and the acceptance criteria hold (reason string mandatory). Every other close form remains forbidden to every role. |

- Quality gate: `just qualitygate` — a `qualitygate` recipe in the project
  justfile. The workflow stays agnostic about what the gate checks.
- Rust toolchain pin (fmt/clippy run on the pinned nightly; tests run
  through `cargo nextest`):
  - `cargo +nightly-2026-09-22 fmt -p <touched-crate>`
  - `cargo +nightly-2026-09-22 clippy -p <touched-crate> --all-targets -- -D warnings`
- Toolchain image (shared-image posture): this repo owns NO Dockerfile —
  the `mulch-toolchain` server environment pins the shared
  `ghcr.io/denkhaus/seeds-toolchain:<sha12>` image, owned and built by
  denkhaus/seeds. An environment-level need (new tool, new pin) is a
  cross-repo change: it lands in the seeds repo's
  `.fabro/Dockerfile.toolchain` plus an environment repin, never here.
- Stage journal: `.fabro/journal/<run_id>.jsonl` — one JSON record per stage
  completion; the fallback source for recovering a run's claimed seed id
  (shell grep for the seed id prefix when a run goal names none).
- Engine credential surfaces (ADR-0019 review axis in the fabro repo): the
  engine injects `GITHUB_TOKEN` into every agent shell call and runs a git
  credential bridge (engine source: `services.rs` / `git_bridge.rs` in
  denkhaus/fabro) — agent-reachable capability that is invisible from the
  container environment alone.
- Loop lineage: `fabro-xxxx` citations in the prompts are evidence lore
  from the origin loop (denkhaus/fabro, tracker prefix `fabro-`); they
  resolve there, not in this repo's tracker. `seeds-xxxx` citations are
  lore from the sister loop (denkhaus/seeds, tracker prefix `seeds-`)
  this loop was cloned from — same rule: both resolve in their own
  trackers, never here.
