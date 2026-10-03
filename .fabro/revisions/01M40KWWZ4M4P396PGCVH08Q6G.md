# Revision — run 01M40KWWZ4M4P396PGCVH08Q6G

- status reviewed: succeeded (1011s wall, $1.56 — first-pass approval, happy path)
- review: .fabro/reviews/develop/01M40KWWZ4M4P396PGCVH08Q6G.md
- seeds filed: mulch-4da8 — Enable terminal notifications for Deadlock/SoftStop parkings on the develop line (needs-user, exempt from balance: engine run-settings change the line cannot implement)
- balance: 0 non-exempt seeds filed / 0 — no credit this pass (no stale or superseded closes; the three open product seeds are out of scope of these findings)
- basis: run 01M40KWWZ4M4P396PGCVH08Q6G, workflow version absent, commit f99626c4792a46666c7cfcfb9f1d1f65efe155b1
- revised_at_commit: f99626c4792a46666c7cfcfb9f1d1f65efe155b1 (ADR-0015: engine drift signal for later judgement)

## Findings

Dedupe: overflow ledger `open` was empty at pass start; `seeds search` on each
theme (preamble, dup-run-check, roundtrip, stale, qualitygate, notifications)
matched nothing. Zero balance credit, so four surviving findings ride as
machine-visible overflow entries for the next pass.

### 1. Deliver the full seed-work diff to the reviewer — overflow to journal

- overflow: Deliver full seed-work diff to reviewer — in `workflow.fabro` reviewer node raise `x.preamble_output_max_lines` 200→~2000 and `x.preamble_inline_max_kb` 16→~64 (or graph `x.preamble_budget_kb` 48→72); effect: reviews verify the base-pinned diff instead of mutable tree state, removing ~12 re-read round-trips per review (evidence@1 capture was 58,469 bytes, 1488 lines truncated).

### 2. Fix the dup-run-check line-dispatch false positive — overflow to journal

- overflow: Fix dup-run-check line-dispatch false positive — add the `seeds: assign <id> @fabro` subject shape to the filed-only classifier in `.fabro/scripts/dup-run-check.nu` and/or skip matches whose diff is confined to `.seeds/**`; effect: removes planner manual adjudication on assignment-commit top matches and the latent mis-close risk (preflight@1 verdict `duplicate` for `mulch-70ed` on tracker-only commit 7c396e6).

### 3. Round-trip battery fail-visible when `ml` absent — duplicate_of: mulch-0e39

- duplicate_of: mulch-0e39 — the change (gate the `reference_ml()` skip behind an env assert, e.g. `MULCH_REQUIRE_ML=1` in the `qualitygate` recipe, in `crates/mulch/tests/reference_roundtrip.rs`) sits inside the open CLI-parity seed's scope; the reviewer explicitly routed it there. No new seed, no close.

### 4. Stale-test-binary signature rule in verification protocol — overflow to journal

- overflow: Stale-test-binary signature rule — in `.fabro/workflows/develop/prompts/planner.md` and `prompts/implementer.md` verification bullets: a suspiciously fast nextest "Finished" (0.01s) re-emitting pre-edit failure text means `touch <edited file>` and re-run, never debug the phantom failure; effect: saves 1–2 false-failure debug cycles per implementer pass (hit twice this run inside the 827s/$1.40 stage).

### 5. Silence the 15 intentional prompt-lint warnings — overflow to journal

- overflow: Silence intentional prompt-lint warnings — in `scripts/qualitygate.nu` whitelist the routing-named top-level properties (`preferred_next_label`, `outcome`, `failure_reason`, `suggested_next_ids`, `context_updates`) for workflow-owned schemas under `.fabro/workflows/*/schemas/`; effect: scannable green-gate log where a real 16th warning cannot hide in noise (tester@1: 15 identical warnings, "ok — 45 files, 15 warnings").

### 6. Terminal notifications for Deadlock/SoftStop — filed mulch-4da8

- Filed as `mulch-4da8` (labels needs-user,revision): engine run-settings change (`notifications.terminal` enabled:false → filtered Deadlock/SoftStop), not implementable by the develop line; parked runs currently sit silent up to the 6h stale-claim sweep.
