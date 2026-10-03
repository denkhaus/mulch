#!/usr/bin/env nu
# Smoke test for planner-preflight.nu's publish_blocked_risk arm
# (seeds-7e57): pure permission-resolution and flagging logic over
# canned `complete`-style records — BOTH branches proven (permission
# present -> no flag; permission absent -> flag), plus fail-open
# ("unknown" never flags) and the workflow-target detection. The live
# probe path (gh/git shelling) is a manual invocation check, per the
# tracker-guard-smoke pattern; the `source` const resolves against THIS
# file's directory, so the script runs from any cwd:
#   nu .fabro/workflows/develop/scripts/planner-preflight-smoke.nu

const PREFLIGHT = "planner-preflight.nu"
source $PREFLIGHT

def fail [what: string]: nothing -> nothing {
    print -e $"planner-preflight-smoke: FAIL — ($what)"
    exit 1
}

# --- permission-normalize: pass-through granted/absent, junk -> unknown
if (permission-normalize "granted") != "granted" { fail "normalize granted" }
if (permission-normalize " absent ") != "absent" { fail "normalize absent+whitespace" }
if (permission-normalize "GRANTED") != "granted" { fail "normalize case-fold" }
if (permission-normalize "") != "unknown" { fail "normalize empty" }
if (permission-normalize "maybe") != "unknown" { fail "normalize junk" }

# --- permission-from-probe: installation payload classes
let probe_write = {"exit_code": 0, "stdout": '{"permissions":{"workflows":"write"}}'}
let probe_read = {"exit_code": 0, "stdout": '{"permissions":{"workflows":"read"}}'}
let probe_none = {"exit_code": 0, "stdout": '{"permissions":{}}'}
let probe_dead = {"exit_code": 1, "stdout": "", "stderr": "403 Forbidden"}
let probe_junk = {"exit_code": 0, "stdout": "not json"}
if (permission-from-probe $probe_write) != "granted" { fail "probe write != granted" }
if (permission-from-probe $probe_read) != "absent" { fail "probe read != absent" }
if (permission-from-probe $probe_none) != "absent" { fail "probe no-workflows-key != absent" }
if (permission-from-probe $probe_dead) != "unknown" { fail "probe http-fail != unknown" }
if (permission-from-probe $probe_junk) != "unknown" { fail "probe bad-json != unknown" }

# --- workflow-target? detection: seeds-25b5/a9e7-style bodies
let wf_body = "Add a step to .github/workflows/ci.yml and .github/workflows/** targets"
if not (workflow-target? $wf_body) { fail "workflow body not detected" }
if (workflow-target? "touch crates/seeds/src/main.rs only") { fail "non-workflow body false-detected" }

# --- publish-blocked-risk: BOTH branches + fail-open
if not (publish-blocked-risk $wf_body "absent") { fail "absent+workflow must flag" }
if (publish-blocked-risk $wf_body "granted") { fail "granted must not flag (post-grant no blanket-block)" }
if (publish-blocked-risk $wf_body "unknown") { fail "unknown must not flag (fail-open)" }
if (publish-blocked-risk "no workflow paths here" "absent") { fail "non-workflow+absent must not flag" }

print "planner-preflight-smoke: ok — publish_blocked_risk both branches, fail-open, and detection verified"

# Sourcing planner-preflight.nu imports its `def main`; nu auto-invokes
# it after the top level runs — exit explicitly so the smoke never
# triggers a live preflight (closeout-smoke idiom).
exit 0
