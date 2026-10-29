# mulch

A structured-expertise store: records of conventions, patterns,
failures, decisions, references and guides, decayed and compacted by
law. The Rust workspace is a format-compatible port of the TypeScript
`ml` CLI — one lib holds the format contract, one channel holds the
bytes.

## Language

**Law**:
A rule of the store's format contract — it holds for every consumer
and therefore belongs in the lib. Laws are tested by decision tables,
not wording.
_Avoid_: rule, logic, behaviour

**Wording**:
The exact bytes a command renders for a message. Parity-pinned, and
therefore owned by the channel, never the lib.
_Avoid_: message text, copy, error string

**Verdict**:
A law's enum answer where the reference throws or branches — the
channel renders each arm its own way (`Outcome`, `Tier`,
`FullVerdict` are verdicts).
_Avoid_: error, result (for law answers)

**Lib**:
The printless format core — everything that computes records, verdicts
and text without writing a channel.
_Avoid_: core, engine (except the hook engine, its named subsystem)

**Channel**:
The command layer: stdout/stderr, exit codes, envelopes, and every
wording.
_Avoid_: CLI layer, frontend

**Alias**:
A configured alternative NAME for a domain, resolving onto the
canonical at every addressing entry (ADR-0006). Not a domain: it
never lists, never owns a file.
_Avoid_: synonym, alternate domain

**Canonical domain**:
The configured domain an alias resolves to; the one whose file every
write lands in.
_Avoid_: target domain, real domain

**Registry row**:
One record type's declared shape — payload fields, id/dedup key,
summary law, compaction strategy. Registry-named laws get lib bodies
(ADR-0005).
_Avoid_: type definition, spec

**Behavior battery**:
The test suite pinning OUR bytes — exit codes, channel-split output,
on-disk state. Born as the parity battery (differential against the
reference binary on twin stores); ADR-0008 froze every surviving
expectation into our own pins.
_Avoid_: integration tests, e2e tests, parity battery (historical)

**Format contract**:
The `.mulch/` store stays ml-0.10.7-readable; changes are additive
only (ADR-0008). Pinned by our schema tests (`tests/format_contract.rs`
plus the lib tables), not by running the old binary.
_Avoid_: format compatibility (vague)

**Promote**:
The law that shipped strings name this binary (`mulch`), never the
retired reference's name — banners, retry echoes, session-close
commands. Historical origin: the parity era; still our voice.
_Avoid_: rename, rebrand

**Deviation**:
A documented divergence from the reference (README DEVIATIONS) —
HISTORICAL RECORD since ADR-0008: the parity duty ended, the section
documents the era's decisions and their expectations.
_Avoid_: difference, gap

**Scratch store**:
A throwaway store a test builds under a temp root, cleaned by the
test.
_Avoid_: temp store, fixture store
