# Improve review — run 01M40KWWZ4M4P396PGCVH08Q6G

- workflow: develop
- branch integrated: this revisor pass (unmerged until approved)
- status: succeeded (1011s wall, $1.56 — revisor pass — reason and cost in run detail)
- generated: 2026-10-03 10:34+0000 by revisor `fabro_ask`

---

I checked the tracker first (`.seeds/issues.jsonl`): it holds exactly four seeds — `mulch-70ed` (closed by this run), `mulch-0e39`, `mulch-96ed`, `mulch-b0f7`. All four are product-feature seeds; none targets the develop workflow's own machinery, so most recommendations below carry new-seed justifications.

**Run grounding (from run events / stage outputs):** wall 1,011s, cost $1.56. Implementer = 827s wall (82% of run) and $1.399 (90% of cost), 98 tool calls (60 shell, 27 edit_file, 8 write_file, 2 read_file, 1 grep). Reviewer 124s / $0.112, 18 tool calls. Planner 47.6s / $0.048. Zero retries, zero gate bounces, first-pass approval — the graph's happy path works. All recommendations below come from friction the run nonetheless recorded.

---

**1. Deliver the full seed-work diff to the reviewer (highest impact: review correctness).**
- **Change:** in `workflow.fabro`, reviewer node: raise `x.preamble_output_max_lines=200` → ~2000 and `x.preamble_inline_max_kb=16` → ~64 (or graph `x.preamble_budget_kb=48` → 72) so a ~58 KB / ~1,700-line capture renders whole.
- **Evidence:** evidence@1 produced a 58,469-byte capture; reviewer@1 opened with "The evidence capture's inline diff was truncated (1488 lines omitted), so I verified against the working tree itself" and its journal painpoint asks exactly for this (naming stage evidence). The reviewer burned 12 `read_file` calls re-reading files the capture already contained, and the review's validity now depends on tree state matching the diff-base.
- **Expected effect:** reviews verify the base-pinned diff instead of mutable tree state; removes ~12 blob/tree round-trips per review (~+15k input tokens ≈ +$0.03, vs. the correctness gain).
- **New-seed justification:** preamble-attribute tuning of the develop graph's reviewer node; all four tracker seeds are product features, none covers workflow assets.

**2. Fix the dup-run-check line-dispatch false positive.**
- **Change:** in `.fabro/scripts/dup-run-check.nu`, add the `seeds: assign <id> @fabro` line-dispatch subject shape to the filed-only classifier (next to the fabro-a32f/fabro-0d48 patterns), and/or skip matches whose diff is confined to tracker paths (`.seeds/**`).
- **Evidence:** preflight@1 verdict `duplicate` for `mulch-70ed` on commit 7c396e6 ("seeds: assign mulch-70ed @fabro … (#6)") — a 1-line tracker-only change. Planner journal: "false positive in spirit … a future preflight arm could ignore commits touching only .seeds/"; implementer journal filed the same painpoint ("the two verdicts disagree mechanically").
- **Expected effect:** removes the planner's manual adjudication every time an assignment commit is the top base match, and removes the latent risk that a future planner trusts the verdict and routes "Already landed" on a live seed — the exact mechanical mis-close class the fabro-395b regression guarded for reopen/verify subjects.
- **New-seed justification:** dev-loop script (`.fabro/scripts/`); no product seed covers the develop preflight.

**3. Make the round-trip battery fail-visible when `ml` is absent (existing seed: `mulch-0e39`).**
- **Change:** in `crates/mulch/tests/reference_roundtrip.rs`, `reference_ml()` returning `None` currently `return`s with a stderr note; gate the skip behind an env assert (e.g. `MULCH_REQUIRE_ML=1` set by the `qualitygate` recipe makes the test panic instead of skip).
- **Evidence:** reviewer journal observation: "green gate does not prove the battery executed … worth a marker/env assert in the CLI-parity seed (mulch-0e39)."
- **Expected effect:** a green gate always proves the ADR-0023 acceptance battery actually ran — this is the integrity of the whole line's acceptance gate, not just this seed.
- **Seed:** `mulch-0e39` (open, blocked-by this run's seed — the reviewer explicitly routed it there).

**4. Add the stale-test-binary signature rule to the implementer's verification protocol.**
- **Change:** in `.fabro/workflows/develop/prompts/planner.md` (cost-tier/verification bullet) and `prompts/implementer.md`: after editing source, a suspiciously fast nextest "Finished" (0.01s) re-emitting pre-edit failure text means `touch <edited file>` (or `cargo clean -p <crate>`) and re-run — never debug the phantom failure.
- **Evidence:** implementer journal observation: "Stale-binary trap hit twice this pass … touching the changed file forced the rebuild and both 'failures' vanished" — two false-failure debug loops inside the implementer's 827s / $1.40 stage.
- **Expected effect:** saves 1–2 debug cycles per implementation pass (~1–3 min + tokens each) in the stage that is already 82% of wall and 90% of cost.
- **New-seed justification:** implementer/planner prompt rule; no tracker seed covers develop-loop prompts.

**5. Silence the 15 known-intentional prompt-lint warnings in the gate log.**
- **Change:** in `scripts/qualitygate.nu` (prompt-lint section), whitelist the deliberately routing-named top-level properties (`preferred_next_label`, `outcome`, `failure_reason`, `suggested_next_ids`, `context_updates`) for the workflow-owned schemas under `.fabro/workflows/*/schemas/`.
- **Evidence:** tester@1 output carries 15 identical `warn: … is routing-named` lines across `planner-output.schema.json`, `develop-output.schema.json`, `survey-output.schema.json`, then "prompt-lint: ok — 45 files, 15 warnings" — these schemas are routing-kind by design (graph comment, fabro-9ec3).
- **Expected effect:** green gate logs a human can scan; a real 16th warning can no longer hide in noise. Zero model cost (tester stage is 3.3s).
- **New-seed justification:** gate-script churn; no product seed covers `scripts/qualitygate.nu`.

**6. Enable terminal notifications for deadlock/soft-stop classes on the develop line.**
- **Change:** run settings `notifications.terminal` (currently `enabled: false`) — enable filtered to Deadlock/SoftStop terminal events only.
- **Evidence:** this run ended clean (PR #7 auto-created, auto-merge squash, no pending questions), so nothing was missed — but the only other failure-signal channel is the tracker_guard's stale-claim requeue at `stale_threshold_hours: 6.0` (from tracker_guard@1 output), i.e. a parked "Needs operator"/"Blocked" run would sit silent for up to six hours.
- **Expected effect:** a human learns within minutes when a future run parks, instead of via the 6-hour stale-claim sweep.
- **New-seed justification:** line-level run-settings change; tracker seeds are product features only.

**Not recommended (checked and rejected):** splitting the implementer stage or re-running the gate separately — the tester re-run cost only 3.3s warm (implementer warms the cache), and context usage peaked at 7.1% of the 1M window, so neither stage split nor context surgery has headroom worth a change in this run's shape.
