#!/usr/bin/env nu
# Touched-crates quality gate (fabro-5453, world merger): derives the crates
# a run actually changed and gates exactly those — the fabro workspace takes
# 15+ min for a cold full build at the 8-CPU run environment, so the
# develop-workflow tester cannot afford `--workspace` gates (measurement:
# scripts/gate-measure-24cpu-reference.log, 394s cold at 24 CPUs).
# Exit 0 = green. Style follows the retired lab exemplar (archived tag
# scripts/qualitygate.nu): sections, `do { ^cmd } | complete`, first red
# stops the gate.
#
# Base detection reuses the lab evidence.nu pattern: the parent of the LAST
# engine checkpoint commit (subject 'fabro(<run-id>): ...') is the run base —
# works in the shallow sandbox clone without origin refs.

# Flaky-test policy: nextest retries each failing test once. The known
# fabro-80b3 class (for_each_accepts_an_array_at_the_item_limit times out
# under full-suite load) gets its second chance; deterministic failures
# still fail the gate.
const NEXTEST_RETRIES = 1

def current-branch [] {
    (git branch --show-current | str trim)
}

def run-base [] {
    let run_id = (
        current-branch
        | parse --regex 'fabro/run/(?P<id>[^/]+)$'
        | get -o id.0
        | default ''
    )
    if ($run_id | is-empty) {
        # Interactive/human invocation outside a run: diff the working tree
        # against HEAD (uncommitted changes) — the touched-set of a draft.
        return {base: "HEAD", grounded: false}
    }
    let subject_mark = $"fabro\(($run_id)\):"
    let checkpoints = (git log --format=%H --fixed-strings --grep $subject_mark | lines | compact)
    if ($checkpoints | is-empty) {
        return {base: "HEAD", grounded: false}
    }
    let base = (git rev-parse $"($checkpoints | last)^")
    {base: $base, grounded: true}
}

# Map changed paths to workspace crates; root manifest/lock changes mark the
# whole workspace as touched (dependency edits can affect every crate).
def touched-crates [] {
    let base = (run-base).base
    # seeds-7e0f: union with untracked files — brand-new files never
    # appear in `git diff --name-only`, so a new test file silently
    # vanished from the derivation. `--exclude-standard` keeps
    # git-ignored files excluded.
    let paths = ((git diff --name-only $base | lines | compact) | append (git ls-files --others --exclude-standard | lines | compact) | uniq)
    let crate_paths = ($paths | where {|p| ($p | str starts-with 'lib/') or ($p | str starts-with 'crates/') })
    # `parse` yields a table PER input string, so `each` would nest the
    # result (list<list<record>> — the run-1 gate crash). flatten first.
    # crates/<name>/** is this repo's layout (seeds-9482); the lib/... arms
    # stay for portability of the pattern.
    let crates = ($crate_paths
        | each {|p| $p | parse --regex '^(?:lib/(?:apps|components|foundation)|crates)/(?P<crate>[^/]+)/' }
        | flatten
        | get -o crate
        | uniq
        | compact)
    # Loud failure (seeds-9482): an all-Rust diff that derives no crate used
    # to fall through to "no crates touched" and exit green.
    let rs_touched = ($paths | where {|p| $p | str ends-with '.rs' } | is-not-empty)
    if $rs_touched and ($crates | is-empty) {
        print "gate FAIL: diff touches *.rs files but no workspace crate could be derived (expected lib/... or crates/<name>/... paths)"
        exit 1
    }
    if ($crate_paths | is-empty) {
        return []
    }
    let root_touched = ($paths | where {|p|
        ($p in ['Cargo.toml' 'Cargo.lock' 'rust-toolchain.toml'])
    } | is-not-empty)
    if $root_touched {
        print "root manifest/lock changed -> workspace-wide gate (cargo check)"
        # Sentinel: main routes this to check-workspace-compiles. Returning
        # [] here would silently degrade to fmt-only (run-1 lesson).
        return ["__workspace__"]
    }
    $crates
}

# Toolchain pin (AGENTS.md): rustfmt/clippy results depend on the compiler
# version — the repo pins nightly-2026-09-22, which is also the default (and
# only) toolchain in the toolchain image. Explicit pin keeps the gate
# identical on the host, where the default is stable.
const PINNED_TOOLCHAIN = "nightly-2026-09-22"

def check-fmt [] {
    print "== cargo fmt --check --all =="
    let res = (do { ^cargo $'+($PINNED_TOOLCHAIN)' fmt --check --all } | complete)
    if $res.exit_code != 0 {
        print ($res.stdout | str trim -r -c "\n")
        print ($res.stderr | str trim -r -c "\n")
        return false
    }
    print "format clean"
    true
}

# Loop-asset tier (fabro-bfe1): deterministic machine verification of the
# dev loop's own .nu scripts. Two parts: a side-effect-free parse check of
# every develop-workflow script (`nu --ide-check` parses only — a bare
# `source <file>` would auto-invoke the script's `def main` after import;
# see the closeout-smoke.nu header), then the checked-in evidence-smoke
# regression over evidence.nu's pure helpers. Runs in BOTH main paths —
# crates touched or not — so a loop-asset-only diff is no longer a 4s
# no-op gate (run 01M23TE61D4Y).
def check-loop-assets [] {
    print '== checking loop-asset scripts =='
    # Full-repo nushell tier (lint-nu.nu): parse check of EVERY script —
    # repo scripts/ and all workflow assets, not just develop's — plus the
    # interpolated-regex scan that parse checks cannot see.
    # The gate previously walked develop scripts only; a verify.nu shipped
    # broken through that gap.
    let lint = (do { ^nu scripts/lint-nu.nu } | complete)
    print $lint.stdout
    if ($lint.exit_code != 0) {
        print $lint.stderr
        return false
    }
    # Prompt-lint tier (fabro-41de C3 guard, gate-wired 2026-09-19): loop
    # prompts/graph/toml literals must not rot — unresolvable seed ids,
    # drifted justfile anchors, provenance literals (run ids, PR refs).
    # Runs in BOTH gate paths so prompt-only diffs are gated too.
    let plint = (do { ^nu .fabro/scripts/prompt-lint.nu } | complete)
    print $plint.stdout
    if ($plint.exit_code != 0) {
        print $plint.stderr
        return false
    }
    let smokes = [
        '.fabro/workflows/develop/scripts/evidence-smoke.nu'
        # Graph-contract pin (fabro-83df/fabro-92e2, incident 2026-09-19):
        # the develop graph must keep its deterministic-exit contract —
        # planner ungated, preflight report-only, guard exits intact.
        '.fabro/workflows/develop/scripts/graph-contract-smoke.nu'
    ]
    for smoke in $smokes {
        let res = (do { ^nu $smoke } | complete)
        if ($res.exit_code != 0) {
            print $"loop-asset smoke FAILED: ($smoke)"
            print ($res.stdout | str trim -r -c "\n" | lines | last 20)
            print ($res.stderr | str trim -r -c "\n")
            return false
        }
    }
    # Checked-in fixture batteries (seed fabro-ac84, run 01M2NDGXSKF8YFANJXRFZGC087):
    # the gate must EXECUTE the fixture scripts under .fabro/scripts/, not just
    # parse them. Discovery is explicit and minimal — name each battery; do NOT
    # blanket-run every .fabro/scripts/*.nu (stage-journal.nu and friction-score.nu
    # are tools, not batteries).
    let batteries = [
        '.fabro/scripts/dup-run-check-fixtures.nu'
        # push-gate status-filter battery (seeds-e5af): quota-parked runs
        # do not refuse, running runs do, unknown fails safe.
        '.fabro/scripts/push-gate-fixtures.nu'
        # planner-preflight anchor battery (fabro-83df report-only
        # end-to-end case included; 0.5s measured 2026-09-19)
        '.fabro/scripts/planner-preflight-anchor-fixtures.nu'
        # revisor overflow-ledger battery: fixture revision file with
        # open + consumed overflows drives open/consume selection
        # deterministically.
        '.fabro/workflows/revisor/scripts/overflow-ledger-fixtures.nu'
    ]
    for battery in $batteries {
        let res = (do { ^nu $battery } | complete)
        if $res.exit_code != 0 {
            print $"loop-asset fixture battery FAILED: ($battery)"
            print ($res.stdout | str trim -r -c "\n" | lines | last 20)
            print ($res.stderr | str trim -r -c "\n")
            return false
        }
    }
    # Graph-parse arm (2026-09-26, truncated-conductor incident: a regex
    # edit cut the DOT edges off workflow.fabro and only the server-side
    # packager caught it). Parse EVERY workflow graph with graphviz when
    # available; fall back to `fabro validate` when the CLI is on PATH
    # (host); error when neither tool exists in neither form.
    let dot = ((which dot | length) > 0)
    let fabro_bin = ((which fabro | length) > 0)
    if $dot {
        for graph in (glob .fabro/workflows/*/workflow.fabro) {
            # Fabro's x.* attribute namespace (stage envelopes, kind edges)
            # is not graphviz-legal DOT: legalize it for the parse check so
            # the lint judges STRUCTURE (cut edges, broken blocks) without
            # rejecting the convention. graphviz exit codes are version-
            # fragile on parse errors (2.43 exits 0 with an Error on
            # stderr): treat either signal as failure.
            let legalized = (open $graph | lines | each {|line|
                $line | str replace --all --regex 'x\.' 'x_'
            } | str join "\n")
            let probe = $"($graph).graphviz-lint.tmp"
            ($legalized + "\n") | save --force $probe
            let res = (do { ^dot -Tcanon $probe } | complete)
            rm $probe
            if $res.exit_code != 0 or ($res.stderr | str contains 'Error:') {
                print $"workflow graph PARSE FAILED: ($graph)"
                print ($res.stderr | str trim | str substring 0..160)
                return false
            }
        }
        print "workflow graphs parse (graphviz)"
    } else if $fabro_bin {
        for wfdir in (glob .fabro/workflows/*) {
            if ($wfdir | path type) != 'dir' { continue }
            let name = ($wfdir | path basename)
            let res = (do { ^fabro validate $name } | complete)
            if $res.exit_code != 0 {
                print $"workflow VALIDATE FAILED: ($name)"
                print (($res.stdout | str trim) + ($res.stderr | str trim) | str substring 0..160)
                return false
            }
        }
        print "workflow graphs validate (fabro cli)"
    } else {
        print "warn: neither graphviz nor fabro available — workflow graph parse skipped"
    }
    print "loop-asset scripts green"
    true
}

def check-clippy [crates: list<string>] {
    if ($crates | is-empty) { return true }
    # '-p' and the crate name MUST be separate argv elements: a single
    # "-p crate" string gets word-split by nu's external spread, handing
    # cargo a package name with a leading space (run-2 gate crash).
    let pkgs = ($crates | each {|c| ['-p' $c] } | flatten)
    print $"== cargo clippy ($crates | str join ', ') -D warnings =="
    let res = (do { ^cargo $'+($PINNED_TOOLCHAIN)' clippy ...$pkgs --all-targets -- -D warnings } | complete)
    if $res.exit_code != 0 {
        print ($res.stdout | str trim -r -c "\n" | lines | last 30)
        print ($res.stderr | str trim -r -c "\n")
        return false
    }
    print "clippy clean"
    true
}

def check-tests [crates: list<string>] {
    if ($crates | is-empty) { return true }
    let pkgs = ($crates | each {|c| ['-p' $c] } | flatten)
    print $"== cargo nextest ($crates | str join ', ') — retries ($NEXTEST_RETRIES) =="
    let res = (do { ^cargo nextest run ...$pkgs --no-fail-fast --retries $NEXTEST_RETRIES } | complete)
    if $res.exit_code != 0 {
        print ($res.stdout | str trim -r -c "\n" | lines | where {|l| ($l | str contains 'FAIL') or ($l | str contains 'Summary')} | last 20)
        print ($res.stderr | str trim -r -c "\n")
        return false
    }
    print "tests green"
    true
}

# Workspace-wide fallback when root manifests changed: a compile check only
# (clippy+tests on all 52 crates would blow the tester timeout).
def check-workspace-compiles [] {
    print '== cargo check --workspace — root manifest changed =='
    let res = (do { ^cargo check --workspace } | complete)
    if $res.exit_code != 0 {
        print ($res.stderr | str trim -r -c "\n" | lines | last 30)
        return false
    }
    print "workspace compiles"
    true
}

def main [] {
    let crates = (touched-crates)
    let base = (run-base)
    if not $base.grounded {
        print 'gate base ungrounded: interactive or pre-checkpoint, diffing working tree'
    }
    if ($crates | is-empty) {
        print "no crates touched"
        let green = ((check-loop-assets) and (check-fmt))
        if $green { print "GATE GREEN"; exit 0 }
        print "GATE RED"
        exit 1
    }
    if ($crates | any {|c| $c == '__workspace__' }) {
        let green = ((check-loop-assets) and (check-fmt) and (check-workspace-compiles))
        if $green { print "GATE GREEN"; exit 0 }
        print "GATE RED"
        exit 1
    }
    print $"touched crates: ($crates | str join ', ')"
    let green = ((check-loop-assets) and (check-fmt) and (check-clippy $crates) and (check-tests $crates))
    if $green {
        print "GATE GREEN"
        exit 0
    }
    print "GATE RED"
    exit 1
}
