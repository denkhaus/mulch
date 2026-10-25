#!/usr/bin/env nu
# Run a workflow end to end and integrate the result
# (thin launcher: `just run <workflow>`).
#
# Pipeline:
#   1. guard: warn on uncommitted changes (the ff-pull later needs a
#      clean tree)
#   2. fabro create <workflow> [--goal] --json   -> run id
#      (or --adopt <run-id> to resume with an existing run)
#   3. fabro start <id> + fabro attach <id>      -> live output
#   4. fabro wait <id> --json                    -> status/reason truth
#   5. integrate: GitHub auto-merge lands the run PR on origin/main
#      (squash), the checkout follows with `git pull --ff-only`:
#        - post the required `lab-check` status LOCALLY on the run branch
#          head (variant B, fabro-ab2c: no Actions runner involved)
#        - detection is TREE EQUALITY (the squash means the run-branch
#          tip is never an ancestor)
#
# PLAIN-GIT POSTURE (post-GitButler, user directive 2026-10-17 —
# mulch-e2df): this checkout sits on `main`; the TARGET is origin/main
# (the merge target of run PRs). Integration is fetch + ff-pull only —
# main keeps required-linear-history, so a diverged main is a loud
# manual path, never a merge or rebase here.
#
# Production server is mirtuell (the local 127.0.0.1 server is for
# TESTS only — fabro repo posture). Runs execute on the server-managed
# `mulch-toolchain` environment (shared seeds-toolchain image).
#
# Any failure prints an ALARM block and exits 1.
#
# External calls follow the repo style (qualitygate.nu): literal external
# commands in `do { ^cmd ... } | complete`, exit_code checked. No generic
# arg-spread wrapper — rest params would swallow --flags meant for the
# external command.

const SERVER_DEFAULT = 'https://mirtuell.net'
const GITHUB_REPO = 'denkhaus/mulch'  # lab world repo (variant B status posts)

# Terminal failure: loud ALARM block on stderr, exit 1.
def fail [msg: string]: nothing -> nothing {
    print -e ''
    print -e $"╔══ ALARM: ($msg) ══╗"
    print -e '╚══════════════════════════════════════════════════════════════╝'
    exit 1
}

# Check an external result; on failure exit via `fail` with stderr detail.
def ok [result: record, what: string]: nothing -> record {
    if $result.exit_code != 0 {
        let detail = ($result.stderr | str trim | default $result.stdout | str trim)
        fail $"($what) failed: ($detail)"
    }
    $result
}

def main [
    workflow: string = 'develop'  # workflow name (as `fabro run` accepts)
    --goal (-g): string           # optional goal override (name the seed!)
    --branch (-b): string         # base branch; the line branch `main`
    --adopt (-a): string          # adopt an existing run id (skip create/start)
    --timeout-min (-t): int = 90  # minutes to wait for the run
    --environment (-e): string = 'mulch-toolchain'  # server-managed environment
                                         # (shared seeds-toolchain image, reuse
                                         # posture; see AGENTS.md)
]: nothing -> nothing {
    let fabro_bin = ($env.FABRO_BIN? | default $"($env.HOME)/.fabro/bin/fabro")
    let server = ($env.FABRO_SERVER? | default $SERVER_DEFAULT)
    let base_branch = (if ($branch | is-empty) { 'main' } else { $branch })  # the ONLY base: main (run PRs land there)

    # ── 1. guard: a clean tree (the ff-pull later needs one) ──
    let dirty = (do { git status --porcelain } | complete)
    ok $dirty 'git status'
    if (($dirty.stdout | lines | compact | is-not-empty)) {
        print 'run_workflow: WARN uncommitted changes present — the later ff-pull refuses on divergence; consider committing first'
    }
    print $"run_workflow: target=origin/($base_branch) workflow=($workflow) (plain git on ($base_branch))"

    # ── 2. create (or adopt) ─────────────────────────────────────────
    let run_id = (if not ($adopt | is-empty) {
        print $"run_workflow: adopting run ($adopt)"
        $adopt
    } else {
        let created = (if ($goal | is-empty) {
            do { ^$fabro_bin create $workflow --environment $environment --json --server $server } | complete
        } else {
            do { ^$fabro_bin create $workflow --goal $goal --environment $environment --json --server $server } | complete
        })
        ok $created 'fabro create'
        let id = ($created.stdout | from json | get -o run_id | default '')
        if ($id | is-empty) {
            fail $"fabro create returned no run_id: ($created.stdout | str trim)"
        }
        $id
    })
    print $"run_workflow: run id ($run_id)"

    # ── 3. start + attach (live output) ──────────────────────────────
    if ($adopt | is-empty) {
        let started = (do { ^$fabro_bin start $run_id --server $server } | complete)
        ok $started 'fabro start'
    }
    # attach streams live output; a non-zero exit is expected on failed
    # runs and is NOT the verdict — step 4 owns that decision.
    try { ^$fabro_bin attach $run_id --server $server }

    # ── 4. wait: the status truth ────────────────────────────────────
    let timeout_sec = ($timeout_min * 60)
    let waited = (do {
        ^$fabro_bin wait $run_id --json --timeout $timeout_sec --server $server
    } | complete)
    # fabro wait may exit non-zero when the RUN failed while still
    # emitting the terminal status JSON — that is the verdict we want
    # below, not an empty wrapper alarm. Only a result without parseable
    # JSON is a tool failure.
    let info = (try { $waited.stdout | from json } catch { null })
    if $info == null {
        ok $waited 'fabro wait'
    }

    let status = ($info | get -o status | default '')
    let reason = ($info | get -o reason | default '')
    print $"run_workflow: terminal status ($status) ($reason)"
    if $status != 'succeeded' {
        fail $"run ($run_id) ended ($status) ($reason) — nothing integrated; inspect ($server)/runs/($run_id)"
    }

    # Orchestration runs (conductor) disable PRs: no PR means nothing to
    # auto-merge — skip the integration wait instead of polling a merge
    # that cannot happen (the journal stays on the run branch; the UI
    # and Slack carry the outcomes).
    let has_pr = (($info | get -o pull_request | default null) != null)
    if not $has_pr {
        print "run_workflow: orchestration run - no PR - nothing to integrate"
        exit 0
    }

    # ── 5. integrate: wait for auto-merge, follow with git pull --ff-only ──
    ok (do { git fetch origin } | complete) 'git fetch'
    let run_branch = $"origin/fabro/run/($run_id)"
    let has_branch = ((do { git rev-parse --verify $"refs/remotes/($run_branch)" } | complete).exit_code == 0)
    if not $has_branch {
        if $reason == 'publish_blocked' {
            fail 'publish blocked and the run branch was NOT pushed — work lives in the server checkpoint; fix credentials and retry'
        }
        fail $"run branch ($run_branch) not found — nothing to integrate"
    }
    # Variant B (fabro-ab2c): post the required `lab-check-local` context
    # locally on the run branch head (see the seeds original for the
    # naming rationale). Failure is a WARN, not fatal — GitHub auto-merge
    # is the integration path here regardless.
    let run_sha = ((do { git rev-parse $run_branch } | complete).stdout | str trim)
    let posted = (do { ^gh api $"repos/($GITHUB_REPO)/statuses/($run_sha)" -f state=success -f context=lab-check-local -f description='local lab-check (run terminal-succeeded)' } | complete)
    if $posted.exit_code != 0 {
        let detail = ($posted.stderr | str trim | default $posted.stdout | str trim)
        print $"run_workflow: WARN local lab-check post failed: ($detail)"
    }

    # Auto-merge is the ONLY automated integration path (linear-
    # history rule: fetch + ff-pull only). Detection is TREE EQUALITY
    # (GitHub auto-merge SQUASHES, so the run-branch tip is never an
    # ancestor).
    mut landed = false
    let merge_deadline = 40
    for _ in 1..$merge_deadline {
        sleep 15sec
        let _ = (do { git fetch origin $base_branch } | complete)
        $landed = ((do { git diff --quiet $run_branch $"origin/($base_branch)" } | complete).exit_code == 0)
        if $landed { break }
    }
    if not $landed {
        fail $"auto-merge did not land within ($merge_deadline * 15) sec — MANUAL PATH: inspect the run PR checks on ($GITHUB_REPO), resolve the blocker, then 'git pull --ff-only' once origin/($base_branch) moved"
    }
    print 'run_workflow: auto-merge landed — integrating the new target into the checkout'
    ok (do { git pull --ff-only origin $base_branch } | complete) 'git pull --ff-only'
    print $"run_workflow: done — run ($run_id) integrated"
}
