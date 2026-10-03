#!/usr/bin/env nu
# Smoke test for evidence.nu's PURE helpers (fabro-bfe1): sanitize,
# resolve-blobrefs, diff-sort-key, is-loop-path, total — exercised over
# canned fixtures without git (helpers that shell out — numstat-rows,
# run-base, worktree-state, claimed-seed — are out of scope here;
# their logic is covered by the parse check in scripts/qualitygate.nu
# and manual invocation). The `source` const resolves against THIS
# file's directory, so the script runs from any cwd (closeout-smoke
# pattern):
#   nu .fabro/workflows/develop/scripts/evidence-smoke.nu

const EVIDENCE = "evidence.nu"
source $EVIDENCE

def fail [what: string]: nothing -> nothing {
    print -e $"evidence-smoke: FAIL — ($what)"
    exit 1
}

# sanitize: bare "/word" tokens get backticked (agent skill-reference
# crash guard); uppercase-led tokens and token-free text pass through.
if (sanitize "run in /tmp dir") != "run in `/tmp` dir" {
    fail $"sanitize single token: (sanitize 'run in /tmp dir')"
}
# Two passes catch consecutive tokens — the trailing space of match one
# is the leading space of match two.
if (sanitize "a /b /c d") != "a `/b` `/c` d" {
    fail $"sanitize consecutive tokens: (sanitize 'a /b /c d')"
}
if (sanitize "path /Workspace stays") != "path /Workspace stays" {
    fail $"sanitize uppercase token must pass: (sanitize 'path /Workspace stays')"
}
if (sanitize "no tokens here") != "no tokens here" {
    fail "sanitize token-free text must be unchanged"
}

# resolve-blobrefs: no refs -> unchanged; resolvable ref (blob file
# materialized in cwd's .fabro/blobs/) -> inlined verbatim; unresolvable
# ref -> passes through as the link, never dropped.
if (resolve-blobrefs "plain text no refs") != "plain text no refs" {
    fail "resolve-blobrefs no-refs text must be unchanged"
}
let tmp = (mktemp -d)
let sha = "1111111111111111111111111111111111111111111111111111111111111111"
mkdir $"($tmp)/.fabro/blobs"
"BLOB-CONTENT-HERE" | save --force $"($tmp)/.fabro/blobs/($sha).json"
# capture the invoking cwd: the smoke must run from any cwd (host or run
# container), never assume a fixed workspace path (origin loop assumed
# /workspace/fabro — run-container layout of the origin repo)
let origin_dir = $env.PWD
cd $tmp
let inlined = (resolve-blobrefs $"pre blob://sha256/($sha) post")
cd $origin_dir
rm -rf $tmp
if $inlined != "pre BLOB-CONTENT-HERE post" {
    fail $"resolve-blobrefs resolvable ref must inline: ($inlined)"
}
let missing = "2222222222222222222222222222222222222222222222222222222222222222"
let passthrough = (resolve-blobrefs $"see blob://sha256/($missing) end")
if $passthrough != $"see blob://sha256/($missing) end" {
    fail $"resolve-blobrefs unresolvable ref must pass through: ($passthrough)"
}

# diff-sort-key: source files sort before docs (a:/z: prefixes) — the
# reviewer sees the complete source diff first.
if (diff-sort-key "lib/main.rs") != "a:lib/main.rs" {
    fail $"diff-sort-key source prefix: (diff-sort-key 'lib/main.rs')"
}
if (diff-sort-key "docs/x.md") != "z:docs/x.md" {
    fail $"diff-sort-key doc prefix: (diff-sort-key 'docs/x.md')"
}
if ((diff-sort-key "lib/main.rs") > (diff-sort-key "docs/x.md")) {
    fail "diff-sort-key must order source before docs"
}

# is-loop-path: dev-loop machinery paths are loop paths; product code
# and untracked roots are not.
let loop_pos = [".fabro/x" ".seeds/issues.jsonl" "scripts/q.nu" ".mulch/y" "justfile" "AGENTS.md" "CLAUDE.md" ".gitignore"]
for p in $loop_pos {
    if not (is-loop-path $p) { fail $"is-loop-path positive: ($p)" }
}
let loop_neg = ["lib/main.rs" "apps/fabro-web/src/x.ts" "README.md" "justfile.md"]
for p in $loop_neg {
    if (is-loop-path $p) { fail $"is-loop-path negative: ($p)" }
}

# total over a canned numstat fixture (the shape numstat-rows returns):
# binary "-" counts as 0, empty input degrades to 0.
let fixture = [
    {add: "10" del: "2" path: "lib/a.rs"}
    {add: "-"  del: "5" path: "logo.png"}
    {add: "3"  del: "1" path: "docs/b.md"}
]
if (total $fixture "add") != 13 { fail $"total add (binary as 0): (total $fixture 'add')" }
if (total $fixture "del") != 8  { fail $"total del: (total $fixture 'del')" }
if (total [] "add") != 0 { fail "total empty must be 0" }

print "evidence-smoke: ok — sanitize/resolve-blobrefs/diff-sort-key/is-loop-path/total verified"

# Sourcing evidence.nu imports its `def main`; nu auto-invokes it after
# the top level runs — exit explicitly so the smoke never reaches it.
exit 0
