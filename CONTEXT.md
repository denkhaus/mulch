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

**Parity battery**:
A reference-gated test suite comparing our bytes against the reference
binary run on a twin store.
_Avoid_: integration tests, e2e tests

**Twin store**:
The battery fixture: two identical throwaway stores, ours and the
reference's, seeded once and compared byte-for-byte.
_Avoid_: fixture pair

**Promote**:
The law that shipped strings name this binary (`mulch`), never the
reference's name — banners, retry echoes, session-close commands.
_Avoid_: rename, rebrand

**Deviation**:
A documented divergence from the reference (README DEVIATIONS), each
with its expectation — never silent.
_Avoid_: difference, gap

**Scratch store**:
A throwaway store a test builds under a temp root, cleaned by the
test.
_Avoid_: temp store, fixture store
