# ADR-0005: Registry-named laws live in the lib

- Status: Accepted
- Date: 2026-10-25
- Deciders: user + local iterate loop (sprint 45, seed mulch-a2c9)
- Context: the registry (`ids.rs`) names per-type laws as data —
  `SummaryLaw`, `CompactStrategy` — filled by the config parser for
  custom types (`compact: …` in `mulch.config.yaml` is part of the
  store's format contract). But only some names had bodies in the lib:
  `records::record_summary` implements `SummaryLaw`, while
  `CompactStrategy`'s meanings (concat's blank-line join with the
  longest `name`/`title`, keep_latest's newest-body re-stamp,
  merge_outcomes' union, manual's refusal) lived inside
  `commands/compact.rs` — 1136 lines with zero `#[cfg(test)]`, reachable
  only through 16 reference-gated behaviour tests that skip without the
  reference binary on PATH. A contract-level name with no executable
  spec anywhere.

## Decision

A new printless lib module owns the mechanical merge:

```text
compact::merge(records: &[Value], row: &Row, now: &str, registry: &Registry)
    -> Outcome::{Merged(Value), Manual, Empty}
compact::stamp_id(record: &mut Value, registry: &Registry)   // reference generateRecordId
```

- The registry ROW is the parameter block. No `MergeSpec` adapter: one
  producer besides the registry itself would be a hypothetical seam.
- Refusals are VERDICTS, not errors, and carry no text. `Manual` is a
  policy answer about which `--auto` and `--apply` say different
  things (warn-and-skip vs refuse); `Empty` is unreachable through the
  CLI (groups have ≥ 2 members). The channel renders both — the same
  law-vs-wording split as ADR-0003/0004.
- `stamp_id` is public beyond `merge` because two single-record paths
  stamp the same way: `--apply`'s flag-built replacement and the
  pre-compact hook's adopted record (ADR-0003 keeps those PATHS in the
  command — not the stamp). This is the first concrete step of the
  mulch-9e28 id-law unification.
- The command keeps what is CLI-shaped: candidate discovery
  (`find_candidates`), the `--auto`/`--apply` rituals, flag parsing,
  `replacement_body`, the `payload.replacement` hook law (ADR-0003),
  and every refusal wording.

The rule this records, for future candidates (C1 governance tiering,
C5 verdict/id consolidation): **when the registry names a law, the lib
gives it a body; the channel owns only wording.** `SummaryLaw` →
`record_summary` was the existing precedent; `CompactStrategy` →
`compact::merge` is the second instance.

## Rationale

- The strategy name crosses the process boundary (config file), so its
  meaning must be testable without the CLI binary — a decision table
  over `strategy × field family` now pins what 16 skip-without-reference
  tests could only sample.
- `--auto` and `--apply` cannot drift: both stamp through the one law,
  and a structural pin (`include_str!` over the command) fails the
  build if a merge helper re-grows there.
- Smallest honest interface: the moved helpers that only the merge
  needs (`newest_first`, `string_union`, `outcomes_union`,
  `string_array`) stay private; the CLI keeps its own two-line
  accessors (`raw_text`, `type_of`, `id_of`) — three-line field reads
  are not laws, and the deletion test says "moved, not concentrated".

## Alternatives considered

- Keep the law in the command because the reference keeps
  `mergeRecords` in `compact.ts`: rejected — the reference's module
  placement does not constrain ours; the reference has no lib/channel
  split at all, and our split is the point (ADR-0002/0003).
- `Result<Value, String>` moved verbatim: rejected — the `String` is
  CLI wording ("use `mulch compact --apply` …"), exactly what the lib
  must not carry.
- A typed `CompactError`: rejected — it models `manual` as an error
  when it is a policy answer two callers render differently.
- A `MergeSpec` input struct: rejected — one producer; `&Row` already
  is the law's parameter block.

## Consequences

- The lib module's `outcomes_union` (N-record flatten) is deliberately
  NOT `upsert::merge_outcomes` (two-side merge) — same field, two laws;
  the names keep them apart and each doc points at the other.
- `stamp_id` reads the record's OWN `type` field with a `content`
  id-key fallback for unregistered types (reference
  `generateRecordId`), so it is defined for records the merge never
  sees; `merge` itself still requires a registry row and the command
  still owns discovery — including its divergence for unknown-type
  groups, which stays a command-side concern (seed mulch-2809).
- The id lands LAST in every merged record (serde_json preserves key
  order and the parity batteries compare bytes) — the positional half
  of the id law now has one owner instead of prose bullets.
