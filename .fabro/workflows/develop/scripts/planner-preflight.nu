#!/usr/bin/env nu
# Deterministic already-landed preflight (fabro-a32f): runs BEFORE the
# planner LLM lap and greps the merge-target base history for the top
# `seeds ready --assignee fabro` candidates, reusing dup-run-check.nu's
# landed-implementation classification verbatim (filed-only revisor
# commits never count; Fabro-Run trailer identity via --self; a landed
# true-merge or squash `(#n)` commit implementing the seed counts).
#
# Motivation (run 01M2Q2Q2NY211PVV0E44YDAAQN, PR #207): the planner's
# ALREADY LANDED arm re-derived a sub-second greppable fact (manual fix
# commit 34e8f2c vs a stale tracker row) across three LLM probe rounds —
# 93.6s / $0.143 for what this script answers mechanically.
#
# Routes (output_schema="routing" on the node) — REPORT-ONLY
# (fabro-83df, user decision 2026-09-19): this script closes NOTHING and
# never exits the run early. It always emits preferred_next_label
# "Preflight done"; the full per-candidate verdict table ships inline to
# the planner as the `output.preflight` context key. The planner's
# ALREADY LANDED arm (fabro-d183 two-branch rule) owns the decision AND
# any superseded-close — every closure stays review-protected. Rationale
# (incident 2026-09-19): a commit subject naming a seed id is not
# evidence that the seed's acceptance criteria are met; the auto-close
# fired once in 34 runs, on reopen/verify commits (fabro-395b), and the
# early exit crashed the line on the planner goal gate (fabro-92e2).
# The report carries a one-line `legend` mapping each verdict string to
# its meaning so the planner never re-reads this header to interpret the
# table; verdicts are ADVISORY — a `duplicate` row is a strong hint, the
# acceptance-criteria judgment stays with the planner.
#
# Fail-open (hard rule): ANY internal error — git fetch failure, tracker
# error, empty candidate list, dup-run-check crash — routes the planner
# normally with the degraded mode recorded in the report. This node must
# never dead-end or block the run; it exits 0 on every degraded path and
# reserves non-zero for invocation bugs.
#
# Relationship to open fabro-a01f (claim race / stale tracker snapshot):
# this preflight NARROWS that window — it catches the merged-but-still-
# open state at pre-planner time — but does NOT close it: a PR can still
# land between this check and the planner's claim, and the tracker
# snapshot can still lag. The durable engine fixes (fabro-6b58,
# fabro-9372) remain the real closures; the implementer-side dup-run
# check stays the backstop.
#
# Anchor verification (fabro-7daf): each candidate's description is
# scanned for `path:line` / `path:line-line` anchors, verified against
# the current worktree, and surfaced as anchors_ok/anchor_flags fields on
# the verdict rows. Anchor checks fail open: any parse/read failure
# degrades to "no anchors" (anchors_ok true) exactly like a seed that
# cites no anchors at all.
#
# Usage (node invocation; also drivable standalone for dry-runs):
#   echo <run-id> | nu planner-preflight.nu [--base origin/main]
#   nu planner-preflight.nu --candidates fabro-x,fabro-y --report-only
#     --candidates  comma-separated override of the seeds ready candidate
#                   list (dry-run / fixture path; skips seeds ready)
#     --report-only skip the closure side effects — report the landing
#                   verdict without closing anything (dry-run / fixtures)
#     --top N       how many top candidates to check (default 5)

#
# STANDING POLICY (fabro-9ec3, 2026-09-17): any planner rule naming a
# mechanically-checkable invariant lands as a CHECK in this script (or
# the engine's output validator), never as a new prose paragraph in
# prompts/planner.md — the 139-line prompt was the structural cause of
# the largest seed cluster. Migrated arms recorded here:
#   arm 1 (fabro-4c81 intent, fabro-7daf mechanism): pre-claim
#     path-resolution of seed-cited anchors. The existing anchor
#     verification covers path:line/path:line-line anchors against the
#     worktree; fabro-9ec3 extended it to bare path-only citations
#     (anchor_check.nu extract-bare-paths/check-bare-paths) — no gap
#     remains, both flag classes ship in anchor_flags.
#   arm 2 (complements engine-side fabro-9372): in-flight run/PR
#     exclusion — in-flight-claims below maps recent unmerged develop
#     run branches to claimed seed ids via the stage-journal fallback
#     and reports them per-candidate as in_flight/in_flight_run.
#     fabro-32db (2026-09-19): a claim is in flight ONLY while its run
#     is NON-TERMINAL — provably-terminal branches (tip subject
#     `closeout`, squash `(#n)`, or `(failed)` older than the retry
#     grace window; terminal-tip? below) are excluded, so an orphaned
#     terminal-run claim never blocks candidate selection.
#   RETIRED with fabro-83df (2026-09-19): the former live-close arm
#     (fabro-ead4 note+close for every unambiguous duplicate) and the
#     report-only/live route split — report-only is the only mode now.
#   arm 3: the brief gate-command ban lives in the PLANNER OUTPUT
#     SCHEMA (.fabro/workflows/develop/schemas/planner-output.schema.json,
#     wired via the planner node's output_schema) — the fabro-017f teeth
#     pattern: a brief containing `just qualitygate` or its
#     byte-equivalent body fails validation and burns an output retry,
#     never passes silently.
#   arm 4 (seeds-7e57, 2026-09-21): publish_blocked_risk — a seed whose
#     description targets `.github/workflows/**` is unpublishable while
#     the fabro GitHub App lacks the Workflows permission (incident run
#     01M32J3AF: six rejected pushes, seed closed inside an unpushed
#     commit). The arm resolves the EFFECTIVE permission (env flag, gh
#     installation probe, then the operator marker file) and flags
#     candidates — advisory, fail-open — ONLY on a POSITIVE "absent"
#     verdict; "unknown" never flags: no static blanket-block on
#     workflow targets after the operator grant.

# Anchor verification helpers (fabro-7daf): cited file:line anchors in
# seed descriptions are checked against the current worktree so rotted
# (dead) seeds surface mechanically in the verdict table below.
source anchor_check.nu

# Run terminality, tip-subject form (fabro-32db): the shared definition
# is "a seed claim is in flight ONLY while its run is non-terminal".
# Git-only terminality proof is the run branch's TIP COMMIT SUBJECT —
# every stage completion lands as `fabro(<run>): <node> (<status>)`, so:
#   - tip node `closeout` (the develop graph's terminal node, any
#     status) -> the run completed its graph: TERMINAL;
#   - tip subject ending `(#n)` -> the PR squash-merged: TERMINAL;
#   - tip status `failed` -> terminal ONLY past a retry-grace window:
#     the engine retries/bounces failed stages within minutes (observed
#     `evidence (failed)` then `tester (succeeded)` in run
#     01M2RJSBP8XSPCKTEACR7E1PYP), so a FRESH failed tip may still move;
#   - anything else (intermediate `(succeeded)` tip, unparseable
#     subject, git log failure) -> NOT provably terminal, stays
#     in-flight — fail-open direction preserved (advisory, never
#     blocks). NOTE: the journal-last-record form of the hypothesis is
#     FALSE for failed runs (a failed run's journal ends mid-flight
#     with no closeout record — run 01M2WAY0KH's journal stops at
#     `evidence`); the tip subject carries the status the journal drops.
const TERMINAL_GRACE_MIN_DEFAULT = 60

def terminal-tip? [run: string, subject: string, age_sec: int, grace_min: float]: nothing -> bool {
    let s = ($subject | str trim)
    # starts/ends-with over interpolated regexes: nu's $'...' treats
    # every bare (...) as interpolation, so paren-heavy regex cannot be
    # interpolated safely — prefix/suffix matching needs no escapes.
    if ($s | str starts-with $"fabro\(($run)\): closeout ") { return true }
    if ($s =~ '\(#\d+\)$') { return true }
    if ($s | str starts-with $"fabro\(($run)\): ") and ($s | str ends-with " (failed)") {
        return ($age_sec > ($grace_min * 60))
    }
    false
}

# In-flight run/PR exclusion (fabro-9ec3 arm 2; complements engine-side
# fabro-9372 and the planner prompt's fabro_runs_list guard): a seed id
# claimed by a RECENT, UNMERGED, NON-TERMINAL develop run branch is in
# flight — the ~15-min claim-to-PR window plus the open-PR lifetime.
# Terminal-run branches (fabro-32db) are excluded: their claims are
# orphaned, not in flight — a requeued seed must not stay marked
# in-flight forever by the dead run's unmerged branch (the af22
# deadlock: guard requeues at 6h, preflight re-marks, planner re-skips). Credential-less:
# derives the remote from --base (same as dup-run-check), fetches
# refs/heads/fabro/run/* once into a scratch namespace, keeps branches
# committed within the last 14 days (newest first, max 12), drops
# branches already merged into base (their seed is the landed arm's
# business), skips the invoking run, and maps run -> seed id by grepping
# the run branch's stage journal (.fabro/journal/<run_id>.jsonl — the
# documented PROJECT_FACTS fallback; planner-node lines preferred for
# precision). Fail-open hard rule: any error degrades to zero claims
# with a note, never blocks — a false claim mark is planner-adjudicated,
# this table never closes or skips anything itself.
# Tracker seed-id prefix: derived from .seeds/config.yaml (project name),
# never hardcoded — this workflow is portable across trackers (ported from
# the origin loop, where the prefix was `fabro-`; lineage lore ids in
# comments stay as-is and never match this parse).
def tracker-prefix [] {
    if (not ('.seeds/config.yaml' | path exists)) { return '' }
    let cfg = (open .seeds/config.yaml)
    $cfg | get project
}

def journal-claims [sha: string, run: string] {
    let j = (do { ^git show $"($sha):.fabro/journal/($run).jsonl" } | complete)
    if $j.exit_code != 0 { return [] }
    let lines = ($j.stdout | lines | where {|l| $l | str contains '"node":"planner"'})
    if ($lines | is-empty) { return [] }
    let prefix = (tracker-prefix)
    # Regex built by concatenation: an interpolated $"..." string would
    # treat the regex's parentheses as nu expression interpolations.
    let pattern = ($prefix + '-(?<seed>(?:[a-z][a-z0-9]*-)?[0-9a-z]{4,})(?![0-9a-z-])')
    $lines
    | parse --regex $pattern
    | get -o seed
    | default []
    | uniq
    | each {|seed| $"($prefix)-($seed)"}
}

def in-flight-claims [remote: string, base: string, self_id: string] {
    let fetch = (do { ^git fetch -q $remote '+refs/heads/fabro/run/*:refs/preflight-run/fabro/run/*' } | complete)
    if $fetch.exit_code != 0 {
        return {claims: [], note: $"in-flight fetch degraded: ($fetch.stderr | str trim | str substring 0..160)"}
    }
    let refs = (do { ^git for-each-ref '--sort=-committerdate' '--format=%(refname) %(objectname) %(committerdate:unix)' 'refs/preflight-run/' } | complete)
    if $refs.exit_code != 0 {
        return {claims: [], note: "in-flight for-each-ref degraded"}
    }
    let now = (date now | format date "%s" | into int)
    let rows = ($refs.stdout | parse --regex '(?m)^refs/preflight-run/fabro/run/(?P<run>[0-9A-Za-z-]+) (?P<sha>[0-9a-f]{7,40}) (?P<ts>\d+)')
    let recent = ($rows | each {|r| $r | update ts ($r.ts | into int)}
        | where {|r| ($now - $r.ts) < 1209600}
        | first 12)
    let grace_min = ($env.FABRO_PREFLIGHT_TERMINAL_GRACE_MIN? | default $TERMINAL_GRACE_MIN_DEFAULT | into float)
    mut claims = []
    for r in $recent {
        if $self_id != null and $r.run == $self_id { continue }
        let merged = (do { ^git merge-base --is-ancestor $r.sha $base } | complete)
        if $merged.exit_code == 0 { continue }
        # Terminal-run exclusion (fabro-32db): a provably-terminal
        # branch's claims are orphaned, not in flight. Fail-open: a git
        # log failure leaves the branch treated as in-flight (advisory).
        let tip = (do { ^git log -1 --format=%s $r.sha } | complete)
        if $tip.exit_code == 0 and (terminal-tip? $r.run $tip.stdout ($now - $r.ts) $grace_min) { continue }
        for seed in (journal-claims $r.sha $r.run) {
            $claims = ($claims | append {seed: $seed, run: $r.run})
        }
    }
    let dedup = (if ($claims | is-empty) {
        []
    } else {
        $claims | group-by seed | items {|seed, cs| {seed: $seed, run: ($cs | get run | first)}}
    })
    {claims: $dedup, note: null}
}

# --- publish_blocked_risk arm (seeds-7e57, 2026-09-21) ---------------
# A seed targeting `.github/workflows/**` cannot land while the fabro
# GitHub App lacks the Workflows permission (incident run 01M32J3AF:
# every push rejected with 'refusing to allow a GitHub App to create or
# update workflow ... without workflows permission'). The arm resolves
# the effective permission ONCE per preflight and flags workflow-
# targeting candidates ONLY on a positive "absent" verdict. Fail-open
# hard rule: "unknown" (no probe possible, no marker set) NEVER flags —
# this is not a static blanket-block on workflow targets.
#
# The operator marker file `.fabro/github-app-workflows-permission`
# (repo root; contents `granted` or `absent`) is the PRIMARY,
# deterministic source for this verdict: the operator's explicit
# statement of the fabro GitHub App's effective Workflows permission,
# set after granting/revoking it (see docs/workflows-permission.md for
# the operator contract, including the staleness rule tied to fabro-11d9).
# The tiers below are conveniences layered on top of that source — the
# resolution order only decides which convenience wins when several are
# present; it never demotes the marker's authority as the documented
# operator source.
#
# Resolution precedence (first definitive answer wins):
#   1. env flag  FABRO_GH_WORKFLOWS_PERMISSION = granted|absent
#   2. gh probe  `gh api repos/{owner}/{repo}/installation` — the
#      installation payload's permissions.workflows field ("write" =
#      granted, anything else = absent); a failed probe falls through.
#      Read-only: no writes, no tokens handled here — gh uses the
#      engine-injected auth surface (ADR-0019: permission changes are
#      engine-mediated/operator-granted only, no raw clients).
#   3. operator marker file (the primary source itself — also the
#      last-resort tier when no env flag or probe answers).
#   4. none of the above -> "unknown" -> fail-open, no flag.

def permission-normalize [v: string]: nothing -> string {
    let t = ($v | str trim | str lowercase)
    if $t in ["granted" "absent"] { $t } else { "unknown" }
}

def permission-from-probe [res: record]: nothing -> string {
    if ($res.exit_code? | default 1) != 0 { return "unknown" }
    let j = (try { $res.stdout | from json } catch { null })
    # leniency guard: from json happily yields a scalar (e.g. a bare
    # string) for non-object payloads — only a record can carry the
    # permissions field.
    if not ($j | describe | str starts-with "record") { return "unknown" }
    if (($j | get -o permissions | default {} | get -o workflows | default "") == "write") {
        "granted"
    } else {
        "absent"
    }
}

def probe-workflows-permission [remote: string]: nothing -> string {
    # cheap read-only installation probe via gh; ANY failure at all —
    # no gh on PATH (a spawn error here does NOT stay inside complete),
    # no auth, network, unparseable remote — falls through to the
    # marker ("unknown"), never blocks the run and never skips the
    # marker fallback.
    try {
        let url = (do { ^git remote get-url $remote } | complete)
        if $url.exit_code != 0 { return "unknown" }
        let m = ($url.stdout | parse --regex '(?:github\.com[/:])(?P<repo>[^/\s]+/[^/\s]+?)(?:\.git)?\s*$')
        if ($m | is-empty) { return "unknown" }
        let res = (do { ^gh api $"repos/($m | first | get repo)/installation" } | complete)
        permission-from-probe $res
    } catch { "unknown" }
}

def resolve-workflows-permission [root: string, remote: string]: nothing -> string {
    let envv = (permission-normalize ($env.FABRO_GH_WORKFLOWS_PERMISSION? | default ""))
    if $envv != "unknown" { return $envv }
    let probe = (probe-workflows-permission $remote)
    if $probe != "unknown" { return $probe }
    let marker = ($root | path join '.fabro' 'github-app-workflows-permission')
    if (not ($marker | path exists)) { return "unknown" }
    permission-normalize (open --raw $marker)
}

def workflow-target? [desc: string]: nothing -> bool {
    ($desc | str contains '.github/workflows/')
}

def publish-blocked-risk [desc: string, perm: string]: nothing -> bool {
    # pure decision: flag ONLY on positive absence (fail-open on unknown)
    (workflow-target? $desc) and ($perm == "absent")
}

# Bounded per-candidate row for the planner-facing report. Anchor fields
# (fabro-7daf): anchors_ok is false when ANY cited file:line anchor in
# the description is missing/rotted/mismatched; anchor_flags carries the
# per-anchor detail. The verdict itself is unchanged — anchor rot routes
# through planner adjudication, never through this script's close path.
def row [v: record, desc: string, root: string, claims: list, perm: string] {
    let m = ($v.implementation_matches? | default [] | first | default {})
    let ce = ($v.closing_evidence? | default {})
    let flags = ((extract-anchors $desc | each {|a| check-anchor $a $root} | append (check-bare-paths $desc $root)) | where {|f| $f.status != "ok"})
    # in-flight is advisory: never marked for candidates whose
    # implementation already landed (the duplicate close path owns them).
    let hit = (if $v.verdict == "duplicate" { null } else { $claims | where {|c| $c.seed == $v.seed} | first | default null })
    {seed: $v.seed,
     verdict: $v.verdict,
     sha: ($m.sha? | default ($ce.sha? | default null)),
     subject: (if (($m.subject? | default ($ce.subject? | default "")) | is-empty) { null } else { ($m.subject? | default ($ce.subject? | default "")) | str substring 0..120 }),
     filed_only_matches: ($v.filed_only_matches? | default 0),
     anchors_ok: (($flags | length) == 0),
     anchor_flags: $flags,
     workflow_target: (workflow-target? $desc),
     publish_blocked_risk: (publish-blocked-risk $desc $perm),
     in_flight: ($hit != null),
     in_flight_run: (if $hit == null { null } else { $hit.run })}
}

# Resolve dup-run-check relative to THIS script (.fabro/scripts/ is
# three levels up from .fabro/workflows/develop/scripts/), never CWD —
# the fixture battery drives this script from a scratch clone. `path
# self` is parse-time only, so the anchor must be a const.
const SCRIPT_DIR = (path self | path dirname)


def main [--base: string = "origin/main", --candidates: string, --top: int = 5]: nothing -> nothing {
    # Non-tty stdin (same nu 0.115 constraint as closeout.nu): the engine
    # pipes internal.run_id, read it through external cat.
    let run_id = (cat | str join | str trim)

    mut mode = "checked"
    mut degraded_reason = ""

    # Candidate list: --candidates override, else the top of seeds ready.
    # Candidates carry their description so cited file:line anchors can
    # be verified (fabro-7daf); a description that cannot be fetched
    # degrades per-candidate to "no anchors", never to a script failure.
    let cand = (if $candidates != null {
        $candidates | split row ',' | each {|c| $c | str trim} | where {|c| not ($c | is-empty)} | each {|id|
            let r = (do { seeds show $id --format json } | complete)
            {id: $id, description: (if $r.exit_code != 0 { "" } else { (try { $r.stdout | from json | get -o issue.description | default "" } catch { "" }) })}
        }
    } else {
        let r = (do { seeds ready --assignee fabro --limit 200 --format json } | complete)
        if $r.exit_code != 0 {
            $mode = "degraded"
            $degraded_reason = $"seeds ready failed: ($r.stderr | str trim | str substring 0..200)"
            []
        } else {
            let parsed = (try { $r.stdout | from json } catch { null })
            if $parsed == null {
                $mode = "degraded"
                $degraded_reason = "seeds ready output not valid JSON"
                []
            } else {
                # seeds ready --format json: {success, command, issues: [...]}
                # (issues is absent/empty when nothing is ready)
                $parsed | get -o issues | default [] | each {|i| {id: ($i | get -o id | default ""), description: ($i | get -o description | default "")}} | where {|c| not ($c.id | is-empty)}
            }
        }
    })

    # Empty candidate list is a legitimate degraded park (tracker empty is
    # the PLANNER's verdict to make, never this script's) — never dead-end.
    if ($cand | is-empty) {
        if $mode != "degraded" {
            $mode = "degraded"
            $degraded_reason = "no candidates (empty override or empty seeds ready)"
        }
        {"outcome": "succeeded",
         "preferred_next_label": "Preflight done",
         "context_updates": {"output.preflight": {"mode": $mode, "run_id": ($run_id | default null), "candidates": [], "degraded_reason": (if ($degraded_reason | is-empty) { null } else { $degraded_reason })}}} | to json --raw | print
        return
    }

    let top_c = ($cand | first ([$top 1] | math max))
    let ids = ($top_c | get id)
    let cdesc = ($top_c | get description)

    let dup = ($SCRIPT_DIR | path join '../../..' 'scripts' 'dup-run-check.nu')
    let res = (do { nu $dup ...$ids --base $base --self $run_id } | complete)
    if $res.exit_code != 0 {
        {"outcome": "succeeded",
         "preferred_next_label": "Preflight done",
         "context_updates": {"output.preflight": {"mode": "degraded", "run_id": ($run_id | default null), "candidates": [], "degraded_reason": ($"dup-run-check failed: ($res.stderr | str trim | str substring 0..200)")}}} | to json --raw | print
        return
    }
    let verdicts = ($res.stdout | lines | compact | each {|l| $l | from json })

    # In-flight claims (fabro-9ec3 arm 2): fail-open wrapper — a crash in
    # the arm degrades to zero claims with a note, never blocks the run.
    let remote = ($base | split row '/' | first)
    let inflight = (try { in-flight-claims $remote $base ($run_id | default null) } catch { {claims: [], note: "in-flight arm degraded (fail-open)"} })

    # publish_blocked_risk arm (seeds-7e57): resolve the effective GitHub
    # App Workflows permission once; any resolution failure degrades to
    # "unknown", which never flags (fail-open, advisory-only).
    let perm = (try { resolve-workflows-permission ($SCRIPT_DIR | path join '../../../..') $remote } catch { "unknown" })

    # Report-only (fabro-83df, 2026-09-19): no closures, no early exit.
    # The former mechanical superseded-close block (fabro-ead4) is
    # retired — the planner's ALREADY LANDED arm owns decisions and
    # closures; verdicts below are advisory input to that arm.
    # Verdict legend (fabro-ead4): one field mapping every verdict string
    # dup-run-check emits to its meaning, so the planner never re-reads
    # the script header to interpret the table.
    let legend = {duplicate: "advisory: implementation possibly already in merge-target base (or tracker-closed without resolvable evidence) — planner judges acceptance criteria and closes per the two-branch rule",
                  clean: "no landed implementation found",
                  degraded: "check failed (fetch/tracker error)",
                  publish_blocked_risk: "advisory: seed targets .github/workflows/** and the fabro GitHub App Workflows permission resolved 'absent' (env flag, gh probe, or operator marker .fabro/github-app-workflows-permission) — planner should route Blocked at claim time; 'unknown' permission never flags"}
    let report = {mode: $mode,
                  run_id: ($run_id | default null),
                  candidates: ($verdicts | enumerate | each {|e| row $e.item ($cdesc | get -o $e.index | default "") ($SCRIPT_DIR | path join '../../../..') ($inflight.claims | default []) $perm}),
                  workflow_permission: $perm,
                  legend: $legend,
                  degraded_reason: (if ($degraded_reason | is-empty) { null } else { $degraded_reason }),
                  in_flight_note: ($inflight.note? | default null)}
    {"outcome": "succeeded",
     "preferred_next_label": "Preflight done",
     "context_updates": {"output.preflight": $report}} | to json --raw | print
}
