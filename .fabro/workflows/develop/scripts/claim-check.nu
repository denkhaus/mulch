#!/usr/bin/env nu
# Claim-contract gate (fabro-c42f stopgap, 2026-09-18): the engine resolves
# stdin_source="current_seed_id" BEFORE this script runs — a missing key
# fails the node deterministically within seconds, so a planner whose
# structured output validated routing but dropped its context_updates
# (observed: 349-char {"outcome":"succeeded",...} prose-hybrid, runs
# 01M2SN3TE4FZ / 01M2SRK0PCDX / 01M2SW12V6WK) parks the run EARLY instead
# of burning a 20-30 min implementer cycle and dying at the evidence node.
# When the key exists, this script only asserts it is non-empty and
# well-formed (this tracker's seed id prefix, derived from
# .seeds/config.yaml — ported from the origin loop where it was fabro-).

def main []: nothing -> nothing {
    # Non-tty stdin: nu's `input` only works on a tty; the engine pipes the
    # context value, so read it through an external `cat` (closeout idiom).
    let raw = (cat | str join)
    let seed_id = ($raw | str trim)
    if ($seed_id | is-empty) {
        print -e "claim-check: stdin carried no seed id (stdin_source misconfigured?)"
        exit 1
    }
    let prefix = (open .seeds/config.yaml | get project)
    if not ($seed_id | str starts-with ($prefix + "-")) {
        print -e $"claim-check: stdin value is not a seed id of this tracker \(prefix ($prefix)-\): ($seed_id)"
        exit 1
    }
    print $"claim-check: ok ($seed_id)"
}
