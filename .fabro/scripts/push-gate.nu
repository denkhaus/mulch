#!/usr/bin/env nu
# Push gate for the local iterate/line-watch cycle (user directive
# 2026-09-17: NEVER push to main while a pass runs; seeds-e5af: make it a
# deterministic lefthook pre-push hook, and treat quota-parked runs as
# NON-blocking).
#
# Two conditions, both must hold for OPEN:
#   (a) NO active run on the production server (status.kind not in the
#       terminal set: succeeded/failed/canceled) — conductor, develop,
#       revisor, architect, anything. NOTE: fabro ps --json carries
#       status as {"kind": "..."} — a string compare against 'running'
#       silently passes while runs are active (the 2026-09-17 violation).
#       EXCEPTION (seeds-e5af): runs blocked for quota
#       (blocked/quota_rate_limit) are PARKED, not active — parked runs
#       must never permanently refuse pushes.
#   (b) NO open run-PR on the line repo.
#
# Exit 0 = GATE OPEN (push allowed). Exit 1 = REFUSED (prints why).
# Usage: nu .fabro/scripts/push-gate.nu [--server https://mirtuell.net]
#        [--repo denkhaus/mulch]

# Pure predicate: does this run record justify REFUSING a push?
# Exported for the fixture battery (.fabro/scripts/push-gate-fixtures.nu);
# `use` (unlike `source`) does not auto-invoke this file's `def main`.
export def is-refusing-run [r: record] {
    let kind = ($r.status?.kind? | default 'unknown')
    if $kind in ['succeeded' 'failed' 'canceled' 'cancelled'] {
        return false
    }
    # Quota park (seeds-e5af): blocked(quota_rate_limit) is not an active
    # run — the scheduler will resume it when quota resets. Any other
    # blocked reason stays refusing (unknown block causes are suspect).
    let reason = ($r.status?.reason? | default '')
    if $kind == 'blocked' and ($reason | str contains 'quota') {
        return false
    }
    true
}

def main [
    --server: string = 'https://mirtuell.net'
    --repo: string = 'denkhaus/mulch'
] {
    let ps = (do { ^fabro ps --server $server --json } | complete)
    if $ps.exit_code != 0 {
        print $"GATE REFUSED: fabro ps failed — ($ps.stderr | str substring 0..200)"
        exit 1
    }
    let runs = ($ps.stdout | from json)
    let active = ($runs | where {|r| is-refusing-run $r })
    let pr = (do { ^gh pr list --repo $repo --state open --json number --jq 'length' } | complete)
    let open_prs = (if $pr.exit_code == 0 {
        $pr.stdout | str trim | into int
    } else {
        999
    })
    if ($active | length) > 0 {
        print $"GATE REFUSED: ($active | length) active runs: ($active | each {|r| $r.run_id } | str join ', ')"
        if $open_prs > 0 { print $"also ($open_prs) open PRs" }
        exit 1
    }
    if $open_prs > 0 {
        print $"GATE REFUSED: ($open_prs) open run-PRs — after-merge window not reached"
        exit 1
    }
    print 'GATE OPEN: no active runs, no open PRs — push allowed'
}
