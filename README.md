# mulch

Native Rust implementation of the [mulch](https://github.com/jayminwest/mulch)
structured-expertise format (the `ml` CLI).

**Format compatibility promise:** read+write drop-in compatible with
`@os-eco/mulch-cli` 0.10.7 (`.mulch/` directory: `mulch.config.yaml`,
`expertise/<domain>.jsonl` — one JSONL per domain — plus the soft-archive
store `archive/`). Unknown record fields are preserved on every write;
additive fields are the only sanctioned extension mechanism. The CLI
surface (`mulch init/add/record/edit/query/setup/prime/onboard/status/
validate/prune/archive/restore/search/rank/outcome/doctor/ready/sync/
delete/delete-domain/move/learn/compact/config/diff/upgrade/completions/
audit`) mirrors the reference tool until this repo's own line replaces it
(self-hosting cutover).

**Product posture:** mulch is a standalone tool — the `mulch` CLI is a
product of its own — while `crates/mulch` (the lib) is at the same time
the native expertise engine that denkhaus/fabro embeds. The lib's public
API is therefore an integration surface for fabro, not an internal
convenience; changes to it are integration-relevant. It carries the
store IO (`StoreFiles`, whose `registry()` is the merged type view —
the six builtins plus the config's validated `custom_types`, with
`REGISTRY` as the builtin table behind it), the record-domain laws —
schema validity (`schema::verdict`), record summaries
(`record_summary`), anchor validity (`anchors::validity`), and
staleness decay (`stale::StaleRule`) — and the
write-model helpers built on them (`UpsertPlan`, `OutcomeEntry`,
`value_text`), and the two-file transaction seam
(`move_to_archive`/`revive`, whose `TransactionError` names the failed
half); commands render, the lib decides.

Freeze policy (ADR-0023 in denkhaus/fabro): no upstream following. The
format is frozen on our side; we extend additively for our own reasons.
The round-trip suite stays runnable against future upstream releases as
an alarm, never as an obligation.

Phase scope beyond parity: the canonical-domain layer — alias resolution
for fragmented domain names (develop/dev-loop/develop-loop/devloop/…)
— moves to this repo per ADR-0023; it lands as an additive resolution
step, never a destructive merge of existing domains.

Format credit: [jayminwest/mulch](https://github.com/jayminwest/mulch) —
this repository is an independent implementation of that format, not a
fork. The rewrite target's CLI is published as `@os-eco/mulch-cli`
(the `ml` binary).

## DEVIATIONS

Deliberate divergences from the reference `ml` 0.10.7, each with its own
expectation (the parity rule: never silently):

- **Archive banner promotes this binary.** The reference's
  `ARCHIVE_BANNER` (src/utils/archive.ts) reads
  `# ARCHIVED — not for active use. Run \`ml restore <id>\` to revive.`
  Ours ships the same law with our binary's name:
  `# ARCHIVED — not for active use. Run \`mulch restore <id>\` to revive.`
  (the same law as the record retry-echo: shipped strings name this
  binary). Byte delta vs the reference: +3 bytes on the banner (empty
  archive file: 71 bytes vs the reference's 68).
- **`status --json` renders a clean null where the reference crashes.**
  A garbage `recorded_at` (unparsable even to JS) in the health block's
  oldest/newest range makes the reference's JSON path throw a
  `RangeError` (exit 1, no envelope); we render the winner as `null`
  (plain renders `NaNd ago`, exit 0 — identical to the reference's
  plain path). Expectation: JSON status completes with `null` where
  the reference aborts.
- **Crash paths render as clean errors.** The reference exits 1 with a
  raw Bun stack trace (no envelope) for (a) `.mulch/` without
  `mulch.config.yaml`, (b) `validate` with no `.mulch/` at all, and
  (c) malformed JSONL lines in `doctor`. We render the reference's own
  message as a handled error envelope (status-channel) or report the
  bad lines as `jsonl-integrity` findings — and `doctor --fix` then
  removes them (truly malformed lines crash the reference's strict
  checks before any report renders; comment lines it repairs
  identically). Primitive JSON lines (e.g. a bare `5`) crash the
  reference's checks/repairs the same way; we report and keep them.
  Expectation: same exit code 1, clean text instead of a stack trace.
- **`doctor --fix` repairs stores the reference crashes on.** When a
  repair rewrite (schema-validation or stale-records pass) reaches a
  kept record with an unknown or missing type and no id, the
  reference's `writeExpertiseFile` throws an unhandled
  `Unknown record type` — no report at all, store untouched. We keep
  the unknown-type record (it is flagged, never silently deleted —
  mulch-d45c) and let the writer assign the deterministic id.
  Expectation: same repair outcome for every record the reference
  processes; our run completes where the reference aborts.
- **`status` reads strictly and fails like the reference, with clean
  text instead of a stack trace.** (Amended 2026-10-04, mulch-00aa:
  probes showed the reference `status` — via `readExpertiseFile` —
  exits 1 on malformed, unknown-type, scalar and `null` lines; the
  original "status reports per-line" deviation rested on a wrong
  premise.) Arrays count as records (reference quirk). We exit 1 with
  the reference's own message shapes (`Malformed JSONL at …`, unknown
  type, plus a clean `non-object record` error where the reference
  crashes with `TypeError: raw is not an Object`). The
  `--allow-unknown-types` flag lifts only the unknown-type failure.
- **Malformed-line reasons carry serde's wording** (V8/JSC phrases
  differ); the `Malformed JSONL at <path>:<line>: <reason>. Line:
  <preview>` template and the 80-char preview are byte-identical.
- **`move` reports inbound references in directory order**, like the
  reference's `readdir` walk (both are filesystem order; a lexically
  sorted list is not guaranteed).
- **The mutating commands read records strictly** (reference
  `readExpertiseFile`): malformed lines and unregistered types abort
  with the reference's messages, and rewrites re-serialize records
  compactly (ids are generated for id-less survivors). The legacy
  singular `outcome` field normalizes exactly like the reference
  (non-object values coerce to a one-element empty entry, `null` stays
  untouched) — and `edit`/`move` abort on such records through the
  updated-record schema gate, like the reference (mulch-15d3,
  mulch-7896).
- **The store writer lock is kernel-owned and store-wide.** Every
  mutating command (`record`, `edit`, `outcome`, `add`, `delete`,
  `delete-domain`, `move`, `doctor --fix`, `init`) wraps its whole
  load→mutate→save cycle in an advisory exclusive `flock` on the
  permanent zero-byte `.mulch/.lock` (ported from seeds-0d7b,
  mulch-228d); reads and `--dry-run` runs stay lock-free. The
  reference instead locks each file with an `O_CREAT | O_EXCL`
  lockfile (`<file>.lock`) plus 30-second mtime-staleness recovery:
  a crashed holder wedges writers until the file ages out or is
  deleted by hand, and a >30-second holder can be double-admitted.
  Our lock lives on the open file description, so the kernel releases
  it on crash — no stale-lockfile machinery. `init` writes
  `.mulch/.gitignore` (`*.lock`) to keep the lock sibling out of
  version control; existing stores add that ignore once by hand.
  Contended writers wait 5 seconds (the reference's budget), then
  fail fast naming the lock file — the wording differs from the
  reference's timeout error, which advises deleting its lockfile by
  hand (meaningless for a kernel-owned lock). Store-wide granularity
  also serializes cross-domain writers (the reference's per-file lock
  does not) and covers config mutations the per-file lock leaves
  racing. The lock-free `--dry-run` carve-out matches the reference
  exactly, including its dry-run domain auto-create (which therefore
  still races — probing the reference confirmed it auto-creates in
  dry-run too).
- **Argument-parse errors use clap wording** (choice rejections,
  missing-argument hints): same channels and exit code 1, different
  text than commander's. `--type`'s choice set is the store's merged
  registry (builtins plus configured custom types, read from the
  store config before parsing — matching the reference's
  command-registration registry; a broken `custom_types` config fails
  the global config-error surface right there), so an unconfigured
  custom type rejects at parse time with this wording while the
  reference rejects with commander's. The no-store paths of `add`,
  `record`, `edit`, `outcome` and the invalid-domain `add` render as
  clean handled errors where the reference crashes or prints stack
  traces — same family as the crash-path bullet below.
- **`move` appends a newline when the target file lacks a trailing
  one.** The reference concatenates (`appendFile`), merging the moved
  record onto the last line — a corrupt store. Expectation: the moved
  record starts on its own line.
- **Config rewrites normalize nested sequences to serde_yaml's
  indentation** (`- pattern` indented 4 spaces under a 4-space key
  level; the reference's js-yaml emits 6). Scalar fields, key order,
  domain order, and the governance/shelf-life backfill match the
  reference byte-for-byte.
- **`delete-domain` cancels on EOF** (closed stdin) instead of blocking
  forever; `--yes` and `--json` behave like the reference.
- **`--format` is global with the reference's four-choice set**
  (`markdown|compact|xml|plain`; the `ids` form the reference's
  query/search help advertises is not parseable through the flag
  there either). Per-command `--help` wording differs (clap renderer;
  the reference reuses its root template with the command name
  swapped in) — help text lists only implemented commands either way.
- **Hook diagnostics: the reference's pino lines and Bun's EPIPE trace
  are not ported.** A failing hook makes the reference log a structured
  pino warn line (`{"level":40,...}` with time/pid/hostname) on stderr —
  its own log channel, not product output, silenced with
  `MULCH_LOG_LEVEL=silent` — and a hook that exits without draining
  stdin can crash the reference after its output completes (Bun 1.4.0
  dumps an `EPIPE: broken pipe` trace and exits 1). This port's engine
  never prints (callers forward each script's captured stderr in
  invocation order) and its stdin writer ignores a broken pipe, so both
  artifacts are absent by construction. Two wording-level deviations ride
  the documented malformed-JSONL class: a mutable hook's non-JSON stdout
  warns with serde's parse message where the reference carries V8's
  (`JSON Parse error: Unexpected identifier "…"`), and a `pre-prune`
  hook runs while this port already holds the store's writer lock (the
  reference takes only its per-domain locks later, in the write phase) —
  a hook that shells back into a mutating command on the same store
  fails fast here and succeeds there. Blocking, timeout, mutation,
  stdout-ignored and stderr-byte semantics match the reference.
- **`compact` errors cleanly where the reference crash-dumps on
  collection- and merge-time throws.** Two classes, both rendered as
  plain `Error:` lines (exit 1): a `manual`-strategy group reaching
  the mechanical merge (the reference throws uncaught AFTER printing
  the summary block; this port prints the summary, then the error,
  and leaves the store untouched), and — mulch-2809 — a candidate
  group whose type is not registered, reachable under
  `--allow-unknown-types`: the reference dies during candidate
  collection in `getRecordSummary` (`Unknown record type: <type>`,
  uncaught, no output at all, in every mode — analyze, dry-run, auto,
  apply; the later `mergeRecords` throw is unreachable from the CLI).
  This port raises the error at collection, so nothing prints, and
  nothing writes — even when earlier domains carried clean,
  compactable groups.
- **Canonical-domain aliases (port extension, ADR-0006).** The
  reference has no alias mechanism and ignores the `domain_aliases`
  config block (probed); this port resolves alias domains onto
  canonical ones at every domain-addressing entry (writes land in the
  canonical's file; `prime`/`query`/… through an alias address the
  canonical; move guards two aliases of one domain as the same
  target). The read-VIEW merge (mulch-7112): the read family (query,
  search, rank, ready, prime, status) surfaces records stranded in
  historic alias-named files under the canonical — canonical file
  first, then alias files in config order, no dedup, no file ever
  moved or merged; `status` marks the merged count (`3 records (+2
  from aliases)` plain, additive `from_aliases` json field, only when
  nonzero). Writers and line-addressed consumers (edit/outcome,
  doctor, move's scan, record, delete, archive, compact, prune,
  restore) stay per-file — a stranded record surfaces in reads but
  must be moved explicitly to be edited or deleted. Strictly
  additive: an alias-free store is byte-identical, an invalid block
  fails loudly. Expectation: identical behavior under the reference
  except that aliases resolve — the reference sees the block as inert
  config.
- **Alias collisions are refused at WRITE, not only at open
  (mulch-8deb).** The open-time validation rejects a name that is
  both a configured domain and an alias, and a canonical that is not
  configured. Three mutators could introduce exactly that state by
  writing — `add <alias-name>`, `delete-domain <canonical that live
  aliases still resolve to>`, and `doctor --fix` adopting an
  alias-named `.jsonl` file (the historic-fragmentation class ADR-0006
  targets) — so all three refuse the mutation: `add`/`delete-domain`
  fail with the typed `invalid domain_aliases` error, `doctor --fix`
  skips the file with a `Skipped orphaned domain …` fix line. The
  reference (no alias block) writes the name happily. Expectation:
  same store files as the reference except the refused mutations,
  which never write — no command can brick its own store.
- **Hook parse-error texts are runtime vocabulary (mulch-5708).** When
  a hook prints non-JSON on stdout, the mutation-ignored warning
  embeds the parser's message — the reference's runtime (Bun/Node
  differ among themselves) vs ours (serde). Same law, runtime texts;
  batteries normalize the parser tail instead of pinning it.
- **Three doctor checks evaluate as unconditional pass** on clean
  stores only: `domain-violations`, `domain-rules-compatibility`,
  `decay-config`. Their warn/fail semantics land with the later
  CLI-parity slices; until then they hold the reference's clean-store
  output shape. The `duplicates` check left this list (mulch-7ac9):
  it now computes the reference's per-record duplicate report. The
  `orphaned-domains` and `file-anchors` checks left it (mulch-42f2):
  both directions warn with the reference's detail lines, and
  `--fix` adopts orphan files unvalidated, creates missing domain
  files, and strips broken `files[]`/`dir_anchors[]`/`evidence.file`
  anchors in pass order. The
  `governance` check left it earlier (mulch-dd36): it computes the
  reference's per-domain threshold report (warn at
  max_entries/warn_entries, FAIL at hard_limit, over the effective
  governance block; never fixable).
- **`doctor` duplicates reports a class the reference misses.** The
  check matches the reference on duplicate records — registry dedup
  field per type, warn with per-record details
  (`domain: duplicate <type> at index N (matches #M)` over parsed
  records), `--fix` never touches them. Additionally a repeated record
  id with divergent content (a content-hash id whose record was edited
  in place) fails our report with `divergent id` details where the
  reference still reports `No duplicates` and exits 0; ours exits 1.
  On a store with both classes ours fails with the combined message
  `N duplicate record(s) found; M divergent id(s) found` where the
  reference only warns (exit 0). Never auto-fixed.
- **`doctor` upgrade check is inert.** The reference warns when the npm
  registry offers a newer version. The native binary has no npm
  registry; the `upgrade` check reports pass with a cargo hint.
  Expectation: check name and order stay, status does not.
- **`--version` reports this binary's version**, not the reference's.
- **Absurd numeric durations above the i64 range** (≈ 9.2×10^18 ms
  ≈ 292 million years) store in serde's f64 byte form where
  `JSON.stringify` expands full digits below 10^21 — the same NUMBER,
  different bytes. Values through i64::MAX (including JS's rounding
  above 2^53, e.g. `9007199254740993` → `9007199254740992`) stay
  byte-identical; the battery pins that range.
- **`doctor --fix` removes malformed JSONL lines** instead of crashing
  (the reference crashes on them before fixing anything); stale records
  are pruned, schema-invalid and malformed records removed — all
  hard-deleted, the domain file left empty, the domain stays
  registered. Expectation: fixes print as `Pruned`/`Removed` lines and
  the exit code stays the PRE-fix check result.
- **Self-referential command strings name THIS binary, everywhere**
  (user directive 2026-10-13): the missing-flags `Retry` hint (and its
  `--json` `Example:` form), `prime`'s whole workflow text (headers,
  quick references, session-close footers, manifest, contract — every
  `ml <cmd>` the reference's text bodies carry), the unknown-domain
  hints of `query`/`search`/`rank`/`prime`, the no-domains lines of
  `status`/`query`, and `move`'s archived/required-fields messages all
  say `mulch <cmd>` where the reference says `ml <cmd>`. Every other
  byte stays reference-identical; the batteries promote the reference
  side for byte comparisons and pin ours explicitly.

- **`ready` ordering of invalid dates is stable, not JSC's.** The
  reference sorts with `(a, b) => new Date(b).getTime() - new Date(a)
  .getTime()`; when either side is an invalid date the comparator
  returns `NaN`, and JSC's `Array#sort` then produces an
  algorithm-specific order (probed: five alternating valid/NaN rows
  come out `A B E C D`). We use a stable sort where an invalid date
  compares equal, which is exact for every all-valid store and for
  the small (n <= 4) and all-NaN fixtures the battery pins.
  Expectation: deterministic insertion order for invalid-date ties,
  never a crash.
- **No ANSI colour, ever.** The reference colours `ready` (and its
  siblings) through chalk when a TTY or `FORCE_COLOR` is present;
  piped output is plain, which is what the batteries compare. We
  never emit ANSI, under any environment. Expectation: byte parity on
  piped output.
- **Domain flags do not walk the JS prototype chain.** The reference
  checks `domain in config.domains`, so `--domain
  constructor|toString|hasOwnProperty|valueOf` is accepted (empty
  output, exit 0) and `--domain __proto__` reaches the name
  validator. We report the unknown domain (exit 1). Applies to
  `ready` and `rank` alike. Expectation: a domain must be a
  configured key.

- **`onboard` ships this binary's snippet: promoted spellings and the
  port's version.** The reference's snippet tells agents to run
  `ml prime` / `ml record` / … / `ml --help`; ours says
  `mulch prime` / `mulch record` / … / `mulch --help` (the same law
  as the archive banner and record retry-echo — including the
  `mulch --help` mention, which the word-level promote rewrite cannot
  reach). The embedded version and its currency marker
  (`<!-- mulch-onboard:v<version> -->`) are this port's, so a
  reference-written snippet reports `outdated` and is refreshed on
  the next `mulch onboard` — snippet currency tracks the INSTALLED
  binary by design. Everything else (candidate order CLAUDE.md /
  .claude/CLAUDE.md / AGENTS.md, the create/append/update/migrate/
  up-to-date actions and messages, `--check`, `--stdout` (no trailing
  newline, quiet- and json-immune), `--quiet`, `--provider` accepted
  and ignored, the `prime.session_close` footer presets, the json
  `{file, action}` envelopes, exit codes) is byte-parity.
- **`setup` ships promoted integrations and a native-honest recipe
  subset.** The generated content names THIS binary everywhere the
  reference writes `ml`: the Claude `settings.json` hook command
  (`"mulch prime"`), the Cursor rule's prose and fenced command
  (`mulch prime`, `mulch prime --files …`), the Codex AGENTS.md
  section prose, the Codex `.codex/config.toml` hook command, the
  Codex toml fence marker (`# mulch:start — managed by `mulch setup
  codex`` — the marker names the invocation, so a reference-written
  toml block is not detected by this port and vice versa), and the
  unknown-provider hint (`mulch setup --list`). The git-hook section
  bytes already say `mulch` in the reference and are byte-identical.
  Recipe discovery resolves filesystem `.sh` scripts (spawned with
  the action as argv and `MULCH_RECIPE_NAME`/`MULCH_RECIPE_ACTION`
  in the env) and the built-ins only: a `.mulch/recipes/<name>.ts`
  recipe resolves to a one-line error (`Recipe "<name>" at <path> is
  a TypeScript recipe — this native build cannot import TypeScript
  recipes; …`), a locally installed `mulch-recipe-<name>` npm
  package likewise (`… cannot load npm recipes; …`), and `--list`
  neither probes nor reports npm shadows — its plain footer states
  our actual order (`filesystem (.mulch/recipes/<name>.sh) →
  built-in`) instead of the reference's npm-inclusive sentence, and
  the provider-argument help drops the npm mention. Everything else
  (the three provider lifecycles with their exact messages, the
  git-hook install/check/remove cycle including the chmod and the
  only-shebang file deletion, the ✔/✖ channel laws with the yellow
  stdout `--check` arm, the sticky exit when a failing `--hooks`
  pass precedes a succeeding provider, the json envelopes with
  absolute recipe paths, exit codes) is byte-parity.
- **`upgrade` reports instead of self-upgrading.** This binary is
  cargo-installed, not the npm package: the reference's registry
  probe (`npm view @os-eco/mulch-cli version`) and its install step
  (`bun install -g @os-eco/mulch-cli@latest`) describe a distribution
  this port was never part of, so neither runs. `mulch upgrade`
  (and `mulch upgrade --check` — the flag parses, there is no
  install to skip) exits 0 with the doctor-check stub's vocabulary:
  plain `✓ Native binary v<version> — update via cargo install
  (registry check not applicable)`, suppressed by `--quiet`; json
  keeps the reference's field names (`current`, `latest`,
  `upToDate`, `updated`) with honest values — `latest` and
  `upToDate` are `null` (unknown), `updated` is `false`, plus a
  `note` explaining the divergence. Expectation: no network access,
  no install, exit 0, the report above.
- **`diff` sorts domains by bytes, not locale collation.** The
  reference orders diff sections with JS `localeCompare`; we order by
  raw bytes — identical for same-case domain names (the probed set:
  `_`-leading, embedded hyphens), observably different only for MIXED
  case (`Beta` sorts before `asset` here, after in the reference's
  collation). Byte order is environment-independent; ICU collation is
  not worth the dependency. Expectation: same order as the reference
  for same-case domains, our pinned order for mixed case (pinned
  ungated by `diff_sorts_domains_by_bytes_not_locale_collation`).
- **`completions` word lists name THIS binary's implemented
  commands.** The reference's hand-rolled scripts (not commander's
  generator — template bytes are otherwise identical for bash, zsh,
  fish) complete its 30-command surface; ours completes the
  implemented subcommands in declaration order plus the global longs
  (`--version --json --quiet --verbose --timing
  --allow-unknown-types --allow-domain-mismatch --format` — help
  excluded, like the reference). Both `complete` registrations ship
  on both binaries (`complete -F _mulch mulch` AND
  `complete -F _mulch ml`, and the fish/zsh twins) — the reference
  deliberately registers both names. Unsupported shells match the
  reference exactly: `✗ Unsupported shell: "<raw>". Supported:
  bash, zsh, fish` on stderr, exit 1, identical in `--json` and
  `--quiet`; shell names match case-insensitively; a missing
  positional is clap's wording (the argument-parse deviation family).

## Status

Bootstrap. The work is tracked in this repo's own `.seeds/` tracker
(denkhaus/seeds format): format core + round-trip suite, CLI parity,
and the self-hosting cutover (the `mulch` binary replacing `ml` for this
repo's own `.mulch/` store) are the bootstrap seeds. The develop loop
runs on the fabro platform against `origin/main` — no upstream mirror.

## Toolchain

This repo deliberately owns NO toolchain image (reuse is key): runs
execute on the server-managed `mulch-toolchain` environment pinning the
shared `ghcr.io/denkhaus/seeds-toolchain:<sha12>` image (owned and built
by denkhaus/seeds). Environment-level changes land in the seeds repo.

## Build & test

```
cargo build --workspace
cargo nextest run --workspace
cargo +nightly-2026-09-22 fmt --check --all
cargo +nightly-2026-09-22 clippy --workspace --all-targets -- -D warnings
```

License: MIT OR Apache-2.0.
