#!/usr/bin/env nu
# Rotate sprint notes: .iterate-state.json keeps the newest $keep notes and
# every older one moves to .iterate-state-archive.jsonl — ONE JSON RECORD per
# line ({"note": …}), oldest first, append-only (records, not bare strings:
# a line must answer `.note` like every other JSONL file in this repo).
# The state file stays small enough to read whole (it had grown to 33 KB at
# 40 notes) while the history stays in the repo, one git-diffable line per
# note.
#
# Ported from brightctl 2026-10-17 (scripts/rotate-sprint-notes.nu there;
# that repo measured the 50-KB read-tool limit at 222 notes).
#
# Deterministic: the same state file always yields the same archive lines
# and the same remaining state. Idempotent: a note already in the archive is
# never moved twice, so an interrupted run repairs itself on rerun — the
# archive is written BEFORE the state, see below.
#
# Run from the repo root:  nu scripts/rotate-sprint-notes.nu

def main [
    --keep: int = 10,   # notes that stay in the state file
    --dry-run,          # report the split, touch nothing
] {
    let state_path = ".iterate-state.json"
    let archive_path = ".iterate-state-archive.jsonl"
    if not ($state_path | path exists) {
        error make {msg: $"($state_path) not found — run this from the repo root"}
    }
    let state = (open $state_path)
    let notes = ($state.notes | each { |n| $n })
    let total = ($notes | length)
    if $total <= $keep {
        print $"nothing to rotate: ($total) notes, keep ($keep)"
        return
    }
    let stay = ($notes | skip ($total - $keep))
    let leaving = ($notes | take ($total - $keep))
    let archived = if ($archive_path | path exists) {
        let old = (open --raw $archive_path)
        if ($old | is-empty) { [] } else {
            $old | lines | where { |line| not ($line | is-empty) } | each {
                let parsed = (from json)
                # fail loud on a foreign shape — a silent unwrap would let
                # duplicates into the archive instead of erroring
                if (not (($parsed | describe) | str starts-with "record") or not ("note" in ($parsed | columns))) {
                    error make {msg: $"archive line is not a {note: …} record: ($parsed | to json --raw)"}
                }
                $parsed.note
            }
        }
    } else { [] }
    let fresh = ($leaving | where { |note| not ($note in $archived) })
    let fresh_count = ($fresh | length)
    let archived_count = ($archived | length)
    let stay_count = ($stay | length)
    print $"rotate ($fresh_count) of ($leaving | length) notes: ($total) -> ($stay_count) stay, archive grows to ($archived_count) + ($fresh_count)"
    if $dry_run { return }
    # The archive is written FIRST: a crash between the two writes then
    # leaves the note in BOTH places, and the fresh-filter above dedupes it
    # on rerun — the other order would lose the note outright.
    if $fresh_count > 0 {
        let lines = ($fresh | each { |note| {note: $note} | to json --raw })
        let addition = ($lines | str join "\n")
        let body = if ($archive_path | path exists) {
            # the stored file ends with a newline — joining raw would put an
            # EMPTY LINE between old and new; trim the tail, join once
            let old = (open --raw $archive_path | str trim --right --char "\n")
            if ($old | is-empty) { $addition } else { $"($old)\n($addition)" }
        } else { $addition }
        $"($body)\n" | save --force $archive_path
    }
    $state | update notes $stay | to json --indent 2 | $"($in)\n" | save --force $state_path
    print $"archived ($fresh_count), state holds ($stay_count) notes"
}
