#!/usr/bin/env nu
# Stage journal hook (ADR-0009 candidate, seed fabro-176b final design).
#
# SHARED by all three loop workflows (conductor, develop, revisor): each
# workflow.toml references this ONE copy from its [[run.hooks]] block —
# never duplicate this file per workflow; a fix lands here exactly once.
#
# Fires on stage_complete (non-blocking, sandbox). Appends ONE JSON line
# per stage execution to the run's journal stream:
# .fabro/journal/<run_id>.jsonl
#
# Schema: fabro-journal-v1 — all fields required except data (object, may
# be empty). Provenance fields (node, visit, status, ts, run_id) come from
# the ENGINE hook context, never from agent claims. `data` is 1:1 the
# stage's declared journal payload from its context_updates (bridged
# engine-side: HookContext carries the completed stage's context_updates
# since fabro-31b2; a pre-bridge server or a stage that declared nothing
# yields an empty object — consumers filter by their own schema inside
# data).
#
# File identity: ONE file per run, named by run id. The per-node-file
# scheme (<node>@<visit>.json) failed merge-back: runs share a base, so
# visit counters counted across runs (run 2 wrote planner@4 for its first
# planner visit) and two runs from one base wrote colliding names — a
# guaranteed auto-PR conflict on journal files, not code. With run-id
# files, parallel and sequential runs never collide; the improve workflow
# reads one ordered stream per run instead of globbing N files.
#
# visit: run-LOCAL — lines in THIS run's file with the same node, + 1.
# First execution of a node in a run -> 1.
#
# Appends are serialized by the engine (one hook per stage completion,
# sequential stages); a crash mid-append can only truncate the LAST line —
# consumers must tolerate one unparseable trailing line.
#
# Parallel stages: this hook fires once per SEQUENTIAL stage completion.
# Parallel branch targets do not run lifecycle hooks (engine limitation);
# the parent parallel node's completion fires this hook once with the
# bundled results available in context (staged for work item 2).
#
# Legacy: pre-JSONL <node>@<visit>.json files from earlier runs stay as
# they are (their painpoints are addressed); they are not migrated and
# not written anymore.

# Run-local visit: parse THIS run's jsonl, count same-node lines. A
# truncated/corrupt last line (crash mid-append) is skipped, not fatal.
def next-visit [file: path, node: string]: nothing -> int {
    if not ($file | path exists) {
        return 1
    }
    let count = (
        open --raw $file
        | lines
        | compact
        | each {|line| try { $line | from json } catch { null } }
        | where {|entry| $entry != null and ($entry.node? | default "") == $node }
        | length
    )
    $count + 1
}

def main []: nothing -> nothing {
    let ctx_path = ($env.FABRO_HOOK_CONTEXT? | default "")
    if ($ctx_path | is-empty) {
        print "stage-journal: no FABRO_HOOK_CONTEXT — skipping"
        return
    }
    let ctx = (open $ctx_path)

    let node = ($ctx.node_id? | default "unknown")
    let event = ($ctx.event? | default "")
    if $event != "stage_complete" {
        return
    }

    let run_id = ($ctx.run_id? | default "")
    let journal_dir = ".fabro/journal"
    if not ($journal_dir | path exists) {
        mkdir $journal_dir
    }
    let out = $"($journal_dir)/($run_id).jsonl"

    # Journal bridge (engine fabro-31b2, landed): HookContext now carries
    # the completed stage's declared context_updates — data is 1:1 the
    # stage's journal payload. Absent field (pre-bridge server or a stage
    # that declared nothing) stays {} — consumers filter by schema inside
    # data, so the envelope shape never changes.
    let data = ($ctx.context_updates?.journal? | default {})

    let entry = {
        "$schema": "fabro-journal-v1",
        run_id: $run_id,
        node: $node,
        visit: (next-visit $out $node),
        status: ($ctx.status? | default ""),
        ts: (date now | format date "%Y-%m-%dT%H:%M:%SZ"),
        data: $data
    }

    # Repair-first append: a crashed earlier append can leave the last line
    # without its newline; writing after it would glue two entries into one
    # unparseable line. Ensure the file ends with a newline, then append.
    if ($out | path exists) and not (open --raw $out | str ends-with "\n") {
        "\n" | save --append $out
    }
    ($entry | to json --raw) ++ "\n" | save --append $out
    print $"stage-journal: appended ($node) visit ($entry.visit) to ($out)"
}
