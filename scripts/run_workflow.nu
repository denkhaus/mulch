#!/usr/bin/env nu
# Run a workflow end to end and integrate the result
# (thin launcher: `just run <workflow>`).
#
# Pipeline:
#   1. guards: GitButler workspace healthy (parseable, no conflicted
#      commits — any number of applied virtual branches is normal here)
#   2. fabro create <workflow> [--goal] --json   -> run id
#      (or --adopt <run-id> to resume with an existing run)
#   3. fabro start <id> + fabro attach <id>      -> live output
#   4. fabro wait <id> --json                    -> status/reason truth
#   5. integrate (auto-merge lands the run PR on origin/main; the local
#      workspace follows via `but pull`):
#        - post the required `lab-check` status LOCALLY on the run branch
#          head (variant B, fabro-ab2c: no Actions runner involved)
#        - GitHub auto-merge merges the PR -> run branch contained in
#          origin/main -> `but pull` integrates the new target state into
#          the workspace (applied virtual branches rebase on top)
#
# GITBUTLER ADAPTATION (experiment, user directive 2026-10-03): this
# checkout is a GitButler workspace (sits on `gitbutler/workspace`); the
# TARGET is origin/main (the merge target of run PRs — main IS the
# product line). Local work lives on applied virtual branches (one per
# agent/line, the multi-agent experiment); they land via `but pr new` +
# auto-merge, never by pushing main. The target cannot itself be a
# virtual branch (gitbutler refuses applying the target). All VCS writes
# go through `but`; read-only git (fetch, diff, rev-parse) stays allowed.
# The seeds original's local squash-merge fallback is deliberately
# REMOVED — `git merge --squash` behind the workspace's back is exactly
# the forbidden class; a failed auto-merge becomes a loud ALARM with a
# manual operator path instead of an automated workspace rewrite.
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
    let base_branch = (if ($branch | is-empty) { 'main' } else { $branch })  # the ONLY base: the gitbutler target

    # ── 1. guards: GitButler workspace state (structured, not tree scraping) ──
    let st = (do { but status --json } | complete)
    ok $st 'but status'
    let ws = (try { $st.stdout | from json } catch { null })
    if $ws == null { fail 'but status --json did not parse — workspace state unknown' }
    let conflicted = ($ws.stacks?.branches?.commits? | flatten | default [] | where {|c| $c.conflicted? | default false })
    if ($conflicted | is-not-empty) {
        fail $"($conflicted | length) conflicted commits in the workspace — resolve via but resolve before running"
    }
    if (($ws.uncommittedChanges? | default [] | length) > 0) {
        print 'run_workflow: WARN uncommitted changes present — but pull later refuses on conflict; consider committing first'
    }
    print $"run_workflow: target=origin/($base_branch) workflow=($workflow) (gitbutler workspace, ($ws.stacks?.branches?.name? | flatten | compact | default [] | length) applied branch(es))"

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

    # ── 5. integrate: wait for auto-merge, follow with but branch update ──
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

    # Auto-merge is the ONLY automated integration path (GitButler rule:
    # no local git merge/rebase). Detection is TREE EQUALITY (GitHub
    # auto-merge SQUASHES, so the run-branch tip is never an ancestor).
    mut landed = false
    let merge_deadline = 40
    for _ in 1..$merge_deadline {
        sleep 15sec
        let _ = (do { git fetch origin $base_branch } | complete)
        $landed = ((do { git diff --quiet $run_branch $"origin/($base_branch)" } | complete).exit_code == 0)
        if $landed { break }
    }
    if not $landed {
        fail $"auto-merge did not land within ($merge_deadline * 15) sec — MANUAL PATH: inspect the run PR checks on ($GITHUB_REPO), resolve the blocker, then 'but pull' once origin/($base_branch) moved"
    }
    print 'run_workflow: auto-merge landed — integrating the new target into the workspace'
    ok (do { but pull } | complete) 'but pull'
    print $"run_workflow: done — run ($run_id) integrated"
}
