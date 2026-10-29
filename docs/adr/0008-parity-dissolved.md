# ADR-0008: Parity dissolved — mulch defines its own behavior

Date: 2026-10-29 · Status: accepted · Seeds: mulch-96ed, mulch-ed39
(user decision, same session) · Supersedes: the executable-comparison
regime of ADR-0023's parity phase (the format contract of ADR-0023
stays).

## Context

mulch reached feature parity with ml 0.10.7 (mulch-0e39 closed at
sprint 56: name-for-name, position-for-position command surface,
every command under a differential battery) and cut over to
self-hosting (ADR-0007). The differential battery proves parity by
running BOTH binaries and comparing bytes — maintaining that
comparison is now the remaining coupling to the retired tool. fabro
does not embed the mulch lib today (and has itself moved away from
seeds for distributed-state reasons), so no consumer forces
byte-level behavior parity either.

## Decision (user, 2026-10-29)

**The parity regime against the ml EXECUTABLE ends now.**

1. The differential battery releases the ml executable: no test
   spawns `ml`/`ml-ref`, the reference discovery and version guard
   are removed, and the npm CLI has no role in this repo's tooling.
   mulch replaces ml — "genau jetzt ist der Zeitpunkt".
2. **Format parity stays anchored, in OUR tests**: the `.mulch/`
   store contract (JSONL line shape, config schema, alias blocks,
   additive-field preservation, serialization laws) is pinned by
   schema-level tests owned by this repo — not by running the old
   binary.
3. **The format contract holds**: `.mulch/` stays ml-0.10.7-readable
   (additive changes only, the ADR-0023/0006 line). Behavioral
   flavor — wordings, defaults, comfort — is free to diverge from
   here on; byte pins that exist today remain as OUR regression
   memory and may be changed deliberately.
4. The toolchain-image question (mulch-222d) resolves as wontfix:
   if run environments ever need the binary, the image is REBUILT
   (then with nushell scripts — fabro's standard scripting language;
   noted for every future script in this ecosystem).

## Consequences

- Test count drops where tests only mirrored ml without pinning our
  own bytes (announced at the sprint ledger); every our-side pin
  survives.
- The reference source (TypeScript) remains readable as historical
  documentation for format questions; no tool depends on it.
- Future format changes get their own ADRs (the contract's
  amendment path); behavior changes need only tests pinning the NEW
  expectation.
- The DEVIATIONS section becomes historical record: it documents the
  parity era, not a live obligation.
