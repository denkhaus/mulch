#!/usr/bin/env nu
# Imported from denkhaus/fabro .fabro/scripts/friction-score.nu (operator
# order 2026-09-22); single adaptation: `sd list` -> `seeds list` (this repo
# self-hosts the seeds binary; sd is only the reference, not on PATH).
# friction-score.nu — deterministic 0.0–1.0 systemic-friction score for the
# dev loop, computed from tracker (seed) and journal (run) metrics.
# Consumed as a trigger condition by the architect workflow (and readable
# by humans/line-watch). No LLM judgment anywhere.
#
# Usage:
#   nu .fabro/scripts/friction-score.nu [--days 7] [--threshold 0.6]
#
# Components (each 0.0–1.0, window = --days, default 7):
#   D drain-deficit   = 1 - min(1, seed_closes / max(1, seed_creates))  [tracker closedAt/createdAt]
#   B backlog-stag    = min(1, median_open_age_days / 14)              [open seeds, @fabro queue]
#   S starvation      = 1 - min(1, closes / max(1, develop_runs))       [idle capacity: runs closing nothing]
#   P stuck-claims    = open in_progress seeds older than 2 days / open [the fabro-3c9d class]
#   score = 0.35·D + 0.25·B + 0.20·S + 0.20·P
#   verdict: <0.30 normal | 0.30–0.60 grind (watch) | >=0.60 architecture-due
#
# Output: one JSON object (score, verdict, components, window, computed_at).
# Exit 0 always (a score is a measurement, not a pass/fail).

def iso-age-days [iso: string] {
    let dt = ($iso | into datetime)
    ((date now) - $dt) / 1day
}

def cap1 [x] {
    if $x > 1.0 { 1.0 } else { $x }
}

def main [--days: int = 7, --threshold: float = 0.6] {
    let open_seeds = (do { seeds list --limit 1000 --format json } | complete
        | get stdout | from json | get issues? | default [])
    let closed_seeds = (do { seeds list --status closed --limit 1000 --format json } | complete
        | get stdout | from json | get issues? | default [])
    let all = ($open_seeds | append $closed_seeds)

    let window_start = ((date now) - ($days * 24hr))

    # tracker events in window
    let creates_in_w = ($all | where {|i|
        let ts = ($i.createdAt? | default "")
        (not ($ts | is-empty)) and (($ts | into datetime) > $window_start) } | length)
    let closes_in_w = ($closed_seeds | where {|i|
        let ts = ($i.closedAt? | default "")
        (not ($ts | is-empty)) and (($ts | into datetime) > $window_start) } | length)

    # D: drain deficit
    let denom = (if $creates_in_w > 0 { $creates_in_w } else { 1 })
    let drain = (($closes_in_w | into float) / ($denom | into float))
    let d = (1.0 - (cap1 $drain))

    # B: backlog stagnation — median open age (fabro-assigned queue)
    let fabro_open = ($open_seeds | where {|i| ($i.assignee? | default "" | str trim -l -c '@') == "fabro" })
    let ages = ($fabro_open | each {|i|
        let ts = ($i.createdAt? | default "")
        if ($ts | is-empty) { null } else { iso-age-days $ts }
    } | compact | sort)
    let median_age = (if ($ages | is-empty) { 0.0 } else {
        let n = ($ages | length)
        ($ages | get ((($n - 1) / 2) | math floor))
    })
    let b = (cap1 ($median_age / 14.0))

    # develop runs in window: develop-signature journals (a planner node
    # record) whose start ts falls inside the window; revisor/conductor
    # journals lack the planner signature
    let journal_files = (glob .fabro/journal/*.jsonl)
    mut develop_runs = 0
    for jf in $journal_files {
        let raw = (open --raw $jf | lines | compact)
        if ($raw | is-empty) { continue }
        if not ($raw | any {|l| ($l | str contains '"node":"planner"') }) { continue }
        let ts = (try { ($raw | first | from json | get ts? | default "") } catch { "" })
        if ($ts | is-empty) { continue }
        let ok = (try { ($ts | into datetime) > $window_start } catch { false })
        if $ok {
            $develop_runs = $develop_runs + 1
        }
    }

    # S: starvation — closes per develop run (1 seed per run is the cadence)
    let rdenom = (if $develop_runs > 0 { $develop_runs } else { 1 })
    let close_rate = (($closes_in_w | into float) / ($rdenom | into float))
    let s = (1.0 - (cap1 $close_rate))

    # P: stuck claims — in_progress older than 2 days
    let stuck = ($open_seeds | where {|i|
        let ts = ($i.updatedAt? | default "")
        ($i.status? | default "") == "in_progress" and (not ($ts | is-empty)) and ((iso-age-days $ts) > 2.0)
    } | length)
    let open_n = ($open_seeds | length)
    let p = (if $open_n == 0 { 0.0 } else { cap1 (($stuck | into float) / ($open_n | into float)) })

    let score = ((0.35 * $d) + (0.25 * $b) + (0.20 * $s) + (0.20 * $p))
    let verdict = (if $score >= $threshold {
        "architecture-due"
    } else if $score >= 0.30 {
        "grind"
    } else {
        "normal"
    })

    {score: ($score | math round --precision 3),
     verdict: $verdict,
     threshold: $threshold,
     components: {
        drain_deficit: ($d | math round --precision 3),
        backlog_stagnation: ($b | math round --precision 3),
        starvation: ($s | math round --precision 3),
        stuck_claims: ($p | math round --precision 3)},
     inputs: {
        creates_in_window: $creates_in_w,
        closes_in_window: $closes_in_w,
        develop_runs_in_window: $develop_runs,
        median_open_age_days: ($median_age | math round --precision 1),
        stuck_in_progress: $stuck,
        open_total: $open_n},
     window_days: $days,
     computed_at: (date now | format date "%Y-%m-%dT%H:%M:%SZ")} | to json --raw | print
}
