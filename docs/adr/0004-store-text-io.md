# ADR-0004: Domain-text IO lives behind StoreFiles

- Status: Accepted
- Date: 2026-10-25
- Deciders: user + local iterate loop (sprint 44, seed mulch-593e)
- Context: `StoreFiles` owns the store's write policies — config
  rewrites, record rewrites, appends, the archive transaction. Since
  d1df776 the config and record writers share one atomic temp+rename
  (`atomic_write`), but the owner was not yet exclusive: the
  soft-archive write carried its own copy of the temp+rename (a
  second scratch-name law: `{path}.tmp.{nanos}`, no pid), and
  `doctor --fix` ran its line-preserving repair passes
  (jsonl-integrity, legacy-outcome) by reading
  `store.domain_path(domain)` with `fs::read_to_string` and writing
  it back with a plain `fs::write`. Doctor's write had no temp file
  and no rename, so a lock-free reader (status, prime, validate)
  could observe a truncated half-file while doctor rewrote it — and
  the module handed out a `PathBuf`, which invites exactly that IO
  from outside the seam.

## Decision

`StoreFiles` gains the text half of its read/write surface:

```text
StoreFiles::read_domain_text(domain)          -> Result<Option<String>>
StoreFiles::rewrite_domain_text(domain, body) -> Result<()>
```

`read_domain_text` returns `None` for an absent file (the reference's
repair passes skip silently) and rejects non-UTF-8 bytes instead of
decoding lossily — those passes re-emit the text they read, so a
decode that changed bytes would corrupt the file. `rewrite_domain_text`
writes the body verbatim (the caller owns trailing newlines) through
the same `atomic_write` as every other store write. Doctor then
speaks `body: &str` and maps store errors through the same rendering
every other command uses (`chain_message_from` in `commands/store.rs`).

The archive write loses its private copy of the law and joins
`atomic_write` too, so the module really has ONE atomicity owner
(after this sprint the only place composing a scratch path is
`atomic_write`).

`domain_path` stays public — embedders and the tests use it, and
`move`/`delete` perform whole-file lifecycle operations (rename,
unlink) for which a path is the honest interface — but its doc now
names the split: path access for the file as a subject (lifecycle,
existence probes), the methods for IO.

## Rationale

- One atomicity owner: a truncated domain file was observable through
  every lock-free reader, doctor was the last writer outside the
  seam, and the archive write was a second (pid-less) scratch law
  waiting to drift from the first.
- The interface stops inviting the mistake: a method that takes
  `&str` cannot be used to `fs::write` around the temp+rename law.
- Doctor loses its IO half instead of moving it: the two helpers that
  remain only map the store's `Error` onto doctor's handled shape.

## Alternatives considered

- Keep the raw path and make doctor use `atomic_write` directly:
  smaller diff, but `atomic_write` is a crate-internal detail and the
  next repair pass would reach for `domain_path` again.
- Shrink `domain_path` to `pub(crate)`: a cleaner public surface, but
  it breaks any embedder using the accessor for its own lifecycle
  work; rejected (user decision 2026-10-25).
- Fold `move`/`delete` onto StoreFiles too (remove/rename methods):
  whole-file lifecycle, not text IO — out of scope here, filed as a
  follow-up seed (mulch-cde9) so the remaining path users are tracked.

## Consequences

- Doctor's IO failures now render with the store's path-based chain
  message ("reading <path>: <cause>") instead of a domain-labelled
  string; every other command already spoke that shape.
- A failed write leg names the SCRATCH path, not the target — that is
  `atomic_write`'s existing behaviour (the target was never touched on
  that leg); the rename leg names the target. Doctor inherits it.
- The rewrite REPLACES the file (new inode), it does not truncate and
  refill: a hard link or an open handle keeps the old bytes, and a
  mode set on the old file does not survive. The old `fs::write` did
  the opposite. Acceptable here because the store's files are
  write-owning artifacts — the reference rewrites them whole too —
  but it is the reason the seam is the right place for the policy
  rather than each caller's guess.
- `read_domain_text` is the only text read that reports a missing
  file distinctly (`read_lenient`/strict reads still treat it as
  empty); the repair passes need the distinction to skip without
  writing.
- Store-level tests pin three claims: the round trip (verbatim body,
  absent → `None`, no scratch file left behind), the inode
  replacement (a hard link keeps the OLD bytes — the assertion a
  plain `fs::write` fails), and the typed error on non-UTF-8.
