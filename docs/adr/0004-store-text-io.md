# ADR-0004: Domain-text IO lives behind StoreFiles

- Status: Accepted
- Date: 2026-10-25
- Deciders: user + local iterate loop (sprint 44, seed mulch-593e)
- Context: `StoreFiles` owns the store's write policies — config
  rewrites, record rewrites, appends, the archive transaction — and
  since d1df776 every one of those writes goes through one atomic
  temp+rename (`atomic_write`). Two holes remained. `doctor --fix`
  ran its line-preserving repair passes (jsonl-integrity,
  legacy-outcome) by reading `store.domain_path(domain)` with
  `fs::read_to_string` and writing it back with a plain `fs::write` —
  no temp file, no rename, so a lock-free reader (status, prime,
  validate) could observe a truncated half-file while doctor
  rewrote it. And the module handed out a `PathBuf`, which invites
  exactly that IO from outside the seam.

## Decision

`StoreFiles` gains the text half of its read/write surface:

```text
StoreFiles::read_domain_text(domain)         -> Result<Option<String>>
StoreFiles::rewrite_domain_text(domain, body) -> Result<()>
```

`read_domain_text` returns `None` for an absent file (the reference's
repair passes skip silently) and rejects non-UTF-8 bytes instead of
decoding lossily — those passes re-emit the text they read, so a
decode that changed bytes would corrupt the file. `rewrite_domain_text`
writes the body verbatim (the caller owns trailing newlines) through
the same [`atomic_write`] as every other store write. Doctor then
speaks `body: &str` and maps store errors through the shared
`chain_message_from` rendering, like every other command's `persist`.

`domain_path` stays public — embedders and the tests use it, and
`move`/`delete` perform whole-file lifecycle operations (rename,
unlink) for which a path is the honest interface — but its doc now
names the split: path access for lifecycle, the methods for IO.

## Rationale

- One atomicity owner: a truncated domain file was observable through
  every lock-free reader, and doctor was the last writer outside the
  seam.
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
  follow-up seed so the remaining path users are tracked.

## Consequences

- Doctor's IO failures now render with the store's path-based chain
  message ("reading <path>: <cause>") instead of a domain-labelled
  string; every other command already spoke that shape.
- `read_domain_text` is the only text read that reports a missing
  file distinctly (`lenient`/strict reads still treat it as empty);
  the repair passes need the distinction to skip without writing.
- A store-level test pins the round trip (verbatim body, absent →
  `None`, no scratch file left behind).
