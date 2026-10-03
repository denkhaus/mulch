#!/usr/bin/env nu
# lint-nu: side-effect-free lint for every nushell script the repo ships.
#
# Two layers, both born from real incidents:
#   1. parse  — `nu --ide-check` reports Error-severity diagnostics
#      (unbalanced delimiters, bad syntax). Parse-only by design: a bare
#      `source <file>` would auto-invoke the script's `def main`.
#   2. interpolated-regex scan — an interpolated string (`$'...'`/`$"..."`)
#      evaluates EVERY `(...)` as a subexpression, so a regex group like
#      `(?P<sha>...)` parses clean and dies at RUNTIME with
#      `Command ?P<sha>... not found` (scripts/verify.nu, run
#      01M2GVW7GGGB's implementer). `nu --ide-check` and `nu-check` both
#      miss this class — the scan is the only reliable detector.
#
# Scope: repo scripts/ plus every workflow's loop assets. The qualitygate
# (fabro-bfe1) previously walked ONLY develop scripts — this is the
# full-repo replacement, also wired into `just validate-workflows`.

def script-paths [] {
    (glob scripts/*.nu)
    | append (glob .fabro/workflows/*/scripts/*.nu)
    | append (glob scripts/**/*.nu)
    | append (glob .fabro/scripts/*.nu)
    | uniq
    | sort
}

def parse-check [file: string] {
    # --ide-check exits 0 even on parse errors: diagnostics are JSON lines
    # on stdout; an Error-severity line is the failure signal.
    let res = (do { ^nu --ide-check 10 $file } | complete)
    let errors = ($res.stdout | lines | where {|l| $l | str contains '"severity":"Error"' })
    if ($errors | is-not-empty) {
        print $"parse FAILED: ($file)"
        print ($errors | last 5)
        false
    } else {
        true
    }
}

def interpolated-regex-check [file: string] {
    # A `$'`/`$"` string containing a regex group opener `(?P<`, `(?<`, or
    # `(?:` is always the subexpression bug — plain strings or
    # concatenation are the fix.
    # Comment lines are skipped: prose about the pattern (this file's own
    # docblock) is not an occurrence.
    let text = (open --raw $file)
    let hits = ($text | lines | enumerate | where {|it|
        ($it.item | str trim | str starts-with '#') == false and (
            # Backtick raw string: regex quotes and backslashes stay literal.
            $it.item | parse --regex `\$['"][^'"\n]*\(\?(?:P<|<|:)` | is-not-empty
        )
    })
    if ($hits | is-not-empty) {
        for h in $hits {
            print $"interpolated-regex FAILED: ($file):($h.index + 1) — regex group inside an interpolated string"
            print $"  ($h.item | str trim)"
        }
        false
    } else {
        true
    }
}

def main [] {
    let scripts = (script-paths)
    if ($scripts | is-empty) {
        print 'lint-nu: no scripts found — scope broken'
        exit 2
    }
    print $"lint-nu: ($scripts | length) scripts"
    mut green = true
    for s in $scripts {
        if not (parse-check $s) { $green = false }
        if not (interpolated-regex-check $s) { $green = false }
    }
    if $green {
        print 'lint-nu: green'
    } else {
        print 'lint-nu: FAILED'
        exit 1
    }
}
