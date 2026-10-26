#!/usr/bin/env nu
# Rotate sprint notes: .iterate-state.json keeps the newest $keep SPRINTS and
# every older one moves to .iterate-state-archive.jsonl — ONE JSON RECORD per
# line, oldest first, append-only. A sprint can carry SEVERAL notes, so the
# state's unit is the sprint OBJECT {"sprint": …, "notes": ["…", …]}
# (user directive 2026-10-26, structure ported from fabro: objects instead
# of bare strings, grouped per sprint — the record doubles as its own JSONL
# row, and further fields get introduced per need without touching this
# script). A line must answer `.sprint` AND `.notes` like every other
# record. The state file stays small enough to read whole while the history
# stays in the repo, one git-diffable line per sprint.
#
# Lineage: brightctl's scripts/rotate-sprint-notes.nu (retro 2026-10-17)
# -> this repo's per-note rotation -> fabro's per-sprint record rotation
# (2026-10-26) -> ported back here the same day (user directive).
#
# SESSION-OWNED (session-bookkeeping ownership rule): the iterate skill
# runs this after a sprint close. Workflows and the qualitygate
# batteries never invoke it.
#
# Deterministic: the same state file always yields the same archive lines
# and the same remaining state. Idempotent: a sprint already in the archive
# is never moved twice, so an interrupted run repairs itself on rerun —
# the archive is written BEFORE the state, see below.
#
# Run from the repo root:  nu scripts/rotate-sprint-notes.nu  (just: just rotate-notes)

def main [
    --keep: int = 10,   # sprints that stay in the state file
    --dry-run,          # report the split, touch nothing
] {
    let state_path = ".iterate-state.json"
    let archive_path = ".iterate-state-archive.jsonl"
    if not ($state_path | path exists) {
        error make {msg: $"($state_path) not found — run this from the repo root"}
    }
    let state = (open $state_path)
    # nu describes an array-of-records as `table<…>` and an empty or
    # heterogeneous array as `list…` — both are arrays, anything else
    # (the retired bare-string note era, a record, a number) is a
    # foreign shape
    let sprints_desc = ($state.sprints | describe)
    if (not (($sprints_desc | str starts-with "table") or ($sprints_desc | str starts-with "list"))) {
        error make {msg: $"state field sprints must be a list of {{sprint: …, notes: […]}} records, found: ($sprints_desc)"}
    }
    let sprints = ($state.sprints | each { |s|
        if (not (($s | describe) | str starts-with "record")
            or not ("sprint" in ($s | columns))
            or not ("notes" in ($s | columns))) {
            error make {msg: $"state sprint is not a {{sprint: …, notes: […]}} record: ($s | to json --raw)"}
        }
        $s
    })
    let total = ($sprints | length)
    if $total <= $keep {
        print $"nothing to rotate: ($total) sprints, keep ($keep)"
        return
    }
    let stay = ($sprints | skip ($total - $keep))
    let leaving = ($sprints | take ($total - $keep))
    let archived = if ($archive_path | path exists) {
        let old = (open --raw $archive_path)
        if ($old | is-empty) { [] } else {
            $old | lines | where { |line| not ($line | is-empty) } | each {
                let parsed = (from json)
                # fail loud on a foreign shape — a silent unwrap would let
                # duplicates into the archive instead of erroring; the
                # {"sprint": …, "notes": […]} record is mandatory on both
                # ends, and dedup keys on the sprint number
                if (not (($parsed | describe) | str starts-with "record")
                    or not ("sprint" in ($parsed | columns))
                    or not ("notes" in ($parsed | columns))) {
                    error make {msg: $"archive line is not a {sprint: …, notes: […]} record: ($parsed | to json --raw)"}
                }
                $parsed.sprint
            }
        }
    } else { [] }
    let fresh = ($leaving | where { |record| not ($record.sprint in $archived) })
    let fresh_count = ($fresh | length)
    let archived_count = ($archived | length)
    let stay_count = ($stay | length)
    print $"rotate ($fresh_count) of the ($leaving | length) oldest sprints: ($total) -> ($stay_count) stay, archive grows to ($archived_count) + ($fresh_count)"
    if $dry_run { return }
    # The archive is written FIRST: a crash between the two writes then
    # leaves the sprint in BOTH places, and the sprint-id dedup above
    # removes it on rerun — the other order would lose it outright.
    if $fresh_count > 0 {
        # dedup is by sprint id (above); the archive line is the state
        # record verbatim, notes array included
        let lines = ($fresh | each { |record| $record | to json --raw })
        let addition = ($lines | str join "\n")
        let body = if ($archive_path | path exists) {
            # the stored file ends with a newline — joining raw would put an
            # EMPTY LINE between old and new; trim the tail, join once
            let old = (open --raw $archive_path | str trim --right --char "\n")
            if ($old | is-empty) { $addition } else { $"($old)\n($addition)" }
        } else { $addition }
        $"($body)\n" | save --force $archive_path
    }
    $state | update sprints $stay | to json --indent 2 | $"($in)\n" | save --force $state_path
    print $"archived ($fresh_count) sprints, state holds ($stay_count)"
}
