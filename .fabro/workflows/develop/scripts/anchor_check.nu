# Cited file:line anchor extraction + verification (fabro-7daf).
#
# Seed descriptions cite their evidence as `path:line` / `path:line-line`
# anchors (e.g. `scripts/evidence.nu:431`, `operations/create.rs:556-585`),
# often with the claimed content quoted right after the anchor
# ("at planner-preflight.nu:135 the declaration 'mut closed = ...'").
# Anchor rot — file gone, lines shifted out of range, cited content
# rewritten — is a strong dead-seed signal the planner previously had to
# re-derive by hand. These helpers extract and verify anchors so the
# preflight verdict table can carry an `anchors_ok` field mechanically.
#
# Fail-open: every failure degrades to "no information" (empty anchor
# list, or existence/range-only checking) — never a crash. Callers route
# candidates with flagged anchors for planner adjudication instead of
# acting on the flags themselves.
#
# Path resolution is relative to the CURRENT worktree root (passed in by
# the caller); loop-asset paths under `.fabro/` etc. are readable here
# because fs_hide binds file TOOLS, not shell/script reads.

# Collapse whitespace so quoted claims match across line wrapping.
export def norm-ws [s: string] {
    $s | str replace --all --regex '\s+' ' ' | str trim
}

# Extract {path, start, end, claim} anchor records from a seed
# description. The optional claim is the first backtick/single-quoted
# snippet of >= 8 chars within 240 chars AFTER the anchor — the usual
# "at path:N the code 'foo = bar'" citation shape. A claim is only
# paired when its quote appears BEFORE any other anchor citation in the
# scan window (positional bleed guard), so multi-anchor descriptions do
# not attach one seed's quote to another anchor.
export def extract-anchors [desc: string] {
    if ($desc | is-empty) { [] } else {
        let q = "[\u{60}']"
        let claim_pat = ("^[^" + $q + "]{0,160}?" + $q + "(?P<claim>[^" + $q + "]{5,240})" + $q)
        let raw = ($desc | parse --regex "(?P<path>[A-Za-z0-9_./@-]+\\.[A-Za-z]{1,8}):(?P<start>\\d+)(?:-(?P<end>\\d+))?")
        let dlen = ($desc | str length)
        let anchors = ($raw | each {|m|
            let e = (if ($m.end | is-empty) { $m.start } else { $m.end })
            let needle = ($m.path + ":" + $m.start + (if ($m.end | is-empty) { "" } else { "-" + $m.end }))
            let idx = ($desc | str index-of $needle)
            let from = ($idx + ($needle | str length))
            let to = ([($from + 240) $dlen] | math min)
            let tail = (if $idx < 0 { "" } else { $desc | str substring $from..$to })
            {path: $m.path, start: ($m.start | into int), end: ($e | into int), needle: $needle, tail: $tail}
        })
        let needles = ($anchors | get needle)
        $anchors | each {|a|
            let qpos = (try { [($a.tail | str index-of "'") ($a.tail | str index-of "\u{60}")] | where {|i| $i >= 0} | math min } catch { null })
            let npos = (try { $needles | where {|n| $n != $a.needle} | each {|n| $a.tail | str index-of $n } | where {|i| $i >= 0} | math min } catch { null })
            let claim = (if $qpos != null and ($npos == null or $qpos < $npos) {
                $a.tail | parse --regex $claim_pat | get -o claim | default [] | first | default ""
            } else { "" })
            {path: $a.path, start: $a.start, end: $a.end, claim: $claim}
        } | uniq | first 40
    }
}

# Workspace member src/ roots (fabro-7611): seed bodies often cite paths
# crate-relative (`operations/create.rs:556`, meaningful inside
# lib/components/fabro-workflow/src/), so a root-only join flags them
# missing_file falsely and planners cannot tell 'file gone' from 'wrong
# base directory'. Members come from the workspace Cargo.toml (glob
# entries expanded to dirs holding a Cargo.toml); each member
# contributes its `src/` dir labeled by its repo-relative path.
# Fail-open: any parse surprise degrades to [] (root-only resolution).
export def member-src-roots [root: string] {
    try {
        let members = (open ($root | path join 'Cargo.toml') | get -o workspace.members | default [])
        let dirs = ($members | each {|m|
            if ($m | str ends-with '*') {
                let base = ($m | path dirname)
                glob ($root | path join $base '*') --no-file | where {|d| ($d | path join 'Cargo.toml') | path exists}
            } else {
                [($root | path join $m)]
            }
        } | flatten)
        $dirs | each {|d|
            let src = ($d | path join 'src')
            if ($src | path exists) { {base: ($d | path relative-to $root | path join 'src'), dir: $src} }
        } | compact
    } catch { [] }
}

# Resolution stage (fabro-7611): a cited anchor path resolves against
# repo root FIRST (a root hit wins, no member retry), then against each
# workspace member src/ root in member order. Returns {full, base} on
# hit (`base` is "root" or the member's repo-relative src dir), null
# when the path exists nowhere — the caller then reports missing_file
# knowing the file is gone, not merely cited against the wrong base.
export def resolve-anchor-path [a_path: string, root: string] {
    let rp = ($root | path join $a_path)
    if ($rp | path exists) {
        {full: $rp, base: "root"}
    } else {
        let hit = (try { member-src-roots $root | where {|m| ($m.dir | path join $a_path) | path exists} | first } catch { null })
        if ($hit == null) { null } else { {full: ($hit.dir | path join $a_path), base: $hit.base} }
    }
}

# Verify one anchor against the current worktree (crate-relative paths
# retried against workspace member src/ roots, fabro-7611). Returns a
# flag record:
#   ok           file exists, lines in range, claim (if any) still present
#   missing_file cited path does not exist at root or any member root
#   out_of_range line(s) beyond EOF (or unreadable/non-UTF8 file)
#   mismatch     cited lines no longer contain the quoted claim
# Every row carries `base` — the base the path resolved against ("root"
# or a member src dir) — so planners distinguish 'file gone' from
# 'wrong base directory' on flagged rows.
export def check-anchor [a: record, root: string] {
    let resolved = (resolve-anchor-path $a.path $root)
    if ($resolved == null) {
        {path: $a.path, line: $a.start, status: "missing_file"}
    } else {
        let ls = (try { open --raw $resolved.full | lines } catch { [] })
        let n = ($ls | length)
        if $n == 0 or $a.start < 1 or $a.end > $n {
            {path: $a.path, line: $a.start, status: "out_of_range", base: $resolved.base}
        } else {
            let cited = ($ls | skip ($a.start - 1) | take ($a.end - $a.start + 1) | str join " ")
            if ($a.claim | is-empty) or ((norm-ws $cited) | str contains (norm-ws $a.claim)) {
                {path: $a.path, line: $a.start, status: "ok", base: $resolved.base}
            } else {
                {path: $a.path, line: $a.start, status: "mismatch", claim: $a.claim, base: $resolved.base}
            }
        }
    }
}

# Bare repo-path extraction (fabro-9ec3 arm 1 — closes fabro-4c81's gap
# that fabro-7daf left open): seed bodies also cite paths WITHOUT a line
# anchor ("the publish step of .fabro/workflows/develop/workflow.fabro"),
# and path-only citations were never existence-checked — exactly the
# wrong-path class that drove run 01M2NA1Z3HS7QPQR8AEWZR3GDB's planner
# burn. extract-bare-paths strips URLs, emails, and the needles of
# already-verified path:line anchors first, then collects slash-bearing
# file paths; check-bare-paths flags nonexistent ones as missing_file
# with line null, folded into the SAME anchor_flags channel — an
# extension of the existing check, not a parallel table. Same fail-open
# contract: any parse surprise degrades to no flags.
export def extract-bare-paths [desc: string] {
    if ($desc | is-empty) { [] } else {
        # extract-anchors returns {path,start,end,claim} (no raw needle);
        # reconstruct the needles so they can be blanked before bare-path
        # scanning — a path:line citation must not double-report.
        let anchored = (extract-anchors $desc | each {|a|
            $a.path + ":" + ($a.start | into string) + (if $a.end > $a.start { "-" + ($a.end | into string) } else { "" })})
        mut cleaned = ($desc
            | str replace --all --regex '\S+://\S+' ' '
            | str replace --all --regex '[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}' ' ')
        for n in $anchored {
            $cleaned = ($cleaned | str replace --all $n ' ')
        }
        $cleaned
        | parse --regex '(?<![A-Za-z0-9_./@-])(?P<path>[A-Za-z0-9_.-]+(?:/[A-Za-z0-9_.@-]+)+\.[A-Za-z]{1,8})'
        | get -o path
        | default []
        | uniq
        | first 40
    }
}

# Existence-only verification for bare paths: nonexistent citation ->
# {path, line: null, status: "missing_file"}; existing paths stay silent
# (no line content to compare against).
export def check-bare-paths [desc: string, root: string] {
    # Seed bodies cite paths both repo-rooted (lib/.../main.rs) and
    # workflow-relative ("prompts/planner.md", "scripts/planner-preflight.nu"
    # — relative to .fabro/workflows/develop/). Resolve against both roots
    # before flagging, so a legitimate relative citation is not rot.
    let roots = [$root ($root | path join '.fabro' 'workflows' 'develop')]
    extract-bare-paths $desc | each {|p|
        if ($roots | any {|r| $r | path join $p | path exists}) { null } else { {path: $p, line: null, status: "missing_file"} }
    } | compact
}
