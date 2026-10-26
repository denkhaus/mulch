# ADR-0006: Canonical-domain alias resolution (port extension)

- Status: Accepted
- Date: 2026-10-25
- Deciders: user + local iterate loop (sprint 51, seed mulch-b0f7)
- Context: mulch-the-format's product direction (ADR-0023 pt 6 in
  denkhaus/fabro) names domain fragmentation as a standing defect:
  records scatter across `dev-loop`/`devloop`/`loop`-class names
  because every tool invents its own domain spelling. The reference
  `ml` 0.10.7 has NO alias mechanism — and (probed) it IGNORES an
  unknown `domain_aliases` config block, so a port-side block is
  drop-in-compatible: the reference simply does not resolve.

## Decision

A strictly-additive `domain_aliases` config block (alias → canonical,
config order) with ADDRESS-LEVEL resolution:

```text
domain_aliases:
  devloop: develop     # chains are legal (cycle-checked)
  develop: dev
```

- **Validation at open** (`Config::validated_domain_aliases`, typed
  `Error::DomainAliases`): both sides are strings; the target side is
  a configured domain or another alias; no alias collides with a
  configured domain or itself; the graph is acyclic. A malformed
  block fails loudly — never silently dropped.
- **The same invariant at WRITE** (mulch-8deb): the config-write seam
  refuses to introduce a state the open would reject — registering or
  adopting a name that is a live alias, or removing a canonical that
  live aliases still resolve to (the typed error lists the dependent
  names). `doctor --fix` skips an alias-named orphan with a fix line
  instead of adopting it. `Config::add_domain`/`remove_domain` remain
  documented raw primitives; `StoreFiles` owns the guard, so no
  command can brick its own store.
- **Resolution** (`StoreFiles::resolve_domain`, transitive, acyclic by
  construction) at every domain-ADDRESSING entry: the
  `ensure_domain` seam (move source+target with a canonical
  same-domain guard, archive, delete, prime's requested/excluded,
  locate_record → edit/outcome/query), record's write target, the
  domain_set family (query/search/rank/ready), and compact's domain
  check. Writes always land in the canonical's file; a miss names the
  ALIAS the user typed.
- **Files and listings never change**: an alias is not a domain
  (`domains()` excludes it), no alias-named file is created, and an
  existing alias-named file stays its own file (the orphan checks
  flag it as usual). The block round-trips through config rewrites
  (raw passthrough).

## Rationale

- Address-level resolution stops FUTURE fragmentation (every command
  addressing an alias lands in one canonical file) and heals
  addressing today (`prime devloop` reads `dev`), which is the
  seed's "resolution at read/prime/query time".
- The read-VIEW merge (a prime/query of the canonical surfacing
  records from historic alias-named FILES) is deliberately NOT in v1:
  the lenient readers carry PHYSICAL line numbers that
  `updated_line_verdict` matches per file — merging inside them would
  break line addressing. That half is a follow-up seed with a
  per-consumer analysis (doctor must never merge; prime/query
  should).
- Port-first, not parity: no reference behavior exists to match; the
  block's reference-tolerance was probed before landing.

## Alternatives considered

- Read-view merge in v1: rejected on the line-addressing hazard
  above; filed as follow-up instead.
- One-hop-only aliases (no chains): simpler validation, but the
  chain case costs one loop that cycle detection already pays for.
- Destructive healing (moving alias files into the canonical):
  explicitly forbidden by the seed — files are never merged, moved,
  or rewritten.

## Consequences

- The extension is invisible to the reference (unknown block ignored,
  probed) and additive to the port; README DEVIATIONS documents it.
- `move` gains a canonical same-domain guard AFTER resolution (two
  aliases of one domain are the same target — nothing to move); the
  raw-argument check keeps its parity-first position.
- Thirteen ungated batteries pin the port law (write redirect, prime
  equality, same-domain refusal, query addressing, loud invalid block,
  add/delete-domain refusals, doctor's orphan skip, and archive/
  delete/edit/outcome/compact-apply entry writes); the config and store
  tables pin validation, resolution and the write guards.
