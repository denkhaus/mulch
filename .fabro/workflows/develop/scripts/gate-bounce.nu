#!/usr/bin/env nu
# Gate-bounce known-bug enrichment (fabro-56f4): deterministic, never an LLM.
#
# Sits on the Gate red path between tester and implementer. Run evidence
# (01M1XN2KWG6T0PHRTJWZ8R0AZX): implementer@2 re-derived the root cause of a
# gate failure from a 186 KB log blob (704 s, $0.38) although the exact
# failure was already filed as open seed fabro-febd. This script matches the
# gate failure tail against OPEN seeds carrying the `workflows` label and
# prints the matching seed bodies as JSON, which the engine stores under
# `output.gate_known_bug_hits` (command-node output_schema mechanism) and
# renders inline in the implementer's ## Context section.
#
# Best-effort semantics (the bounce must never be stranded): every internal
# failure — seeds unavailable, unreadable tail — prints a brace-free warning and
# exits 0 with empty hits. Warnings MUST stay brace-free: the engine merges
# stderr into the captured stdout (`exec 2>&1`) and the output_schema
# validator scans for balanced JSON objects, so braces in warnings would
# poison the extraction.
#
# Matching is deterministic and case-insensitive: distinctive compounds —
# runs of at least 6 chars of [a-z0-9_/-] starting with a letter that carry a
# separator or digit (e.g. `target/debug/fabro`, `get_graph_returns_svg`,
# `fabro-server`, `openapi-generator-cli`), or a rare word of >= 12 chars —
# extracted from each seed's title+description must appear as substrings of
# the lowercased failure tail; a seed matches when at least 2 distinct
# compounds hit. Bare short words (`server`, `target`, `render`) occur in
# every gate log AND in most seed prose — counting them produced immediate
# false positives (fabro-5453/fabro-0195 matching an unrelated febd tail),
# so only separator-bearing compounds and long rare words count.

# Gate-log generics >= 6 chars: no matching signal, present in every tail.
const STOPWORDS = [
    "should" "would" "because" "without" "already" "between" "through"
    "during" "before" "prefer" "instead" "missing" "failure" "failures"
    "failed" "evidence" "implementer" "deterministic" "deterministically"
    "revisit" "directions" "update" "status" "second" "single" "problem"
    "context" "preamble" "workflow" "tracker" "project" "output" "rustfmt"
]

# Tail window: the failure signature lives at the END of the tester's
# captured output (the engine pipes the full command.output, ~186 KB in the
# worst observed case — only the tail carries the deterministic error).
const TAIL_LINES = 120

# Bounded enrichment: at most 3 seeds, description capped so the whole
# payload stays around ~4 KB.
const MAX_HITS = 3
const DESC_MAX_CHARS = 1200

# A seed must have at least this many distinct compound hits to match —
# one shared word is coincidence, two is a signature.
const MIN_COMPOUND_HITS = 2

def warn [msg: string]: nothing -> nothing {
    # Brace-free on purpose — see header.
    print -e $"gate-bounce: ($msg)"
}

def empty_hits []: nothing -> nothing {
    print ({hits: []} | to json -r)
}

def tail_of [text: string]: nothing -> string {
    let n = ([($text | lines | length) TAIL_LINES] | math min)
    if $n == 0 { "" } else { $text | lines | last $n | str join "\n" }
}

# Truncate a seed description at a codepoint boundary, marking the cut.
def bounded_description [desc: string]: nothing -> string {
    if ($desc | str length) <= $DESC_MAX_CHARS {
        $desc
    } else {
        ($desc | str substring 0..<$DESC_MAX_CHARS) + " … [gate-bounce: truncated]"
    }
}

# Distinctive compounds of a seed's title+description present in the tail.
def compound_hits [seed: record, tail: string]: nothing -> int {
    let text = ($seed.title + "\n" + ($seed.description | default ""))
    let tokens = (
        $text
        | str lowercase
        | parse --regex '(?<token>[a-z][a-z0-9_/-]{5,})'
        | get token
        | uniq
        | where {|t| not ($t in $STOPWORDS) }
        | where {|t| ($t =~ "[_/-0-9]") or ($t | str length) >= 12 }
    )
    ($tokens | where {|t| $tail | str contains $t } | length)
}

def main []: nothing -> nothing {
    # Non-tty stdin (closeout.nu pattern): nu's `input` raises on pipes, an
    # external `cat` inherits the engine's piped stdin.
    let cat_res = (do { cat } | complete)
    let raw = ($cat_res.stdout | str join)
    if $cat_res.exit_code != 0 or ($raw | is-empty) {
        warn "unreadable or empty gate tail on stdin — no known-bug enrichment"
        empty_hits
        return
    }

    let tail = (tail_of $raw | str lowercase)
    if ($tail | is-empty) {
        warn "gate tail reduced to nothing — no known-bug enrichment"
        empty_hits
        return
    }

    if (which seeds | is-empty) {
        warn "seeds not on PATH — no known-bug enrichment"
        empty_hits
        return
    }

    let res = (do { seeds list --format json --limit 200 } | complete)
    if $res.exit_code != 0 {
        let detail = ($res.stderr | str trim | str replace --all '{' '' | str replace --all '}' '')
        warn $"seeds list unavailable: ($detail) — no known-bug enrichment"
        empty_hits
        return
    }

    let seeds = (do -i {
        $res.stdout
        | from json
        | get issues
        | default []
        | where status == "open" and ("workflows" in ($it.labels? | default []))
    } | default [])

    let matched = (
        $seeds
        | each {|s| {seed: $s, hits: (compound_hits $s $tail)} }
        | where hits >= $MIN_COMPOUND_HITS
        | sort-by -r hits
        | first $MAX_HITS
    )

    let payload = {
        hits: (
            $matched
            | each {|m|
                {
                    id: $m.seed.id
                    title: $m.seed.title
                    description: (bounded_description ($m.seed.description | default ""))
                }
            }
        )
    }
    print ($payload | to json -r)
}
