#!/usr/bin/env nu
# Deterministic tracker guard (fabro-0da8): sits AFTER start and BEFORE
# the planner (ahead of the preflight too — a drained tracker makes the
# already-landed check moot), proving tracker state in ~1-3 s of shell
# so the drained-tracker terminal case costs ZERO planner tokens.
#
# Basis (run 01M0SFEYVC9TD6MP816RHEBFQY): planner@1 spent 27.7 s wall /
# 25.4 s inference / $0.0186 solely to discover 0 open seeds — what two
# `seeds list` calls prove mechanically.
#
# Ownership rule (PROJECT_FACTS): BOTH calls filter `--assignee fabro
# --limit 200` — plain `seeds list` would count unassigned/user-owned seeds
# this line must never work. The empty-tracker exit fires only when no
# FABRO-ASSIGNED seed remains (open or in_progress). `seeds list` default
# output is OPEN issues only, which includes blocked-open seeds (per the
# seed body; `seeds ready` alone lists unblocked only and would misroute,
# fabro-aa3d) — hence open via the default listing, in_progress via
# `--status in_progress`.
#
# Routes (output_schema="routing" on the node):
#   - BOTH lists empty -> preferred_next_label "Tracker empty" — the
#     existing label contract already declared on the planner's exit
#     edge, wired here to the plain exit edge (natural completion: the
#     goal is achieved, NOT an error; same family as the planner's own
#     "Tracker empty" edge).
#   - either non-empty -> preferred_next_label "Tracker non-empty"
#     (unconditional edge to the preflight, then the planner unchanged).
#     This node produces NO context value — none is required downstream.
#
# FAIL-OPEN (annotated choice): if `seeds` errors, exits non-zero, or
# prints non-JSON output, the node routes "Tracker non-empty" (degraded
# mode) — it NEVER parks and NEVER fails the run: a broken tracker must
# not dead-end the dev loop, and the planner adjudicates the residue.
# Mirrors planner-preflight.nu's fail-open contract.
#
# Stale-claim requeue arm (fabro-d9f7): in_progress fabro-assigned seeds
# with NO live develop run claiming them, whose last claiming develop
# run's terminal activity (stage journal last ts) — or seeds updatedAt when
# no develop journal claims the seed — is older than the stale
# threshold, are reset to open via `seeds update <id> --status open
# --assignee fabro`. Orphaned claims stop pinning seeds outside
# `seeds ready` eligibility. Claim association reuses the preflight's
# journal-claims mechanism (seed ids on `"node":"planner"` journal
# lines — the documented PROJECT_FACTS fallback); a claiming journal
# fresher than the threshold means a live run owns the claim and the
# seed is never requeued. fabro-32db alignment: a claim whose claiming
# journal is provably TERMINAL (last record node `closeout` — the
# develop graph's terminal node) is NOT in flight and is requeueable
# IMMEDIATELY, no 6h wait; live (non-terminal) claims alone keep the
# reference clock. Runs whose terminality is unprovable (journal
# absent — e.g. a failed run's branch-only journal — or ending
# mid-flight) keep the 6h journal-silence clock / seeds updatedAt
# fallback.
# Fail-open on this arm too: degraded inputs (seeds
# failure, unreadable journal dir) skip requeueing entirely and report
# degraded — never a false requeue. Self-exclusion at BOTH grains: the
# current run's journal is skipped entirely (the engine pipes
# internal.run_id on stdin, planner-preflight idiom), and
# FABRO_GUARD_CURRENT_SEED names the claimed seed for manual runs.
# Requeued ids land in the output's `requeued` array; the
# outcome/preferred_next_label semantics above are unchanged.

# Stale threshold in hours. Default 6h: develop runs complete in
# minutes-to-hours (the loop cycles seeds continuously), so a claiming
# journal silent for 6h belongs to a terminal or abandoned run — its
# claim is orphaned. The brief's observable-today pair (fabro-29f7,
# fabro-6a78) sat terminal 9h/7.5h with no live run at implementation
# time, which a 24h default would not reach. Override:
# FABRO_GUARD_STALE_HOURS.
const STALE_HOURS_DEFAULT = 6.0

# Count issues in one `complete`-style seeds result; -1 marks failure
# (non-zero exit, invalid JSON, or success:false) for the fail-open path.
def seeds-issue-count [res: record] {
    if $res.exit_code != 0 { return (-1) }
    # nu's `from json` does NOT error on invalid input — it echoes the
    # raw string back — so the failure check is a type test, not try/catch.
    let parsed = (try { $res.stdout | from json } catch { null })
    if not ($parsed | describe | str starts-with "record") { return (-1) }
    if ($parsed.success? | default true) == false { return (-1) }
    ($parsed.issues? | default [] | length)
}

# Parse one `complete`-style seeds list result into its issues table;
# null marks failure (same classes as seeds-issue-count's -1).
def seeds-issues [res: record] {
    if $res.exit_code != 0 { return null }
    let parsed = (try { $res.stdout | from json } catch { null })
    if not ($parsed | describe | str starts-with "record") { return null }
    if ($parsed.success? | default true) == false { return null }
    ($parsed.issues? | default [])
}

# Pure decision over the two seeds results (smoke-testable without
# shelling): {exit_code, stdout} records -> routing record.
def guard-decision [open_res: record, inprog_res: record] {
    let open_n = (seeds-issue-count $open_res)
    let inprog_n = (seeds-issue-count $inprog_res)
    if $open_n < 0 or $inprog_n < 0 {
        # Fail-open: degraded mode routes the preflight/planner normally.
        {"outcome": "succeeded", "preferred_next_label": "Tracker non-empty"}
    } else if $open_n == 0 and $inprog_n == 0 {
        {"outcome": "succeeded", "preferred_next_label": "Tracker empty"}
    } else {
        {"outcome": "succeeded", "preferred_next_label": "Tracker non-empty"}
    }
}

# Pure stale-claim decision (smoke-testable): the ids of in_progress
# seeds that are NOT the current run's seed and whose claim is not kept
# alive by a LIVE run. `claims` is a table of {seed, ts, terminal?} —
# terminal marks a claim by a provably terminal run (fabro-32db): such
# a claim is NOT in flight and is stale IMMEDIATELY (a closeout journal
# must never pin the seed past its run's death). When at least one LIVE
# claim exists, the live claims' freshest ts is the reference clock;
# when no develop journal claims the seed at all, seeds updatedAt stands
# in (claim with a lost/absent journal). A live claiming journal fresher
# than the threshold means a live run owns the claim and the seed is
# never listed — never requeue it.
def stale-claim-ids [seeds: list, claims: list, now: datetime, stale_hours: float, current_seed: string]: nothing -> list<string> {
    $seeds
    | where {|s| $s.id != $current_seed }
    | where {|s|
        let mine = ($claims | where seed == $s.id)
        let live = ($mine | where {|c| not ($c.terminal? | default false)})
        if ($mine | is-empty) {
            # no claiming journal: seeds updatedAt is the claim clock
            ($now - ($s.updatedAt | into datetime)) > ($stale_hours * 1hr)
        } else if ($live | is-empty) {
            # only terminal claims: orphaned NOW, no silence wait
            true
        } else {
            let ref = ($live | get ts | math max)
            ($now - $ref) > ($stale_hours * 1hr)
        }
    }
    | get id
}

# Scan the stage journals for develop-run claims on the given seeds. A
# develop-run journal is one whose records include a `planner` node
# (revisor/architect journals use other node sets and never claim
# develop seeds); it CLAIMS every seed id appearing on its planner-node
# lines (planner-preflight journal-claims mechanism — incidental ids on
# other nodes never count). The invoking run's own journal is skipped
# entirely (self-run exclusion: its planner observations routinely name
# in_progress siblings it merely considered). Returns {claims,
# degraded}: claims is a table of {seed, ts} (journal's last record ts
# per claimed seed); degraded=true means the journal dir was unreadable
# — callers must NOT requeue.
def develop-claims [journal_dir: string, seed_ids: list, self_run: string]: nothing -> record {
    let files = (try { ls $"($journal_dir)/*.jsonl" | get name } catch { [] })
    if ($files | is-empty) {
        # An absent/unreadable journal dir is degraded (the loop always
        # has journals); an empty-but-present dir cannot be told apart
        # here, so fail-open: no requeue this pass.
        return {claims: [], degraded: true}
    }
    let claims = ($files | each {|f|
        let stem = ($f | path basename | path parse | get stem)
        if $stem == $self_run { return [] }
        let content = (open --raw $f)
        let recs = ($content | lines | each {|l| try { $l | from json } catch { null } } | where {|r| $r != null })
        # seeds-aa89: journals may carry foreign non-v1 records (no
        # ts/node columns — e.g. a hand-appended {stage,run_id,seed,note}
        # line). The verdict runs over v1 records ONLY: filter to
        # records carrying BOTH ts and node before any column access, so
        # mixed-schema journals never fail the stage.
        let v1 = ($recs | where {|r| ($r.ts? != null) and ($r.node? != null) })
        if ($v1 | is-empty) { return [] }
        if not ($v1 | any {|r| $r.node == "planner" }) { return [] }
        let planner_text = ($v1 | where {|r| $r.node == "planner" } | to json)
        let last_ts = ($v1 | get ts | last)
        # fabro-32db: terminal = the journal's last v1 record is the
        # workflow's terminal node (closeout) — the run completed its
        # graph, so its claims are not in flight. A journal ending at an
        # intermediate node is live-or-crashed: unprovable, keeps the
        # silence clock.
        let terminal = (($v1 | last | get -o node? | default "") == "closeout")
        $seed_ids | each {|sid|
            if ($planner_text | str contains $sid) { {seed: $sid, ts: ($last_ts | into datetime), terminal: $terminal} }
        } | flatten
    } | flatten)
    {claims: $claims, degraded: false}
}

def main [--dry-run (-d)]: nothing -> nothing {
    # Non-tty stdin (same nu 0.115 constraint as planner-preflight.nu):
    # the engine pipes internal.run_id, read it through external cat.
    let self_run = (cat | str join | str trim)
    let now = (date now)
    let stale_hours = ($env.FABRO_GUARD_STALE_HOURS? | default $STALE_HOURS_DEFAULT | into float)
    # Self-exclusion, seed grain (fabro-d9f7): never requeue the current
    # run's claimed seed. At guard time this run has not claimed yet
    # (the planner claims downstream), but the env makes the contract
    # explicit and covers manual mid-run invocations.
    let current_seed = ($env.FABRO_GUARD_CURRENT_SEED? | default "")

    let open_res = (do { seeds list --format json --assignee fabro --limit 200 } | complete)
    let inprog_res = (do { seeds list --format json --status in_progress --assignee fabro --limit 200 } | complete)
    let base = (guard-decision $open_res $inprog_res)

    # Stale-claim requeue arm. Fail-open: any degraded input (seeds failure
    # or unreadable journals) skips requeueing entirely.
    let inprog_seeds = (seeds-issues $inprog_res)
    let seed_ids = ($inprog_seeds | default [] | get -o id | default [])
    let scan = (develop-claims ".fabro/journal" $seed_ids $self_run)
    let arm_degraded = (($inprog_seeds == null) or $scan.degraded)

    let decisions = (if $arm_degraded {
        []
    } else {
        (stale-claim-ids $inprog_seeds $scan.claims $now $stale_hours $current_seed)
    })

    let applied = (if $dry_run or ($decisions | is-empty) {
        {requeued: (if $dry_run { $decisions } else { [] }), failed: []}
    } else {
        let results = ($decisions | each {|sid|
            let r = (do { seeds update $sid --status open --assignee fabro } | complete)
            {sid: $sid, ok: ($r.exit_code == 0)}
        })
        {requeued: ($results | where ok | get sid), failed: ($results | where ok == false | get sid)}
    })

    $base
    | merge {
        requeued: $applied.requeued,
        requeue_failed: $applied.failed,
        dry_run: $dry_run,
        degraded: $arm_degraded,
        stale_threshold_hours: $stale_hours
    }
    | to json --raw
    | print
}
