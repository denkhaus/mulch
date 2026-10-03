#!/usr/bin/env nu
# Fixture battery for dup-run-check.nu (fabro-4b76): exercises the closure
# identity (--self) semantics end-to-end against a synthetic git repo — no
# /tmp seeds wrappers, no dependence on the live tracker or the real
# merge-target branch. Fixture seed ids (seeds-fix*) do not exist in the
# tracker, so the tracker arm degrades to `unknown` and the verdicts below
# are driven purely by the landed-PR history the battery fabricates —
# exactly the closure-identity surface the seed asks to verify.
#
# Cases (seed acceptance criteria):
#   (a) landed implementation whose Fabro-Run trailer names the invoking
#       run, checked WITH --self        -> clean (self-closure, never duplicate)
#   (b) the same trailer WITHOUT --self -> duplicate (trailer is foreign)
#   (c) a landed implementation carrying a FOREIGN run's trailer, checked
#       WITH --self                     -> duplicate
#   (d)/(e) fabro-a32f filed-only: revisor-line landed PRs that merely
#       FILE a seed ("Revise run ...; file fabro-x" / "Revisor pass:
#       file N seeds") NEVER count as landed implementations -> clean
#   (i) fabro-0d48 filed-only: the comma/and-separated "file seeds
#       fabro-x and fabro-y" subject shape (e.g. real 706d639
#       'Improve: revise develop run 01M2Q7VVH, file seeds fabro-6ae9
#       and fabro-... (#212)') must classify filed-only, NOT as a
#       foreign implementation forcing a duplicate verdict -> clean
#   (o)/(p) fabro-395b bookkeeping: landed squash commits that merely
#       REOPEN ("fabro-X: reopen ... (#n)", e.g. real e769f791) or
#       VERIFY/move a seed ("Verify fabro-X fix in-tree; move seed to
#       in_progress (#n)", e.g. real f3c12e03) classify filed-only,
#       NOT landed implementations -> clean (incident runs
#       01M2WG2Z42N8P2QKX0K7VXT1MC / 01M2WHTHVGK360638W9H9PN2BT)
#   (f)-(h) fabro-a32f pre-planner preflight (planner-preflight.nu):
#       landed top candidate -> "Already landed"; filed-only top
#       candidate -> "Preflight done"; empty candidate list -> degraded
#       fail-open, still "Preflight done"
#   (j) fabro-9ec3 arm 2: a seed claimed on the journal of a recent,
#       UNMERGED develop run branch -> in_flight true with the run id;
#       the invoking run's own branch never marks; a branch already
#       merged into base never marks
#   (k) fabro-9ec3 arm 3: the planner output schema's brief
#       gate-command ban pattern rejects `just qualitygate` and its
#       byte-equivalent body, accepts delegated phrasing, and the
#       planner node in workflow.fabro references the schema file
#   (l)-(n) fabro-83df report-only: the script NEVER closes and NEVER
#       routes "Already landed" — duplicates (top or non-top, sha or
#       not) all route "Preflight done" as ADVISORY verdicts for the
#       planner, and the scratch tracker rows stay untouched; every
#       report carries the verdict legend. (The former fabro-ead4 live
#       close arm was retired 2026-09-19: a commit subject naming a seed
#       is not closure evidence — reopen/verify commits matched too,
#       fabro-395b; incident runs 01M2WG2Z42/01M2WHTHV.)
#
# Usage: nu .fabro/scripts/dup-run-check-fixtures.nu   (exit 0 = all pass)

# Parse-time anchor to this battery's directory (`path self` is
# parse-time only): resolves the sibling preflight script portably.
const FIXTURES_DIR = (path self | path dirname)

# Assert helper: record one case result, abort the battery on mismatch.
def expect [case: string, got, want] {
    if $got == $want {
        print $"PASS \($case\): ($got)"
    } else {
        print -e $"FAIL \($case\): got ($got), want ($want)"
        exit 1
    }
}

# Run dup-run-check for one seed id against the synthetic base and return
# the parsed JSON verdict object (one JSON line per seed id on stdout).
def check [script: path, id: string, self: string] {
    let out = (if ($self | is-empty) {
        do { nu $script $id --base origin/main } | complete
    } else {
        do { nu $script $id --base origin/main --self $self } | complete
    })
    if $out.exit_code != 0 {
        print -e $"dup-run-check exited ($out.exit_code) for ($id): ($out.stderr)"
        exit 1
    }
    $out.stdout | lines | first | from json
}


# Run planner-preflight.nu (fabro-a32f; report-only since fabro-83df)
# with a piped run id against the synthetic base and return the parsed
# routing JSON object.
def preflight [script: path, candidates: string, self: string] {
    let out = (do { $self | nu $script --base origin/main --candidates $candidates } | complete)
    if ($out.exit_code != 0) {
        print -e $"planner-preflight exited ($out.exit_code): ($out.stderr)"
        exit 1
    }
    $out.stdout | lines | last | from json
}

# Read one seed from the scratch seeds tracker via seeds (parsed issue
# record; null on failure) — never parse .seeds/issues.jsonl by hand.
def seeds-stat [id: string] {
    let r = (do { seeds show $id --format json } | complete)
    if $r.exit_code != 0 { null } else { $r.stdout | from json | get issue }
}

# Fabricate one scratch-tracker row: the exact compact-JSON-per-line
# shape seeds itself writes (verified against seeds init/create output), for
# the (l)-(n) live close-arm cases.
def fixture-row [id: string, status: string] {
    # concat, not $"($id)" interpolation: the literal parentheses around
    # the id are part of the expected description body.
    {id: $id, title: ("fixture " + $id), status: $status, type: "task", priority: 2, createdAt: "2026-09-19T00:00:00.000Z", updatedAt: "2026-09-19T00:00:00.000Z", description: ("fixture body (" + $id + ")")} | to json --raw
}

def main [] {
    let script = ('.fabro/scripts/dup-run-check.nu' | path expand)
    let home_dir = $env.PWD
    let scratch = (mktemp -d -t dup-run-check-fixtures.XXXXXX)
    let origin_dir = ($scratch | path join 'origin')

    try {
        # Synthetic upstream: bare repo the clone fetches `origin/main` from.
        ^git init -q --bare $origin_dir
        let work = ($scratch | path join 'work')
        ^git init -q $work
        cd $work
        # Explicit tracker config from the start: the dynamic tracker
        # prefix (project "seeds") must resolve for every case, including
        # the journal-claims cases that run before any tracker rows exist.
        mkdir .seeds
        ("project: \"seeds\"\nversion: \"1\"\nmax_plan_depth: 3" | save .seeds/config.yaml)
        ^git remote add origin $origin_dir
        ^git config user.email fixtures@fabro.local
        ^git config user.name 'dup-run-check fixtures'
        ^git checkout -q -b main
        ^git commit -q --allow-empty -m 'root'

        # (a)/(b) landed squash-PR implementing seeds-fix001, closed by the
        # invoking run RUN-SELF (Fabro-Run trailer names it).
        ^git commit -q --allow-empty -m 'Implement thing for seeds-fix001 (#101)' -m 'Fabro-Run: RUN-SELF'
        # (c) landed squash-PR implementing seeds-fix002, closed by a
        # FOREIGN run's trailer.
        ^git commit -q --allow-empty -m 'Implement other for seeds-fix002 (#102)' -m 'Fabro-Run: RUN-OTHER'
        ^git push -q origin main

        # Case (a): self trailer + --self -> clean, closure self, note present
        let a = (check $script 'seeds-fix001' 'RUN-SELF')
        expect 'a: self trailer with --self verdict' $a.verdict 'clean'
        expect 'a: self trailer with --self closure' ($a.implementation_matches | first | get closure) 'self'
        expect 'a: self trailer with --self closure_note' ($a.closure_note | str contains 'RUN-SELF') true

        # Case (b): same trailer, no --self -> duplicate (closure foreign)
        let b = (check $script 'seeds-fix001' '')
        expect 'b: self trailer without --self verdict' $b.verdict 'duplicate'
        expect 'b: self trailer without --self closure' ($b.implementation_matches | first | get closure) 'foreign'

        # Case (c): foreign trailer + --self -> duplicate
        let c = (check $script 'seeds-fix002' 'RUN-SELF')
        expect 'c: foreign trailer with --self verdict' $c.verdict 'duplicate'
        expect 'c: foreign trailer with --self closure' ($c.implementation_matches | first | get closure) 'foreign'

        # (d)/(e) fabro-a32f filed-only: revisor-line landed PRs that
        # merely FILE seeds never count as landed implementations.
        # ("Revise run ...; file fabro-xxx" — e.g. real a7ee183 filing
        # fabro-ea41/fabro-7aac via PR #206 — and "Revisor pass: file N
        # seeds".) Without the extended classifier these read as landed
        # implementations and would mechanically close live seeds in the
        # pre-planner preflight.
        ^git commit -q --allow-empty -m 'Revise run RUNX; file seeds-fix003 (#103)'
        ^git commit -q --allow-empty -m 'Revisor pass: file 2 seeds incl seeds-fix004 (#104)'
        ^git push -q origin main

        let d = (check $script 'seeds-fix003' 'RUN-SELF')
        expect 'd: revise-run file verdict' $d.verdict 'clean'
        expect 'd: revise-run file filed_only' $d.filed_only_matches 1

        let e = (check $script 'seeds-fix004' 'RUN-SELF')
        expect 'e: revisor file-N-seeds verdict' $e.verdict 'clean'
        expect 'e: revisor file-N-seeds filed_only' $e.filed_only_matches 1

        # (i) fabro-0d48: the 706d639 shape — 'Improve: revise develop
        # run <id>, file seeds fabro-x and fabro-y (#n)' — previously
        # matched NO classifier branch (revisors wrote 'revise develop
        # run', and 'file seeds fabro-' was uncovered), so it landed
        # among implementations with closure=foreign and line ~244's
        # foreign_impl > 0 forced verdict=duplicate even when the run's
        # own self-closure PR existed. Both mentioned seeds must read
        # filed-only -> clean.
        ^git commit -q --allow-empty -m 'Improve: revise develop run RUN-REV, file seeds seeds-fix005 and seeds-fix006 (#105)'
        ^git push -q origin main

        let i1 = (check $script 'seeds-fix005' 'RUN-SELF')
        expect 'i: file-seeds-and verdict (first seed)' $i1.verdict 'clean'
        expect 'i: file-seeds-and filed_only (first seed)' $i1.filed_only_matches 1
        expect 'i: file-seeds-and impl matches (first seed)' ($i1.implementation_matches | length) 0

        let i2 = (check $script 'seeds-fix006' 'RUN-SELF')
        expect 'i: file-seeds-and verdict (second seed)' $i2.verdict 'clean'
        expect 'i: file-seeds-and filed_only (second seed)' $i2.filed_only_matches 1
        expect 'i: file-seeds-and impl matches (second seed)' ($i2.implementation_matches | length) 0

        # (o)/(p) fabro-395b: landed squash commits that merely REOPEN or
        # VERIFY/move the named seed (the real 2026-09-19 incident shapes:
        # e769f791 'fabro-29f7: reopen gate seed, verify upstream offers
        # remain unmerged ... (#250)' and f3c12e03 'Verify fabro-6a78 fix
        # in-tree; move seed to in_progress ... (#254)') must classify
        # filed-only, never landed implementations — otherwise the
        # pre-planner preflight mechanically re-closes deliberately
        # reopened seeds.
        ^git commit -q --allow-empty -m 'seeds-fix013: reopen gate seed, verify upstream offers remain unmerged (#109)'
        ^git commit -q --allow-empty -m 'Verify seeds-fix014 fix in-tree; move seed to in_progress (#110)'
        ^git push -q origin main

        let o = (check $script 'seeds-fix013' 'RUN-SELF')
        expect 'o: reopen squash verdict' $o.verdict 'clean'
        expect 'o: reopen squash filed_only' $o.filed_only_matches 1
        expect 'o: reopen squash impl matches' ($o.implementation_matches | length) 0

        let p = (check $script 'seeds-fix014' 'RUN-SELF')
        expect 'p: verify/move-seed squash verdict' $p.verdict 'clean'
        expect 'p: verify/move-seed squash filed_only' $p.filed_only_matches 1
        expect 'p: verify/move-seed squash impl matches' ($p.implementation_matches | length) 0

        # (l)-(n) fabro-ead4 fixture history: foreign landed
        # implementations for fix010 (closes from the live arm, non-top),
        # fix011 (duplicate verdict but deliberately NO tracker row -> its
        # close aborts fail-open), and fix012 (top-position close -> the
        # real "Already landed" route). fix009 gets NO commit at all: it
        # is tracker-closed with no landed evidence — the ambiguous
        # duplicate that must stay for planner adjudication.
        ^git commit -q --allow-empty -m 'Implement dup-close arm for seeds-fix010 (#106)' -m 'Fabro-Run: RUN-OTHER'
        ^git commit -q --allow-empty -m 'Implement dup-close arm for seeds-fix011 (#107)' -m 'Fabro-Run: RUN-OTHER'
        ^git commit -q --allow-empty -m 'Implement dup-close arm for seeds-fix012 (#108)' -m 'Fabro-Run: RUN-OTHER'
        ^git push -q origin main

        # (f)-(h) fabro-a32f pre-planner preflight routing (report-only
        # since fabro-83df: no tracker writes EVER; duplicates are
        # advisory planner input, never exits or closes).
        let pre = ($FIXTURES_DIR | path join '..' 'workflows' 'develop' 'scripts' 'planner-preflight.nu')

        let f = (preflight $pre 'seeds-fix002' 'RUN-PREFLIGHT')
        expect 'f: landed top candidate routes to planner (report-only)' $f.preferred_next_label 'Preflight done'
        expect 'f: landed top candidate verdict' ($f.context_updates | get 'output.preflight' | get candidates | first | get verdict) 'duplicate'
        expect 'f: landed top candidate sha' ((($f.context_updates | get 'output.preflight' | get candidates | first | get sha | str length) >= 7)) true

        let g = (preflight $pre 'seeds-fix003' 'RUN-PREFLIGHT')
        expect 'g: filed-only top candidate route' $g.preferred_next_label 'Preflight done'
        expect 'g: filed-only top candidate verdict' ($g.context_updates | get 'output.preflight' | get candidates | first | get verdict) 'clean'

        let h = (preflight $pre '' 'RUN-PREFLIGHT')
        expect 'h: empty candidates route' $h.preferred_next_label 'Preflight done'
        expect 'h: empty candidates mode' ($h.context_updates | get 'output.preflight' | get mode) 'degraded'

        # (j) fabro-9ec3 arm 2: run-branch -> journal -> seed-id mapping.
        # RUN-LIVE claims seeds-fix007 on its journal and is NOT merged
        # into main -> in flight. The invoking run's own branch also
        # claims seeds-fix007 (self must never mark). RUN-DONE claims
        # seeds-fix008 but sits at main's tip (merged ancestor) -> not
        # in flight; that seed is the landed arm's business.
        ^git checkout -q -b run-live
        mkdir .fabro/journal
        ('{"$schema":"seeds-journal-v1","run_id":"RUN-LIVE","node":"planner","visit":1,"status":"succeeded","ts":"2026-09-17T00:00:00Z","data":{"painpoints":[],"observations":["seeds-fix007 claimed; preflight clean"]}}' | save -f .fabro/journal/RUN-LIVE.jsonl)
        ^git add .fabro
        ^git commit -q -m 'journal RUN-LIVE'
        ^git push -q origin HEAD:refs/heads/fabro/run/RUN-LIVE
        ^git checkout -q -b run-self
        ('{"$schema":"seeds-journal-v1","run_id":"RUN-PREFLIGHT","node":"planner","visit":1,"status":"succeeded","ts":"2026-09-17T00:00:00Z","data":{"painpoints":[],"observations":["seeds-fix007 claimed by self"]}}' | save -f .fabro/journal/RUN-PREFLIGHT.jsonl)
        ^git add .fabro
        ^git commit -q -m 'journal RUN-PREFLIGHT'
        ^git push -q origin HEAD:refs/heads/fabro/run/RUN-PREFLIGHT
        # run-done branches from MAIN (not run-self): its journal commit
        # must not drag RUN-LIVE's tip into main's history via the ff
        # below — RUN-LIVE has to stay unmerged for the in-flight mark.
        ^git checkout -q main
        ^git checkout -q -b run-done
        mkdir .fabro/journal
        ('{"$schema":"seeds-journal-v1","run_id":"RUN-DONE","node":"planner","visit":1,"status":"succeeded","ts":"2026-09-17T00:00:00Z","data":{"painpoints":[],"observations":["seeds-fix008 claimed but merged"]}}' | save -f .fabro/journal/RUN-DONE.jsonl)
        ^git add .fabro
        ^git commit -q -m 'journal RUN-DONE'
        ^git push -q origin HEAD:refs/heads/fabro/run/RUN-DONE
        # Fast-forward main over run-done's journal commit: run-done is
        # now a true ancestor of base while its journal still claims
        # seeds-fix008 — merged branches must never mark in flight.
        ^git checkout -q main
        ^git merge -q --ff-only run-done
        ^git push -q origin main
        ^git checkout -q main

        let j1 = (preflight $pre 'seeds-fix007' 'RUN-PREFLIGHT')
        let jrow = ($j1.context_updates | get 'output.preflight' | get candidates | first)
        expect 'j: unmerged foreign run marks in_flight' $jrow.in_flight true
        expect 'j: in_flight_run names the live run' $jrow.in_flight_run 'RUN-LIVE'

        let j2 = (preflight $pre 'seeds-fix008' 'RUN-PREFLIGHT')
        let j2row = ($j2.context_updates | get 'output.preflight' | get candidates | first)
        expect 'j: merged run branch never marks' $j2row.in_flight false

        # (l)-(n) fabro-83df report-only: scratch seeds tracker in the
        # work clone (CWD) — seeds resolves .seeds relative to CWD. The
        # script must NEVER write it: duplicates of every shape route
        # "Preflight done" as advisory verdicts and the rows stay
        # untouched (the planner owns decisions and closures).
        # tracker config written up front (project "seeds")
        [(fixture-row 'seeds-fix003' 'open')
         (fixture-row 'seeds-fix009' 'closed')
         (fixture-row 'seeds-fix010' 'open')
         (fixture-row 'seeds-fix012' 'open')] | save --append .seeds/issues.jsonl

        # (l) mixed batch: clean top, trackerless duplicate, non-top
        # duplicate — all route "Preflight done", nothing closes, no
        # close_log exists, mode stays "checked" (no aborts possible:
        # the report-only script has no failing side effects).
        let l = (preflight $pre 'seeds-fix003,seeds-fix011,seeds-fix010' 'RUN-PREFLIGHT')
        expect 'l: mixed batch routes to planner' $l.preferred_next_label 'Preflight done'
        let lrep = ($l.context_updates | get 'output.preflight')
        expect 'l: no closed field in report' ($lrep | get -o closed | default null) null
        expect 'l: no close_log field in report' ($lrep | get -o close_log | default null) null
        expect 'l: mode stays checked' $lrep.mode 'checked'
        expect 'l: trackerless duplicate verdict still duplicate' ($lrep.candidates | where {|c| $c.seed == 'seeds-fix011'} | first | get verdict) 'duplicate'
        expect 'l: legend maps duplicate' ($lrep.legend.duplicate | str contains 'merge-target base') true
        expect 'l: legend maps clean' ($lrep.legend.clean | str contains 'no landed implementation') true
        expect 'l: legend maps degraded' ($lrep.legend.degraded | str contains 'check failed') true
        expect 'l: clean top stays open' (seeds-stat 'seeds-fix003').status 'open'
        expect 'l: non-top duplicate stays open' (seeds-stat 'seeds-fix010').status 'open'

        # (m) ambiguous duplicate: fix009 is tracker-closed with no
        # landed implementation and no closing evidence — verdict
        # duplicate without a sha. Report-only: it is reported as-is,
        # nothing closes, the tracker row stays byte-identical.
        let m = (preflight $pre 'seeds-fix009' 'RUN-PREFLIGHT')
        expect 'm: ambiguous top routes to planner' $m.preferred_next_label 'Preflight done'
        let mrep = ($m.context_updates | get 'output.preflight')
        expect 'm: ambiguous duplicate verdict reported' ($mrep.candidates | first | get verdict) 'duplicate'
        expect 'm: ambiguous duplicate sha null' ($mrep.candidates | first | get sha) null
        let mrow = (seeds-stat 'seeds-fix009')
        expect 'm: tracker description untouched' $mrow.description 'fixture body (seeds-fix009)'
        expect 'm: no spurious closeReason' ($mrow | get -o closeReason | default null) null
        expect 'm: status still closed (fixture state)' $mrow.status 'closed'

        # (n) top-position duplicate with resolvable sha: the strongest
        # already-landed signal that USED to auto-close — it still only
        # routes "Preflight done"; the tracker row stays open for the
        # planner's two-branch-rule decision.
        let n = (preflight $pre 'seeds-fix012' 'RUN-PREFLIGHT')
        expect 'n: top duplicate routes to planner' $n.preferred_next_label 'Preflight done'
        let nrep = ($n.context_updates | get 'output.preflight')
        expect 'n: duplicate sha still reported' (($nrep.candidates | first | get sha | default null | str length) >= 7) true
        expect 'n: tracker shows top seed still open' (seeds-stat 'seeds-fix012').status 'open'

        # (k) fabro-9ec3 arm 3: schema ban semantics + graph wiring.
        let wf = ($FIXTURES_DIR | path join '..' 'workflows' 'develop')
        let schema_ok = (do { ^python3 -c ("import json,re;s=json.load(open('" + ($wf | path join 'schemas' 'planner-output.schema.json') + "'));p=re.compile(s['properties']['context_updates']['properties']['current_seed_brief']['pattern']);assert p.search('gate green via the deterministic tester step');assert not p.search('run just qualitygate');assert not p.search('invoke nu scripts/qualitygate.nu');print('ok')") } | complete)
        expect 'k: schema ban pattern semantics' ($schema_ok.stdout | str trim) 'ok'
        let graph = (open --raw ($wf | path join 'workflow.fabro'))
        expect 'k: planner node wired to schema' ($graph | str contains 'output_schema="@schemas/planner-output.schema.json"') true
    } finally {
        cd $home_dir
        rm -rf $scratch
    }
}
