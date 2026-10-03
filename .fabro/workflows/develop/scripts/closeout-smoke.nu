#!/usr/bin/env nu
# Smoke test for closeout.nu's Dockerfile-touch warning (fabro-6f6e).
# Exercises the pure glob/filter logic — dockerfile-hits over a path
# list — without git; the full-script path is a manual invocation check
# (documented on the seed). The `source` const resolves against THIS
# file's directory, so the script runs from any cwd:
#   nu .fabro/workflows/develop/scripts/closeout-smoke.nu

const CLOSEOUT = "closeout.nu"
source $CLOSEOUT

def fail [what: string]: nothing -> nothing {
    print -e $"closeout-smoke: FAIL — ($what)"
    exit 1
}

# Positives: every Dockerfile* segment shape the warning must catch.
let hits = (dockerfile-hits [
    ".fabro/Dockerfile.toolchain"
    ".fabro/Dockerfile"
    "Dockerfile"
    "apps/web/Dockerfile.dev"
])
if $hits != [".fabro/Dockerfile.toolchain" ".fabro/Dockerfile" "Dockerfile" "apps/web/Dockerfile.dev"] {
    fail $"positives mismatch: ($hits | to json -r)"
}

# Negatives: lowercase, prefix-only, and unrelated paths stay silent.
# (Note `Dockerfile-notes.md` WOULD match: its segment starts with
# `Dockerfile`, which is exactly the glob `Dockerfile*` — a true
# positive, not a false one.)
let clean = (dockerfile-hits [
    "dockerfile"
    "lib/main.rs"
    "apps/fabro-web/src/x.ts"
    "a/Docker/keep.rs"
    "docs/about-Dockerfile.md"
])
if ($clean | is-not-empty) {
    fail $"negatives matched: ($clean | to json -r)"
}

# Empty input degrades to empty — no warning, byte-identical close.
if (dockerfile-hits []) != [] { fail "empty input not empty" }

# --- Closure-discipline pre-close check (fabro-02c4) -----------------
# Token extraction: distinctive >=4-char tokens survive; function-word
# stopwords, short tokens, and duplicates drop.
let toks = (demand-tokens "Closeout closure discipline: seeds close only when the seed demand is visible in the run diff, else a documented reason is mandatory")
for expected in ["closeout" "demand" "visible" "diff"] {
    if not ($toks | any {|t| $t == $expected }) { fail $"demand-tokens dropped '($expected)'" }
}
for banned in [seed when else only this with from] {
    if ($toks | any {|t| $t == $banned }) { fail $"demand-tokens kept stopword '($banned)'" }
}
if ($toks | any {|t| ($t | str length) < 4 }) { fail "demand-tokens kept a <4-char token" }

# Visible demand: matching token in a non-empty patch -> close proceeds.
if not (demand-visible ["closeout" "retry"] "diff --git a/.fabro/x b/.fabro/x
+closeout gate added") {
    fail "visible-demand patch was not visible (would park a good close)"
}
# Non-visible demand: non-empty patch with NO token overlap -> park.
if (demand-visible ["retry" "backoff"] "diff --git a/lib/x b/lib/x
+unrelated change") {
    fail "non-visible-demand patch reported visible (fabro-9967 class)"
}
# The fabro-9967 shape itself: empty patch -> always park, with or
# without tokens.
if (demand-visible [] "") { fail "empty patch (no tokens) reported visible" }
if (demand-visible ["closeout"] "") { fail "empty patch (with tokens) reported visible" }
# Degrade: no distinctive tokens + non-empty patch -> visible.
if not (demand-visible [] "+some change") { fail "token-less non-empty patch not visible (degrade broken)" }

# --- Reviewer-journal non-blocking sweep (fabro-22fa) ------------------
# Marker matching: explicit non-blocking wording (both phrasings,
# case-insensitive); a blocking statement stays out.
if not (is-nonblocking "Non-blocking: octal escapes mis-decode, harmless today") {
    fail "is-nonblocking dropped the 'Non-blocking:' marker"
}
if not (is-nonblocking "Noted but not blocking: retry count is per-process") {
    fail "is-nonblocking dropped the 'noted but not blocking' phrasing"
}
if (is-nonblocking "this defect is blocking, do not approve") {
    fail "is-nonblocking matched a blocking statement"
}

# Journal parsing: reviewer-node observations only; other nodes and
# non-matching observations never file.
let jl = (
    nonblocking-from-journal (
        [
            '{"node":"implementer","data":{"observations":["non-blocking mention in the wrong stage"]}}'
            '{"node":"reviewer","data":{"observations":["Verified the diff hunk-by-hunk."]}}'
            '{"node":"reviewer","data":{"observations":["Non-blocking: Latin-1 mis-decode of quoted paths, harmless for the sole consumer."]}}'
            '{"node":"tester","data":{}}'
            'not json at all'
        ] | str join "\n"
    )
)
if ($jl | length) != 1 {
    fail $"nonblocking-from-journal wrong count: ($jl | to json -r)"
}
if not ($jl.0 | str contains "Latin-1") {
    fail "nonblocking-from-journal dropped the finding text"
}

# Null path, fixture journal (tmp file): a real reviewer record with NO
# non-blocking findings must yield an empty list — and an empty list
# means zero seeds create calls in the sweep loop. Also proves the
# missing-journal path degrades to empty.
let tmp = (mktemp -t closeout-null.XXXXXX.jsonl)
'{"node":"reviewer","data":{"painpoints":[],"observations":["Clean approve: diff verified against spec, nothing residual."]}}' | save -f $tmp
if ((journal-nonblocking $tmp) | is-not-empty) {
    fail "null-path fixture journal produced findings (would file seeds)"
}
rm -f $tmp
if ((journal-nonblocking "/nonexistent/.fabro/journal/none.jsonl") | is-not-empty) {
    fail "missing journal did not degrade to empty"
}

# Filed-seed title: excerpt + provenance, bounded length.
let xs = (1..200 | each {"x"} | str join)
let ft = (finding-title $xs "fabro-22fa")
if not ($ft | str starts-with "Reviewer residual (non-blocking) from fabro-22fa:") {
    fail $"finding-title missing prefix: ($ft)"
}
if ($ft | str length) > 140 {
    fail "finding-title unbounded excerpt"
}

# Residual label (fabro-2ab8): the sweep's seeds create args carry the
# `residual` label so machine-filed provenance is visible in the pool.
if (residual-seed-labels) != ["residual"] {
    fail $"residual-seed-labels wrong: (residual-seed-labels | to json -r)"
}

# Regression guard (fabro-22fa): finding-title and the null path stay
# unchanged next to the new label helper.
if (finding-title "Short finding" "fabro-22fa") != "Reviewer residual \(non-blocking\) from fabro-22fa: Short finding" {
    fail "finding-title shape drifted"
}
let long = (0..79 | each { "x" } | str join)
if ((finding-title $long "fabro-22fa" | str length) != 70 + ("Reviewer residual \(non-blocking\) from fabro-22fa: " | str length)) {
    fail "finding-title truncation drifted"
}
if ((journal-nonblocking "/nonexistent/.fabro/journal/none2.jsonl") | is-not-empty) {
    fail "null-path journal degradation drifted"
}

# --- Actionable-marker filing gate (seeds-81cd) ------------------------
# The predicate: literal case-insensitive `residual:` PREFIX on the
# observation text (leading whitespace tolerated).
if not (is-actionable-residual "residual: retry semantics are now per-tenant") {
    fail "is-actionable-residual dropped the 'residual:' prefix"
}
if not (is-actionable-residual "  Residual: ordering assumption in the sweep loop") {
    fail "is-actionable-residual dropped case-insensitive/leading-space 'Residual:'"
}
if (is-actionable-residual "Non-blocking: plain informational note, no marker") {
    fail "is-actionable-residual matched a plain non-blocking note"
}
if (is-actionable-residual "a mid-sentence residual: mention is not a marker") {
    fail "is-actionable-residual matched a mid-sentence 'residual:' mention"
}

# Journal extraction: only marker-matching reviewer observations are
# actionable; plain non-blocking notes stay journal-visible only.
let al = (
    actionable-from-journal (
        [
            '{"node":"reviewer","data":{"observations":["Non-blocking: Latin-1 mis-decode of quoted paths, harmless for the sole consumer."]}}'
            '{"node":"reviewer","data":{"observations":["residual: unbounded retry loop in the sync client, approve with follow-up."]}}'
            '{"node":"implementer","data":{"observations":["residual: wrong node must never file"]}}'
        ] | str join "\n"
    )
)
if ($al | length) != 1 {
    fail $"actionable-from-journal wrong count: ($al | to json -r)"
}
if not ($al.0 | str contains "unbounded retry") {
    fail "actionable-from-journal dropped the marker finding text"
}

# Filing gate, file-level (tmp fixture): a marker-matching observation
# is extracted (would file); a plain non-blocking note is NOT (absence
# of filing is the behavior — it stays journal-visible only).
let gtmp = (mktemp -t closeout-gate.XXXXXX.jsonl)
[
    '{"node":"reviewer","data":{"painpoints":[],"observations":["Residual: contract change rippled to two callers, follow-up needed."]}}'
] | str join "\n" | save -f $gtmp
if ((journal-actionable $gtmp) | length) != 1 {
    fail "marker-matching observation not actionable (would NOT file)"
}
rm -f $gtmp

let ptmp = (mktemp -t closeout-plain.XXXXXX.jsonl)
[
    '{"node":"reviewer","data":{"painpoints":[],"observations":["Noted but not blocking: style nit in a comment, purely informational."]}}'
] | str join "\n" | save -f $ptmp
if ((journal-actionable $ptmp) | is-not-empty) {
    fail "plain non-blocking note is actionable (would file — seeds-ad6c/seeds-436a waste class)"
}
if ((journal-actionable $ptmp) | is-not-empty) or ((journal-nonblocking $ptmp) | is-empty) {
    fail "plain note visibility drifted (must stay non-blocking, never actionable)"
}
rm -f $ptmp
if ((journal-actionable "/nonexistent/.fabro/journal/none3.jsonl") | is-not-empty) {
    fail "missing journal did not degrade to empty (actionable path)"
}

# --- Deferred-action sweep (fabro-7aac) --------------------------------
# Marker matching: `deferred-action:` at the START, case-insensitive,
# whitespace-tolerant; mid-sentence mentions and plain observations
# never match.
if not (is-deferred-action "deferred-action: regen the TS client locally (openapi-generator absent in sandbox)") {
    fail "is-deferred-action dropped the start marker"
}
if not (is-deferred-action "  Deferred-Action: confirm bun build with user") {
    fail "is-deferred-action is case/whitespace sensitive"
}
if (is-deferred-action "we noted a deferred-action: mid-sentence mention") {
    fail "is-deferred-action matched a mid-sentence marker mention"
}
if (is-deferred-action "ordinary observation, no marker") {
    fail "is-deferred-action matched a plain observation"
}

# Marker strip: deferred-text yields the action text after the marker;
# non-matching input passes through (defensive path).
if (deferred-text "deferred-action: run bun generate locally") != "run bun generate locally" {
    fail $"deferred-text strip wrong: (deferred-text 'deferred-action: run bun generate locally')"
}
if (deferred-text "plain text") != "plain text" {
    fail "deferred-text passthrough broken"
}

# Journal parsing: implementer-node observations only; reviewer-node
# marker observations and non-matching implementer observations never
# file; unparsable lines degrade away.
let dj = (
    deferred-from-journal (
        [
            '{"node":"reviewer","data":{"observations":["deferred-action: reviewer is not the sweep source"]}}'
            '{"node":"implementer","data":{"observations":["wrote the sweep, nothing deferred"]}}'
            '{"node":"implementer","data":{"observations":["deferred-action: regen fabro-api-client via bun run generate (sandbox lacks java/openapi-generator)"]}}'
            '{"node":"tester","data":{}}'
            'not json at all'
        ] | str join "\n"
    )
)
if ($dj | length) != 1 {
    fail $"deferred-from-journal wrong count: ($dj | to json -r)"
}
if not ($dj.0 | str contains "bun run generate") {
    fail "deferred-from-journal dropped the action text"
}

# Null path, fixture journal (tmp file): a real implementer record with
# NO marker observations must yield an empty list — and an empty list
# means zero seeds create calls in the sweep loop. Also proves the
# missing-journal path degrades to empty.
let dtmp = (mktemp -t closeout-deferred-null.XXXXXX.jsonl)
'{"node":"implementer","data":{"painpoints":[],"observations":["Clean pass: sweep added, smoke green, nothing deferred."]}}' | save -f $dtmp
if ((journal-deferred $dtmp) | is-not-empty) {
    fail "deferred null-path fixture journal produced actions (would file seeds)"
}
rm -f $dtmp
if ((journal-deferred "/nonexistent/.fabro/journal/none3.jsonl") | is-not-empty) {
    fail "deferred missing journal did not degrade to empty"
}

# Filed-seed title: excerpt + provenance, bounded length.
let dft = (deferred-title "regen the generated TS client locally with the pinned generator" "fabro-7aac")
if not ($dft | str starts-with "Deferred follow-up from fabro-7aac:") {
    fail $"deferred-title missing prefix: ($dft)"
}
if ($dft | str length) > 140 {
    fail "deferred-title unbounded excerpt"
}

print "closeout-smoke: ok — reviewer-journal and deferred-action sweep logic verified"


# Sourcing closeout.nu imports its `def main`; nu auto-invokes it after
# the top level runs — exit explicitly so the smoke never reaches it.
exit 0

