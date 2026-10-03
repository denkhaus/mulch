#!/usr/bin/env nu
# Prompt-lint for the loop workflows: prompt/graph/toml literals must not
# rot (C3 guard — generalizes fabro-41de's revisor-only check to every
# loop asset).
#
# Checks (.fabro/workflows/{conductor,develop,revisor}/ prompts + graph +
# toml, plus .fabro/scripts/*.nu):
#   1. every `fabro-<hex>{4,}` literal resolves in the tracker (`seeds show`)
#      — an unresolvable id is provenance rot. Errors.
#   2. every `justfile:<line>` anchor must point at a NON-comment line —
#      a comment line means the anchor drifted (the planner qualitygate
#      anchor rot class). Errors.
#   3. date pins `20xx-xx-xx` older than 45 days — warnings only (human
#      review; load-bearing pins are allowed to stay).
#   4. `nu -c "..."` snippets in fenced shell/bash blocks of ANY workflow's
#      prompts (`.fabro/workflows/**/prompts/*.md`) whose payload contains
#      `$` — bash double-quote interpolation strips nu variables
#      (mx-e606d5; run 01M2N36HV2). Errors; single quotes pass.
#
# Exit 1 on any error, 0 otherwise (warnings pass).

def lint-files [] {
    (glob .fabro/workflows/conductor/**/*)
    | append (glob .fabro/workflows/develop/**/*)
    | append (glob .fabro/workflows/revisor/**/*)
    | append (glob .fabro/scripts/*.nu)
    | where {|p| ([$p '.md' '.fabro' '.toml' '.nu'] | any {|ext| ($p | str ends-with $ext)})}
    | where {|p| ($p | path type) == 'file'}
}

# Tracker prefix: derived from .seeds/config.yaml (project name). Only
# ids with THIS tracker's prefix are validated — foreign-prefix ids
# (origin-loop lineage lore, documented in the develop PROJECT_FACTS
# lineage bullet) are out of scope by design (ported from the origin
# loop, where the prefix was `fabro-`).
def tracker-prefix [] {
    if (not ('.seeds/config.yaml' | path exists)) { return '' }
    open .seeds/config.yaml | get project
}

def seed-ids-in [text] {
    let prefix = (tracker-prefix)
    if ($prefix | is-empty) { return [] }
    let matches = ($text | parse --regex ($prefix + '-(?<id>[0-9a-f]{4,})'))
    if ($matches | is-empty) { return [] }
    # word-boundary guard: reject candidates directly followed by another
    # identifier char or dash (a longer non-seed word must not yield a
    # shorter hex-looking prefix)
    $matches.id | uniq | where {|id|
        not ($text | parse --regex ($prefix + '-' + $id + '(?![0-9a-zA-Z-])') | is-empty)
    }
}

def prompt-md-files [] {
    (glob .fabro/workflows/**/prompts/*.md)
    | where {|p| ($p | path type) == 'file'}
}

# check 4: double-quoted nu -c payloads containing $ inside fenced blocks
def nu-c-dollar-errors [f] {
    let text = (open --raw $f)
    mut errors = []
    for m in ($text | parse --regex '(?s)```[\w-]*\r?\n(?<block>.*?)```') {
        for s in ($m.block | parse --regex 'nu -c "(?<payload>[^"]*)"') {
            if ($s.payload | str contains '$') {
                $errors = ($errors | append $"($f): double-quoted nu -c snippet contains \$ — use single quotes")
            }
        }
    }
    $errors
}

# check 6: provenance literals in PROMPT files — run ids and PR-number
# references are evidence, not rules (fabro-41de class; user directive
# 2026-09-19: prompts stay universal and project-agnostic — provenance
# lives in the seed Basis). Errors. Scoped to prompt .md files: scripts
# and schemas legitimately carry synthetic ids and (#n) shape docs.
def provenance-errors [f] {
    let text = (open --raw $f)
    mut errors = []
    for m in ($text | parse --regex '\brun (?<r>01M[0-9A-HJ-NP-TV-Z]{10,})') {
        $errors = ($errors | append $"($f): run id literal 'run ($m.r)' — provenance belongs in the seed Basis, not in prompts")
    }
    for m in ($text | parse --regex '\b(?<b>01M[0-9A-HJ-NP-TV-Z]{20,})\b') {
        $errors = ($errors | append $"($f): bare run id literal '($m.b)' — provenance belongs in the seed Basis, not in prompts")
    }
    for m in ($text | parse --regex '\bPR #(?<p>\d+)') {
        $errors = ($errors | append $"($f): PR-number literal 'PR #($m.p)' — provenance belongs in the seed Basis, not in prompts")
    }
    $errors
}

# check 5: routing-named top-level properties in workflow @schemas/*.json —
# warnings only: a payload containing such a field opts the node into
# routing semantics even under a custom schema (fabro-a211)
def routing-field-schema-warnings [] {
    let names = [preferred_next_label, outcome, failure_reason, suggested_next_ids, context_updates]
    mut warnings = []
    for f in (glob .fabro/workflows/**/schemas/*.json) {
        if ($f | path type) != 'file' { continue }
        let schema = (try { open --raw $f | from json } catch { null })
        if $schema == null { continue }
        let props = (try { $schema | get properties } catch { null })
        if ($props == null) or (not ($props | describe | str starts-with 'record')) { continue }
        for n in $names {
            if ($props | columns | any {|c| $c == $n }) {
                $warnings = ($warnings | append $"($f): top-level property '($n)' is routing-named — a payload containing it activates routing semantics")
            }
        }
    }
    $warnings
}

def main [] {
    let files = (lint-files)
    if ($files | is-empty) {
        print -e "prompt-lint: no files found — scope broken"
        exit 2
    }
    mut errors = []
    mut warnings = []

    # 6. provenance literals in prompts (run ids, PR refs)
    for f in (prompt-md-files) {
        $errors = ($errors | append (provenance-errors $f))
    }

    let justfile_lines = (if ('justfile' | path exists) { open --raw justfile | lines } else { [] })

    for f in $files {
        let text = (open --raw $f)
        # 1. seed ids must resolve (this tracker's prefix only)
        let tracker_proj = (tracker-prefix)
        for id in (seed-ids-in $text) {
            let full = $"($tracker_proj)-($id)"
            let s = (do { seeds show $full --format json } | complete)
            if $s.exit_code != 0 {
                $errors = ($errors | append $"($f): seed id '($full)' does not resolve in the tracker")
            }
        }
        # 2. justfile anchors must not sit on comments
        for m in ($text | parse --regex 'justfile:(?<n>\d+)') {
            let n = ($m.n | into int)
            let line = (if $n < ($justfile_lines | length) { $justfile_lines | get $n } else { null })
            if $line == null {
                $errors = ($errors | append $"($f): justfile:($n) is past end of file")
            } else if (($line | str trim) | str starts-with '#') {
                $errors = ($errors | append $"($f): justfile:($n) points at a comment line — anchor drifted")
            }
        }
        # 3. old date pins warn
        for m in ($text | parse --regex '(?<d>20\d{2}-\d{2}-\d{2})') {
            let dt = ($m.d | into datetime)
            if ((date now) - $dt) > 45day {
                $warnings = ($warnings | append $"($f): date pin '($m.d)' older than 45 days — still load-bearing?")
            }
        }
    }

    # 4. double-quoted nu -c with $ in any workflow's prompt markdown
    for f in (prompt-md-files) {
        $errors = ($errors | append (nu-c-dollar-errors $f))
    }

    # 5. routing-named top-level schema properties warn
    $warnings = ($warnings | append (routing-field-schema-warnings))

    for w in ($warnings | uniq) {
        print $"warn: ($w)"
    }
    if ($errors | is-empty) {
        print $"prompt-lint: ok — ($files | length) files, ($warnings | uniq | length) warnings"
    } else {
        for e in ($errors | uniq) {
            print -e $"error: ($e)"
        }
        print -e $"prompt-lint: ($errors | uniq | length) errors"
        exit 1
    }
}
