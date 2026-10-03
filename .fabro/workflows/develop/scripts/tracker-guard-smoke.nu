#!/usr/bin/env nu
# Smoke test for tracker-guard.nu's PURE decision logic (fabro-0da8):
# guard-decision/seeds-issue-count over canned `complete`-style records —
# both-empty route, non-empty routes (open and in_progress arms), and
# the seeds-failure fail-open contract (non-zero exit, invalid JSON,
# success:false) — without shelling to seeds (the live-path is a manual
# invocation check, closeout-smoke pattern). The `source` const
# resolves against THIS file's directory, so the script runs from any
# cwd:
#   nu .fabro/workflows/develop/scripts/tracker-guard-smoke.nu

const GUARD = "tracker-guard.nu"
source $GUARD

def fail [what: string]: nothing -> nothing {
    print -e $"tracker-guard-smoke: FAIL — ($what)"
    exit 1
}

def ok-empty []: nothing -> record {
    {"exit_code": 0, "stdout": '{"success":true,"command":"list","issues":[]}'}
}

def ok-issues [n: int]: nothing -> record {
    let items = (1..$n | each {|i| ('{"id":"fabro-x' + ($i | into string) + '"}') } | str join ",")
    {"exit_code": 0, "stdout": ('{"success":true,"command":"list","issues":[' + $items + ']}')}
}

def label-of [r: record]: nothing -> string {
    $r.preferred_next_label?
    | default ""
}

# Route: BOTH lists empty -> "Tracker empty" (the zero-token exit).
let both = (guard-decision (ok-empty) (ok-empty))
if (label-of $both) != "Tracker empty" { fail $"both-empty misrouted: ($both | to json -r)" }
if $both.outcome != "succeeded" { fail "both-empty outcome not succeeded" }

# Route: open non-empty (in_progress empty) -> planner path.
let open_route = (guard-decision (ok-issues 2) (ok-empty))
if (label-of $open_route) != "Tracker non-empty" { fail $"open-non-empty misrouted: ($open_route | to json -r)" }

# Route: in_progress non-empty (open empty) -> planner path — the
# active-run shape (a just-claimed seed) must never take the empty exit.
let inprog_route = (guard-decision (ok-empty) (ok-issues 1))
if (label-of $inprog_route) != "Tracker non-empty" { fail $"in-progress-non-empty misrouted: ($inprog_route | to json -r)" }

# Fail-open: non-zero seeds exit (either call) -> planner path, succeeded.
let sd_dead = {"exit_code": 1, "stdout": "", "stderr": "boom"}
if (label-of (guard-decision $sd_dead (ok-empty))) != "Tracker non-empty" { fail "seeds open-failure not fail-open" }
if (label-of (guard-decision (ok-empty) $sd_dead)) != "Tracker non-empty" { fail "seeds in-progress-failure not fail-open" }

# Fail-open: invalid JSON stdout -> planner path.
let bad_json = {"exit_code": 0, "stdout": "not json at all"}
if (label-of (guard-decision $bad_json (ok-empty))) != "Tracker non-empty" { fail "invalid JSON not fail-open" }

# Fail-open: seeds-reported success:false -> planner path.
let sd_err = {"exit_code": 0, "stdout": '{"success":false,"error":"tracker locked"}'}
if (label-of (guard-decision (ok-empty) $sd_err)) != "Tracker non-empty" { fail "success:false not fail-open" }

# Edge: absent `issues` key is a legitimate empty list, not a failure.
let no_issues_key = {"exit_code": 0, "stdout": '{"success":true,"command":"list"}'}
if (label-of (guard-decision $no_issues_key (ok-empty))) != "Tracker empty" { fail "absent issues key not treated as empty" }

# seeds-issue-count unit checks: 0, n, -1 classes.
if (seeds-issue-count (ok-empty)) != 0 { fail "seeds-issue-count empty != 0" }
if (seeds-issue-count (ok-issues 3)) != 3 { fail "seeds-issue-count 3-issue != 3" }
if (seeds-issue-count $sd_dead) != -1 { fail "seeds-issue-count failure != -1" }
if (seeds-issue-count $bad_json) != -1 { fail "seeds-issue-count bad JSON != -1" }

print "tracker-guard-smoke: ok — routing, fail-open, and stale-claim requeue pure logic verified"

# --- Stale-claim requeue arm (fabro-d9f7), pure functions ---
let now = ("2026-09-19T09:00:00Z" | into datetime)

# seeds-issues: parse classes mirror seeds-issue-count (null on failure).
if (seeds-issues (ok-issues 2) | length) != 2 { fail "seeds-issues 2-issue != 2" }
if (seeds-issues $sd_dead) != null { fail "seeds-issues failure != null" }
if (seeds-issues $bad_json) != null { fail "seeds-issues bad JSON != null" }
if (seeds-issues $sd_err) != null { fail "seeds-issues success:false != null" }

# Claims carry fabro-32db terminality: terminal=true marks a claim by a
# provably terminal run (closeout journal) — not in flight, requeueable
# immediately; live claims keep the silence clock.
let claims = [
    {seed: "fabro-x1", ts: ("2026-09-18T00:00:00Z" | into datetime)}
    {seed: "fabro-x1", ts: ("2026-09-19T00:00:00Z" | into datetime)}
    {seed: "fabro-x2", ts: ("2026-09-19T08:00:00Z" | into datetime)}
    {seed: "fabro-x4", ts: ("2026-09-19T08:30:00Z" | into datetime), terminal: true}
    {seed: "fabro-x5", ts: ("2026-09-18T00:00:00Z" | into datetime), terminal: true}
    {seed: "fabro-x5", ts: ("2026-09-19T08:10:00Z" | into datetime)}
]
let s_old_claim = {id: "fabro-x1", updatedAt: "2026-09-19T08:59:00Z"}
let s_live_claim = {id: "fabro-x2", updatedAt: "2026-09-10T00:00:00Z"}
let s_no_claim = {id: "fabro-x3", updatedAt: "2026-09-19T02:00:00Z"}
let s_term_claim = {id: "fabro-x4", updatedAt: "2026-09-19T08:59:00Z"}
let s_term_plus_live = {id: "fabro-x5", updatedAt: "2026-09-01T00:00:00Z"}

# stale-claim-ids: stale old-claim requeued; live-claim seed kept even
# with a stale updatedAt; no-claim stale updatedAt requeued; TERMINAL
# claim requeued IMMEDIATELY even with a fresh ts (fabro-32db); a live
# claim alongside a terminal one governs (re-claimed seed stays); current
# seed NEVER requeued; exact-threshold age is NOT stale (strict >).
let seeds = [$s_old_claim, $s_live_claim, $s_no_claim, $s_term_claim, $s_term_plus_live]
let got = (stale-claim-ids $seeds $claims $now 6.0 "fabro-none")
if $got != ["fabro-x1" "fabro-x3" "fabro-x4"] { fail $"stale-claim-ids wrong set: ($got | to json -r)" }
if (stale-claim-ids ($seeds | append {id: "fabro-x1", updatedAt: "2026-09-01T00:00:00Z"}) $claims $now 6.0 "fabro-x1") != ["fabro-x3" "fabro-x4"] { fail "current seed not excluded" }
let boundary = {id: "fabro-x9", updatedAt: "2026-09-19T03:00:00Z"}
if (stale-claim-ids [$boundary] [] $now 6.0 "") != [] { fail "exact-threshold age must not be stale" }
# claims WITHOUT the terminal key (pre-fabro-32db shape) behave as live:
# the fresh 08:30 ts keeps fabro-x4-style claims unrequeueable.
let legacy_claims = [{seed: "fabro-x4", ts: ("2026-09-19T08:30:00Z" | into datetime)}]
if (stale-claim-ids [$s_term_claim] $legacy_claims $now 6.0 "") != [] { fail "terminal-absent claim must default live" }

# --- develop-claims mixed-schema regression (seeds-aa89) ---
# A journal mixing fabro-journal-v1 records with a foreign
# {stage,run_id,seed,note} line (the run 01M332792GNEPNR45GMEXV12WW
# shape) must yield a verdict, not a column_not_found error; the
# verdict runs over v1 records only (foreign lines never count as
# planner claims and never break the ts read). The foreign line sits
# BETWEEN two v1 records (mid-file), mirroring the original incident
# shape — one v1 planner record precedes it and the v1 closeout tail
# follows it (seeds-0511).
let mixed_dir = (mktemp -d)
let foreign_line = '{"stage":"planner","run_id":"01M332792GNEPNR45GMEXV12WW","seed":"seeds-t1","note":"foreign shape"}'
('{"$schema":"fabro-journal-v1","run_id":"01M332792GNEPNR45GMEXV12WW","node":"planner","visit":1,"status":"succeeded","ts":"2026-09-20T00:00:00Z","data":{"observations":["claimed seeds-t1"]}}' + "\n" | save --append $"($mixed_dir)/01M332792GNEPNR45GMEXV12WW.jsonl")
($foreign_line + "\n" | save --append $"($mixed_dir)/01M332792GNEPNR45GMEXV12WW.jsonl")
('{"$schema":"fabro-journal-v1","run_id":"01M332792GNEPNR45GMEXV12WW","node":"closeout","visit":1,"status":"succeeded","ts":"2026-09-20T00:05:00Z","data":{}}' + "\n" | save --append $"($mixed_dir)/01M332792GNEPNR45GMEXV12WW.jsonl")
# all-foreign journal: no planner v1 record -> no claims, no error.
($foreign_line + "\n" | save $"($mixed_dir)/01M33999000000000000000000.jsonl")
let mixed = (develop-claims $mixed_dir [seeds-t1, seeds-t2] "01M3NONE0000000000000000000")
if $mixed.degraded { fail "mixed-schema journal dir wrongly degraded" }
if ($mixed.claims | length) != 1 { fail $"mixed-schema claims wrong count: ($mixed.claims | to json -r)" }
if $mixed.claims.0.seed != "seeds-t1" { fail "mixed-schema claim wrong seed" }
if not $mixed.claims.0.terminal { fail "closeout-tipped v1 tail must be terminal" }
rm -r $mixed_dir

# --- Preflight in-flight arm (fabro-32db): terminal-tip? pure logic ---
# Sourcing planner-preflight.nu alongside tracker-guard.nu is safe: the
# smoke exits before either auto-invoked `def main` shells out.
const PREFLIGHT = "planner-preflight.nu"
source $PREFLIGHT

# closeout tip (either status) is terminal: the develop graph ended.
if not (terminal-tip? "01M2TEST" "fabro(01M2TEST): closeout (succeeded)" 0 60) { fail "closeout succeeded tip must be terminal" }
if not (terminal-tip? "01M2TEST" "fabro(01M2TEST): closeout (failed)" 0 60) { fail "closeout failed tip must be terminal" }
# squash-merge landing subject is terminal (the PR landed).
if not (terminal-tip? "01M2TEST" "fabro-274d: recover oversized findings (#138)" 0 60) { fail "squash-merge tip must be terminal" }
# af22 regression pin (fabro-32db): the orphaned terminal-failed branch
# tip of run 01M2WAY0KH — `reviewer (failed)` — is terminal past grace.
if not (terminal-tip? "01M2WAY0KHV8F0JB5GTW8TWHMW" "fabro(01M2WAY0KHV8F0JB5GTW8TWHMW): reviewer (failed)" 21600 60) { fail "stale failed tip (af22 shape) must be terminal" }
# a FRESH failed tip stays in-flight: the engine retries/bounces failed
# stages within minutes (run 01M2RJSBP8 evidence-failed -> tester).
if (terminal-tip? "01M2WAY0KHV8F0JB5GTW8TWHMW" "fabro(01M2WAY0KHV8F0JB5GTW8TWHMW): reviewer (failed)" 600 60) { fail "fresh failed tip must stay in-flight" }
# intermediate succeeded tips are mid-flight, any age.
if (terminal-tip? "01M2TEST" "fabro(01M2TEST): claim_check (succeeded)" 86400 60) { fail "intermediate succeeded tip must not be terminal" }
if (terminal-tip? "01M2TEST" "fabro(01M2TEST): reviewer (succeeded)" 86400 60) { fail "non-closeout succeeded tip must not be terminal" }
# a failed tip for a DIFFERENT run id never matches this run.
if (terminal-tip? "01M2OTHER" "fabro(01M2TEST): reviewer (failed)" 86400 60) { fail "other-run failed tip must not match" }
# unparseable subject: fail-open toward in-flight.
if (terminal-tip? "01M2TEST" "" 86400 60) { fail "empty subject must not be terminal" }
# grace boundary: exactly grace age is NOT terminal (strict >).
if (terminal-tip? "01M2TEST" "fabro(01M2TEST): reviewer (failed)" 3600 60) { fail "exact-grace failed tip must not be terminal" }

# Sourcing tracker-guard.nu imports its `def main`; nu auto-invokes it
# after the top level runs — exit explicitly so the smoke never shells
# to seeds (closeout-smoke idiom).
exit 0
