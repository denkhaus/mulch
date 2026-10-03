#!/usr/bin/env nu
# Fixture battery for overflow-ledger.nu (fabro-552a): a fixture revision
# file with open + consumed overflows drives input selection
# deterministically — no dependence on the live `.fabro/revisions/` tree
# for the core cases (only the last case reads it, as an invariant check).
#
# Cases (seed acceptance criteria):
#   (a) open lists exactly the unconsumed `- overflow:` entries (title,
#       file, line) — the verbatim memoize finding from revision
#       01M2X8458MDWMRVBRVEMDX9W4J is the motivating open entry
#   (b) a consumed entry (filed-as marker directly after the bullet) is
#       excluded from open — idempotent consumption, nothing filed twice
#   (c) an `overflow-dup:` link line is NOT an entry (adjacent-pass
#       dedupe links instead of duplicating)
#   (d) consume marks the entry: marker appended after the bullet, open
#       then excludes it, stats count it consumed
#   (e) consume of an already-consumed entry FAILS (exit 3) — the
#       double-file guard
#   (f) consume of a non-overflow line FAILS (exit 2) — usage guard
#   (g) live invariant: WHILE the real memoize entry in
#       `.fabro/revisions/01M2X8458MDWMRVBRVEMDX9W4J.md` is unconsumed,
#       the default-dir `open` listing must surface it (deterministic
#       visibility against the real tree; once consumed, no assertion)
#
# Usage: nu .fabro/workflows/revisor/scripts/overflow-ledger-fixtures.nu
# (exit 0 = all pass)

const LEDGER = ('.fabro/workflows/revisor/scripts/overflow-ledger.nu' | path expand)
const REAL_REVISIONS = ('.fabro/revisions' | path expand)

# Assert helper: record one case result, abort the battery on mismatch.
def expect [case: string, got, want] {
    if $got == $want {
        print $"PASS \($case\): ($got)"
    } else {
        print -e $"FAIL \($case\): got ($got), want ($want)"
        exit 1
    }
}

def expect-contains [case: string, haystack: string, needle: string] {
    if ($haystack | str contains $needle) {
        print $"PASS \($case\)"
    } else {
        print -e $"FAIL \($case\): missing '($needle)'"
        exit 1
    }
}

# Run the ledger with a scratch --dir and return parsed JSON.
def ledger-open [dir: string] {
    let r = (do { nu $LEDGER open --dir $dir } | complete)
    if $r.exit_code != 0 {
        print -e $"open exited ($r.exit_code): ($r.stderr)"
        exit 1
    }
    $r.stdout | str trim | from json
}

def ledger-stats [dir: string] {
    let r = (do { nu $LEDGER stats --dir $dir } | complete)
    if $r.exit_code != 0 {
        print -e $"stats exited ($r.exit_code): ($r.stderr)"
        exit 1
    }
    $r.stdout | str trim | from json
}

# Fixture revision file: one consumed entry, one open entry (the verbatim
# memoize finding shape), one overflow-dup link.
def fixture-text [] {
    [
        '# Revision — run 01FIXTUREOPEN'
        ''
        '## Findings'
        ''
        '### Park upstream-PR-gated seeds mechanically in the planner preflight'
        ''
        '- filed: not filed — overflow to journal (zero credit this pass)'
        '- overflow: Park upstream-PR-gated seeds mechanically in the planner preflight — add an externally_gated arm to the verdict table; effect: -60s per develop run'
        '- filed-as: fabro-fix01 (consumed 2026-09-18)'
        ''
        '### Memoize planner external-wait skip adjudications into the seed body'
        ''
        '- overflow: Memoize planner external-wait skip adjudications into the seed body — append-only skip note in the seed body at the skip-adjudication step; effect: removes recurring per-lap re-derivation tax'
        ''
        '### adjacent-pass link'
        ''
        '- overflow-dup: Memoize planner external-wait skip adjudications — already open above; link, not a new entry'
        ''
    ] | str join "\n"
}

def main [] {
    let scratch = (mktemp -d -t overflow-ledger-fixtures.XXXXXX)
    let fixture = ($scratch | path join '01FIXTUREOPEN.md')
    (fixture-text) ++ "\n" | save --force $fixture

    # (a) open lists exactly the unconsumed memoize entry
    let open = (ledger-open $scratch)
    expect '(a) open entry count' ($open | length) 1
    let entry = ($open | first)
    expect '(a) open entry file' $entry.file '01FIXTUREOPEN.md'
    expect-contains '(a) open entry title' $entry.title 'Memoize planner external-wait skip adjudications'

    # (b)+(c) consumed excluded, dup-link not an entry — count already 1;
    # stats pins the split explicitly.
    let stats = (ledger-stats $scratch)
    expect '(b) stats open' $stats.open 1
    expect '(b) stats consumed' $stats.consumed 1
    expect '(b) stats files' $stats.files 1

    # (d) consume marks the open entry
    let c = (do { nu $LEDGER consume --file $fixture --line $entry.line --seed fabro-fix02 --run-id 01FIXTURERUN } | complete)
    expect '(d) consume exit' $c.exit_code 0
    expect-contains '(d) marker appended' (open --raw $fixture) '- filed-as: fabro-fix02 (consumed'
    let open2 = (ledger-open $scratch)
    expect '(d) open after consume' ($open2 | length) 0
    expect '(d) stats consumed after consume' (ledger-stats $scratch).consumed 2

    # (e) double consume fails with exit 3
    let c2 = (do { nu $LEDGER consume --file $fixture --line $entry.line --seed fabro-fix02 } | complete)
    expect '(e) double-consume exit' $c2.exit_code 3
    expect-contains '(e) double-consume message' $c2.stderr 'already consumed'

    # (f) consume of a non-overflow line fails with exit 2
    let c3 = (do { nu $LEDGER consume --file $fixture --line 1 --seed fabro-fix02 } | complete)
    expect '(f) non-overflow consume exit' $c3.exit_code 2

    # (g) live invariant: while the real memoize entry stays unconsumed,
    # the default-dir listing surfaces it.
    let real = ($REAL_REVISIONS | path join '01M2X8458MDWMRVBRVEMDX9W4J.md')
    if ($real | path exists) {
        let lines = (open --raw $real | lines)
        let idxs = ($lines | enumerate | where {|it| ($it.item | str starts-with '- overflow: Memoize planner')} | get index)
        if ($idxs | is-empty) {
            print 'SKIP (g): real memoize entry not found in canonical bullet shape'
        } else {
            let i = ($idxs | first)
            let nxt = if $i + 1 < ($lines | length) { $lines | get ($i + 1) } else { '' }
            if ($nxt | str starts-with '- filed-as:') {
                print 'SKIP (g): real memoize entry already consumed'
            } else {
                let live = (ledger-open ($REAL_REVISIONS | path dirname | path join 'revisions' ))
                let hit = ($live | where {|e| $e.file == '01M2X8458MDWMRVBRVEMDX9W4J.md' and ($e.title | str starts-with 'Memoize planner')} | length)
                expect '(g) real memoize entry surfaced' $hit 1
            }
        }
    } else {
        print 'SKIP (g): real revision file absent'
    }

    rm -rf $scratch
    print 'overflow-ledger fixtures green'
}
