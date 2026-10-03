#!/usr/bin/env nu
# Fixture battery for push-gate.nu's pure status filter (seeds-e5af).
# Wired into the qualitygate battery list; `use` imports the predicate
# without auto-invoking the gate's `def main`.
use ./push-gate.nu is-refusing-run

def expect [label: string, got: bool, want: bool] {
    if $got != $want {
        print $"push-gate fixture FAILED: ($label) — got ($got), want ($want)"
        exit 1
    }
}

# Running runs REFUSE.
expect 'running run refuses' (is-refusing-run {run_id: 'r1', status: {kind: 'running'}}) true
expect 'starting run refuses' (is-refusing-run {run_id: 'r2', status: {kind: 'starting'}}) true

# Quota-parked runs do NOT refuse (seeds-e5af: parked != active).
expect 'quota-parked run does not refuse' (is-refusing-run {run_id: 'r3', status: {kind: 'blocked', reason: 'quota_rate_limit'}}) false

# Terminal runs never refuse.
expect 'succeeded does not refuse' (is-refusing-run {run_id: 'r4', status: {kind: 'succeeded'}}) false
expect 'failed does not refuse' (is-refusing-run {run_id: 'r5', status: {kind: 'failed'}}) false
expect 'canceled does not refuse' (is-refusing-run {run_id: 'r6', status: {kind: 'canceled'}}) false

# Other blocked reasons still refuse; unknown statuses refuse (fail-safe).
expect 'blocked-other-reason refuses' (is-refusing-run {run_id: 'r7', status: {kind: 'blocked', reason: 'dependency_failed'}}) true
expect 'blocked-no-reason refuses' (is-refusing-run {run_id: 'r8', status: {kind: 'blocked'}}) true
expect 'missing status refuses (fail-safe)' (is-refusing-run {run_id: 'r9'}) true

print 'push-gate-fixtures: ok — running refuses, quota-park does not, terminals do not, unknown fails safe'
