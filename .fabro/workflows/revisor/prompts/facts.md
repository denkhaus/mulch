## FACTS — the loop and repo values this revisor runs on

This block is the ONE place the revisor prompts carry facts about THIS
repository and its loop tooling (ADR-0013 pattern; the stage prompts stay
project-agnostic). Porting the loop means editing this file plus the
workflow graph, not the prompts. A stale value here is loop friction:
report it in the journal, never silently work around it.

- Issue tracker — the `seeds` CLI (Seeds, git-native in `.seeds/`). Seed ids
  carry the prefix `mulch-` (e.g. `mulch-70ed`); the supported read path
  is `seeds show <id> --format json`. `seeds search` matches title/description
  text only and is AND-strict: use ONE keyword per query (broaden by
  dropping words), and use `seeds show` — never `seeds search` — for id
  lookups.
- Merge-target branch — `origin/main`: the branch the line's run PRs
  integrate into. The duplicate-run preflight
  (`nu .fabro/scripts/dup-run-check.nu <seed-id> --self <run-id>`) greps exactly this
  branch; it defaults to it. The `--self` argument is the id of the run
  whose OWN PR counts as first-party — pass the SUBJECT run's id (for the
  revisor: `revisor_target_run_id`; for the implementer: its own run id),
  never the consuming session's id.
- Stage journal — `.fabro/journal/<run_id>.jsonl`: the fallback source for
  the seed id a reviewed run claimed (shell grep for the `mulch-` prefix
  when the run's goal names none).
- Engine-provided credentials (ADR-0019 review axis): the engine injects
  `GITHUB_TOKEN` into every agent shell call (`resolve_workflow_env` in
  `lib/components/fabro-workflow/src/services.rs`) and runs a git
  credential bridge (`lib/components/fabro-workflow/src/git_bridge.rs`) —
  agent-reachable capability that is invisible from the container
  environment alone.
