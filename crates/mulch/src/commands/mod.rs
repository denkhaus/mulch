//! Command implementations, one module per parity-slice command.

mod active_work;
mod add;
mod archive;
mod delete;
mod delete_domain;
mod doctor;
mod edit;
mod git_context;
mod init;
mod learn;
mod move_cmd;
mod outcome;
mod prime;
mod prune;
mod query;
mod rank;
mod ready;
mod record;
mod render;
mod restore;
mod search;
mod status;
mod validate;

use std::path::Path;

/// Store lookup: the filesystem seam lives in the library
/// ([`mulch::StoreFiles`]); the CLI only matches its outcome.
pub(crate) use mulch::StoreLocation;
use mulch::{Error, ResolveError, js_string, truthy_str};

use crate::cli::{Cli, Command, EditArgs, RecordArgs};
use crate::output::{Failure, failure_envelope};

/// The handled-error message for a missing store (status et al.).
pub(crate) const NO_STORE_MESSAGE: &str = "No .mulch/ directory found. Run `mulch init` first.";

/// The no-store message of the read-modify-write commands: the
/// reference's config reader throws it (with the `Error: ` prefix the
/// command adds).
pub(crate) const NO_STORE_CONFIG_MESSAGE: &str =
    "Error: No .mulch/ directory found. Run `mulch init` to set up this project.";

/// Locates the store for the parity commands.
pub(crate) fn locate(root: &Path) -> Result<mulch::StoreLocation, mulch::Error> {
    mulch::StoreFiles::locate(root)
}

/// Opens the store or fails with the command's no-store contract.
///
/// One owner for the "no store" policy: the message variants and the
/// JSON-envelope channel live here, not in ten command prologues.
/// `config_message` selects the read-modify-write wording (the
/// reference's config reader) over the reporting wording.
pub(crate) fn open_store(
    command: &'static str,
    config_message: bool,
) -> Result<mulch::StoreFiles, Failure> {
    open_located(command, config_message, false)
}

/// Opens the store under the writer lock (mulch-228d): the advisory
/// exclusive lock on `.mulch/.lock` is held until the returned store
/// drops, so the command's whole load→mutate→save cycle is atomic
/// against other mulch processes. Every mutating command opens this
/// way; a contended lock fails fast through the shared error path
/// naming the lock file.
pub(crate) fn open_store_mut(
    command: &'static str,
    config_message: bool,
) -> Result<mulch::StoreFiles, Failure> {
    open_located(command, config_message, true)
}

/// The shared open body behind [`open_store`] and
/// [`open_store_mut`]: locate (optionally exclusive) and map the
/// no-store/error contracts once.
fn open_located(
    command: &'static str,
    config_message: bool,
    exclusive: bool,
) -> Result<mulch::StoreFiles, Failure> {
    let cwd = std::env::current_dir()
        .map_err(|source| Failure::handled(command, format!("resolving cwd: {source}")))?;
    let located = if exclusive {
        mulch::StoreFiles::locate_exclusive(&cwd)
    } else {
        locate(&cwd)
    };
    match located {
        Ok(StoreLocation::Open(store)) => Ok(store),
        Ok(StoreLocation::Missing | StoreLocation::NoConfig) => {
            let message = if config_message {
                NO_STORE_CONFIG_MESSAGE
            } else {
                NO_STORE_MESSAGE
            };
            Err(Failure::handled_on_stderr(command, message))
        }
        Err(source) => Err(match &source {
            mulch::Error::CustomTypes { message } => Failure::config_error(message),
            _ => Failure::handled(command, chain_message_from(&source)),
        }),
    }
}

/// The crate-local alias for the output module's chain renderer.
fn chain_message_from(error: &mulch::Error) -> String {
    crate::output::chain_message(error)
}

/// Runs the parsed command.
pub(crate) fn dispatch(cli: &Cli, command: &Command) -> Result<(), Failure> {
    let _ = &cli.command;
    match command {
        Command::Init => init::run(&cli.opts),
        Command::Status => status::run(&cli.opts),
        Command::Validate => validate::run(&cli.opts),
        Command::Doctor { fix } => doctor::run(&cli.opts, *fix),
        Command::Prune(args) => prune::run(&cli.opts, args),
        Command::Archive {
            domain,
            id,
            records,
            reason,
            dry_run,
        } => archive::run(
            &cli.opts,
            domain,
            id.as_deref(),
            records.as_deref(),
            reason,
            *dry_run,
        ),
        Command::Restore { id } => restore::run(&cli.opts, id),
        Command::Add { domain } => add::run(&cli.opts, domain.clone()),
        Command::Record(args) => record::run(&cli.opts, args),
        Command::Edit(args) => edit::run(&cli.opts, args),
        Command::Outcome {
            domain,
            id,
            outcome,
        } => outcome::run(&cli.opts, domain, id, outcome),
        Command::Delete {
            domain,
            id,
            records,
            all_except,
            dry_run,
        } => delete::run(
            &cli.opts,
            id.as_deref(),
            records.as_deref(),
            all_except.as_deref(),
            domain,
            *dry_run,
        ),
        Command::DeleteDomain {
            domain,
            yes,
            dry_run,
        } => delete_domain::run(&cli.opts, domain, *yes, *dry_run),
        Command::MoveRecord {
            source_domain,
            id,
            target_domain,
            dry_run,
            force,
        } => move_cmd::run(
            &cli.opts,
            source_domain,
            id,
            target_domain,
            *dry_run,
            *force,
        ),
        Command::Query(args) => query::run(&cli.opts, args),
        Command::Search(args) => search::run(&cli.opts, args),
        Command::Rank {
            domain,
            record_type,
            limit,
            min_score,
        } => rank::run(
            &cli.opts,
            domain.as_deref(),
            record_type.as_deref(),
            limit,
            min_score,
        ),
        Command::Ready {
            limit,
            domain,
            since,
        } => ready::run(
            &cli.opts,
            limit.as_str(),
            domain.as_deref(),
            since.as_deref(),
        ),
        Command::Learn { since } => learn::run(&cli.opts, since),
        Command::Prime(args) => prime::run(&cli.opts, args),
    }
}

/// The current instant as reference-format `recorded_at`
/// (ISO-8601, millisecond precision, `Z`).
pub(crate) fn now_iso() -> String {
    jiff::Timestamp::now()
        .strftime("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

/// Which unknown-domain surface a command renders — the reference
/// hand-writes a shape per command; this carries them as data.
#[derive(Clone, Copy)]
pub(crate) enum DomainFailure {
    /// Two-line plain (lowercase `domain`), one-line json (capital D).
    Standard,
    /// `move`'s single-line plain keeps the capital D.
    Move,
    /// `delete-domain`'s plain carries a Hint instead of the list.
    WithHint,
    /// query's read-family hint (query.ts:70-98): the message is the
    /// two-line hint in BOTH modes — the json surface rides the
    /// error-field envelope — and an empty store backfills `(none)`.
    QueryHint,
    /// search's hint (search.ts:153-170): the same envelope-carrying
    /// shape, but the list joins WITHOUT the `(none)` fallback (an
    /// empty store renders the trailing `Available domains: `).
    SearchHint,
    /// rank's hint (rank.ts): the json switch rides the shared handled
    /// law — json prints the bare sentence, plain the hint; the list
    /// backfills `(none)` like query's.
    RankHint,
    /// ready's rewording (ready.ts): json keeps the standard sentence,
    /// plain is the single lowercase line `domain "x" not found.
    /// Available: …` (no `in config`, no `domains`, no hint), the
    /// list without the `(none)` fallback.
    ReadyOneLine,
    /// prime's shape (prime.ts): the error-field envelope with the
    /// one-line sentence; `excluded` prefixes the subject (`Excluded
    /// domain`, the --exclude-domain walk).
    Prime {
        /// Whether the name came from the exclusion walk.
        excluded: bool,
    },
}

/// The one unknown-domain failure: the available-list join, every
/// message shape, and the two envelope laws (the shared handled
/// derivation vs the error-field envelope) live here, not in
/// per-command copies (mulch-9e70; the read family joined in
/// mulch-5366).
pub(crate) fn unknown_domain_failure(
    command: &str,
    domain: &str,
    available: &[String],
    json: bool,
    shape: DomainFailure,
) -> Failure {
    // search and ready join the list without the `(none)` backfill
    // every other shape takes (probe-pinned per-command drift).
    let bare_list = matches!(
        shape,
        DomainFailure::SearchHint | DomainFailure::ReadyOneLine
    );
    let list = if available.is_empty() && !bare_list {
        "(none)".to_string()
    } else {
        available.join(", ")
    };
    // prime's exclusion walk prefixes the subject; every other
    // shape reads `Domain`.
    let subject = match shape {
        DomainFailure::Prime { excluded: true } => "Excluded domain",
        _ => "Domain",
    };
    // The one-line capital-D sentence: json output for every
    // shape, and `move`'s plain text prefixed with `Error: `.
    let one_line = format!("{subject} \"{domain}\" not found in config. Available domains: {list}");
    // The read family's shared hint line (query, search, rank,
    // prime): it promotes THIS binary and names the config file.
    let hint = format!(
        "Hint: Run `mulch add {domain}` to create this domain, or check .mulch/mulch.config.yaml"
    );
    // The shape's plain wording (json output is the bare sentence).
    let plain = match shape {
        DomainFailure::Standard => {
            format!("Error: domain \"{domain}\" not found in config.\nAvailable domains: {list}")
        }
        DomainFailure::Move => format!("Error: {one_line}"),
        DomainFailure::WithHint => format!(
            "Error: domain \"{domain}\" not found in config.\nHint: Run `mulch add {domain}` to create it, or check `mulch status` for existing domains."
        ),
        // The read family's hint shapes share the config-file hint.
        DomainFailure::QueryHint
        | DomainFailure::SearchHint
        | DomainFailure::RankHint
        | DomainFailure::Prime { .. } => format!("Error: {one_line}\n{hint}"),
        DomainFailure::ReadyOneLine => {
            format!("Error: domain \"{domain}\" not found. Available: {list}")
        }
    };
    // The message: query and search keep the hint in BOTH modes
    // (their json surface rides the error-field envelope below, not
    // the message); prime keeps the `Error: `-prefixed sentence for
    // json (the envelope overrides the printed surface anyway); the
    // handled shapes print the bare sentence as their json output.
    let message = match shape {
        DomainFailure::QueryHint | DomainFailure::SearchHint => plain,
        DomainFailure::Prime { .. } if json => format!("Error: {one_line}"),
        _ if json => one_line.clone(),
        _ => plain,
    };
    // The error-field envelope family (query, search, prime): the
    // json surface is a failure envelope whose `error` field is the
    // sentence alone — the handled derivation would drag the plain
    // wording along.
    match shape {
        DomainFailure::QueryHint | DomainFailure::SearchHint | DomainFailure::Prime { .. } => {
            let mut failure = Failure::handled_on_stderr(command, message);
            let mut fields = serde_json::Map::new();
            fields.insert("error".into(), serde_json::Value::String(one_line));
            failure.envelope = failure_envelope(command, fields);
            failure
        }
        _ => Failure::handled_on_stderr(command, message),
    }
}

/// The shared identifier-resolution failure: the reference's
/// not-found and ambiguous texts (identical across the record
/// commands; `command` selects the envelope's command field).
pub(crate) fn resolve_failure(command: &str, error: ResolveError) -> Failure {
    match error {
        ResolveError::NotFound(identifier) => Failure::handled_on_stderr(
            command,
            format!(
                "Error: Record \"{identifier}\" not found. Run `mulch query` to see record IDs."
            ),
        ),
        ResolveError::Ambiguous {
            identifier,
            count,
            ids,
        } => Failure::handled_on_stderr(
            command,
            format!(
                "Error: Ambiguous identifier \"{identifier}\" matches {count} records: {}. Use more characters to disambiguate.",
                ids.join(", ")
            ),
        ),
    }
}

/// The strict domain read with its failure mapped onto the command's
/// handled shape (mulch-a8f4): the pair ten call sites repeated beside
/// [`read_and_resolve`]. `render_core_error` owns the byte law
/// (malformed-line + unknown-type texts).
pub(crate) fn read_domain_records(
    command: &str,
    store: &mulch::StoreFiles,
    domain: &str,
    allow_unknown: bool,
) -> Result<Vec<mulch::LineRecord>, Failure> {
    store
        .read_records(domain, allow_unknown)
        .map_err(|source| Failure::handled_on_stderr(command, render_core_error(&source)))
}

/// The read half of the record-locator seam: strict-reads the domain
/// and resolves the identifier (exact id, bare hash, or unique prefix).
/// `move` and `delete` reach this half directly — their prologues pin
/// a different order around it (`move` checks source==target and both
/// domains first, `delete` checks the domain before its mode
/// validation, mulch-e645).
pub(crate) fn read_and_resolve(
    command: &str,
    store: &mulch::StoreFiles,
    domain: &str,
    id: &str,
    allow_unknown: bool,
) -> Result<(Vec<mulch::LineRecord>, usize), Failure> {
    let records = read_domain_records(command, store, domain, allow_unknown)?;
    let position =
        mulch::resolve_record_id(&records, id).map_err(|error| resolve_failure(command, error))?;
    Ok((records, position))
}

/// The full-record verdict of an UPDATED record for the write gates
/// (`edit`, `move`): [`mulch::schema::full_verdict`] over the
/// reference's in-memory shape. When the target's raw line carries a
/// singular `outcome` key, the strict reader normalizes it away — but
/// the reference's in-memory copy keeps the key (an undefined-valued
/// property after its normalization, the raw value otherwise), and
/// its ajv gate reports the record as having an additional property.
/// `line` is the target's physical (1-based) line number.
pub(crate) fn updated_record_verdict(
    store: &mulch::StoreFiles,
    domain: &str,
    line: usize,
    updated: &serde_json::Value,
) -> mulch::schema::FullVerdict {
    let raw_had_outcome = store.read_lenient(domain).into_iter().any(|finding| {
        matches!(&finding,
            mulch::LenientLine::Record { line: at, record }
                if *at == line
                    && record
                        .as_object()
                        .is_some_and(|object| object.contains_key("outcome")))
    });
    if !raw_had_outcome {
        return mulch::schema::full_verdict(store.registry(), updated);
    }
    // Key presence alone decides the additional-property report; the
    // reference's undefined value is unrepresentable in JSON, and
    // null behaves identically there.
    let mut augmented = updated.clone();
    if let Some(object) = augmented.as_object_mut() {
        object.insert("outcome".into(), serde_json::Value::Null);
    }
    mulch::schema::full_verdict(store.registry(), &augmented)
}

/// The single-id prologue every per-record command opens with: open
/// the store, check the domain, strict-read the file, resolve the id
/// (mulch-0fd8 — one seam beside [`open_store`],
/// [`unknown_domain_failure`] and [`resolve_failure`]). `shape` and
/// `config_message` select the unknown-domain and no-store wordings;
/// the future per-record commands (query family) start here.
pub(crate) fn locate_record(
    opts: &crate::cli::GlobalOpts,
    command: &'static str,
    domain: &str,
    id: &str,
    shape: DomainFailure,
    config_message: bool,
    exclusive: bool,
) -> Result<(mulch::StoreFiles, Vec<mulch::LineRecord>, usize), Failure> {
    // Mutating callers (edit, outcome) hold the writer lock across
    // the whole cycle; the read family (query, mulch-16da) passes
    // false (mulch-228d).
    let store = if exclusive {
        open_store_mut(command, config_message)?
    } else {
        open_store(command, config_message)?
    };
    let domains = store.domains();
    if !domains.iter().any(|d| d == domain) {
        return Err(unknown_domain_failure(
            command, domain, &domains, opts.json, shape,
        ));
    }
    let (records, position) =
        read_and_resolve(command, &store, domain, id, opts.allow_unknown_types)?;
    Ok((store, records, position))
}

/// The write-path law of the mutating commands (mulch-b6d5): rewrite
/// the domain file with the new record set, mapping a store failure
/// onto the command's handled shape (chain-rendered). The projection
/// to the record set stays at the call site — it differs per command
/// (plain projection, index replace, survivor filter, batch working
/// copy).
pub(crate) fn persist(
    command: &'static str,
    store: &mulch::StoreFiles,
    domain: &str,
    records: &[serde_json::Value],
) -> Result<(), Failure> {
    store
        .rewrite_domain(domain, records)
        .map_err(|source| Failure::handled(command, chain_message_from(&source)))
}

/// The strict flag-value failure (utils/numeric-flags.ts wording,
/// mulch-a878): `Error: --<flag> must be a <what> (got "<raw>").` on
/// stderr; the JSON envelope drops the `Error: ` prefix (the shared
/// handled law). One owner beside [`persist`] — rank, ready, and
/// prime carried private copies.
pub(crate) fn flag_value_failure(
    command: &'static str,
    flag: &str,
    what: &str,
    raw: &str,
) -> Failure {
    Failure::handled_on_stderr(
        command,
        format!("Error: --{flag} must be a {what} (got \"{raw}\")."),
    )
}

/// The payload<->flag adapter (mulch-4ee2): the one owner of the six
/// registry payload fields' CLI flag sources. `record` and `edit`
/// read payload fields through [`PayloadFlags::get`], so the
/// field->flag mapping lives in one table instead of one hand-copy
/// per command.
///
/// Two views, two laws (both reference-pinned):
/// [`get`](PayloadFlags::get) is the raw flag view — edit stores
/// whatever was flagged, `Some("")` included (`value === undefined`
/// is edit's only gate). [`resolved`](PayloadFlags::resolved) is
/// record's `collectField` value: the flag when given (even empty),
/// else the positional `[content]` feeding the type's fallback field
/// (convention→content, pattern/reference/guide→description,
/// failure/decision→dropped — `positionalFallbackField`,
/// mulch-e2cf); record then treats empty strings as missing
/// (`value === undefined || value === ""`, mulch-1209). The retry
/// echo (`buildRetryCommand`) reads flag values alone — the
/// positional renders as a bare quoted argument, never as a pair.
pub(crate) struct PayloadFlags<'a> {
    /// (payload field, provided flag value) rows — raw flag values,
    /// `Some("")` included. `files` rides along: not a payload field,
    /// but a declared flag the row loops read when the type declares
    /// it (the reference's option key space, mulch-d524).
    table:      [(&'static str, Option<&'a str>); 7],
    /// Custom-type field flags (field name with underscores, the
    /// parsed `--<field>` value) — the registry-fed extension of the
    /// builtin table, one flat map across customs like the
    /// reference's option key space (mulch-d524).
    custom:     &'a [(String, String)],
    /// `record`'s positional `[content]` argument.
    positional: Option<&'a str>,
    /// The type's positional fallback field (record path only).
    fallback:   Option<&'static str>,
}

impl<'a> PayloadFlags<'a> {
    /// `record`'s rows: raw flag values; the positional fallback
    /// resolution happens in [`resolved`](PayloadFlags::resolved).
    pub(crate) fn from_record(args: &'a RecordArgs, record_type: &str) -> Self {
        Self::new(
            [
                ("content", args.content_flag.as_deref()),
                ("name", args.name.as_deref()),
                ("description", args.description.as_deref()),
                ("resolution", args.resolution.as_deref()),
                ("title", args.title.as_deref()),
                ("rationale", args.rationale.as_deref()),
                ("files", args.files.as_deref()),
            ],
            &args.custom,
            args.content.as_deref(),
            // The positional drops for the two flag-only builtins and
            // for EVERY custom (positionalFallbackField: builtin-only,
            // null for `kind !== "builtin"` — probed, mulch-d524). A
            // NEW REGISTRY BUILTIN lands in the named arms below; the
            // registry-shape test in ids.rs forces that revisit.
            match record_type {
                "convention" => Some("content"),
                "pattern" | "reference" | "guide" => Some("description"),
                _ => None,
            },
        )
    }

    /// `edit`'s rows: content is the `--content` flag alone.
    pub(crate) fn from_edit(args: &'a EditArgs) -> Self {
        Self::new(
            [
                ("content", args.content.as_deref()),
                ("name", args.name.as_deref()),
                ("description", args.description.as_deref()),
                ("resolution", args.resolution.as_deref()),
                ("title", args.title.as_deref()),
                ("rationale", args.rationale.as_deref()),
                ("files", args.files.as_deref()),
            ],
            &args.custom,
            None,
            None,
        )
    }

    /// The shared row constructor (the two commands differ in the
    /// positional pair alone).
    fn new(
        table: [(&'static str, Option<&'a str>); 7],
        custom: &'a [(String, String)],
        positional: Option<&'a str>,
        fallback: Option<&'static str>,
    ) -> Self {
        Self {
            table,
            custom,
            positional,
            fallback,
        }
    }

    /// The provided flag value for a payload field, if any — the raw
    /// flag view (edit's law: store what was flagged). Custom-type
    /// fields read the dynamic flag table (a field overlapping a
    /// builtin flag name reads the builtin's row — the reference's
    /// declaredOptionNames skip).
    pub(crate) fn get(&self, field: &str) -> Option<&'a str> {
        self.table
            .iter()
            .find(|(name, _)| *name == field)
            .and_then(|(_, value)| *value)
            .or_else(|| {
                self.custom
                    .iter()
                    .find(|(name, _)| name == field)
                    .map(|(_, value)| value.as_str())
            })
    }

    /// The record-path `collectField` value: the flag when given,
    /// else the positional feeding the type's fallback field. Empty
    /// strings survive here — record's missing/drop check filters
    /// them (mulch-1209).
    pub(crate) fn resolved(&self, field: &str) -> Option<&'a str> {
        match self.fallback {
            Some(fallback) if fallback == field => self.get(field).or(self.positional),
            _ => self.get(field),
        }
    }

    /// The retry-echo pair `--{field} "<value>"` of a provided,
    /// non-empty flag (buildRetryCommand echoes flag values alone,
    /// JSON.stringify-quoted).
    pub(crate) fn retry_pair(&self, field: &str) -> Option<String> {
        truthy_str(self.get(field)).map(|value| format!("--{field} {}", js_string(value)))
    }

    /// The table's field universe (the pin surface in
    /// [`payload_flags_tests`]).
    #[cfg(test)]
    pub(crate) fn fields(&self) -> impl Iterator<Item = &'static str> {
        self.table.iter().map(|(name, _)| *name)
    }
}

/// Reads a one-line answer from stdin (the delete-domain prompt).
pub(crate) fn read_confirmation() -> std::io::Result<String> {
    use std::io::BufRead as _;
    let mut buffer = String::new();
    std::io::stdin().lock().read_line(&mut buffer)?;
    Ok(buffer)
}

/// Renders a format-core read error the way the reference does (the
/// malformed-line and unknown-type contracts are byte-pinned).
pub(crate) fn render_core_error(error: &Error) -> String {
    match error {
        Error::MalformedLine {
            path,
            line,
            preview,
            reason,
        } => format!(
            "Error: Malformed JSONL at {}:{line}: {reason}. Line: {preview}",
            path.display()
        ),
        Error::UnknownRecordType {
            path,
            line,
            id,
            record_type,
        } => {
            let id_part = id
                .as_ref()
                .map_or_else(String::new, |id| format!(" (id={id})"));
            format!(
                "Error: Unknown record type \"{record_type}\" at {}:{line}{id_part}. Register it under custom_types in mulch.config.yaml, remove the record, or pass --allow-unknown-types to bypass.",
                path.display()
            )
        }
        other => format!("Error: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_render_the_reference_surfaces() {
        let available = vec!["d".to_string()];
        let standard =
            unknown_domain_failure("delete", "x", &available, false, DomainFailure::Standard);
        assert_eq!(
            standard.message,
            "Error: domain \"x\" not found in config.\nAvailable domains: d"
        );
        let json = unknown_domain_failure("delete", "x", &available, true, DomainFailure::Standard);
        assert_eq!(
            json.message,
            "Domain \"x\" not found in config. Available domains: d"
        );
        let moved = unknown_domain_failure("move", "x", &available, false, DomainFailure::Move);
        assert_eq!(
            moved.message,
            "Error: Domain \"x\" not found in config. Available domains: d"
        );
        let hint = unknown_domain_failure(
            "delete-domain",
            "x",
            &available,
            false,
            DomainFailure::WithHint,
        );
        assert_eq!(
            hint.message,
            "Error: domain \"x\" not found in config.\nHint: Run `mulch add x` to create it, or check `mulch status` for existing domains."
        );
        let none = unknown_domain_failure("delete", "x", &[], false, DomainFailure::Standard);
        assert!(none.message.ends_with("Available domains: (none)"));
        let query =
            unknown_domain_failure("query", "x", &available, false, DomainFailure::QueryHint);
        assert_eq!(
            query.message,
            "Error: Domain \"x\" not found in config. Available domains: d\n\
             Hint: Run `mulch add x` to create this domain, or check .mulch/mulch.config.yaml"
        );
        assert_eq!(
            query.envelope["error"],
            "Domain \"x\" not found in config. Available domains: d"
        );
        assert!(query.envelope_to_stderr);
        // search joins the list without the `(none)` backfill.
        let search = unknown_domain_failure("search", "x", &[], false, DomainFailure::SearchHint);
        assert_eq!(
            search.message,
            "Error: Domain \"x\" not found in config. Available domains: \n\
             Hint: Run `mulch add x` to create this domain, or check .mulch/mulch.config.yaml"
        );
        let rank = unknown_domain_failure("rank", "x", &available, false, DomainFailure::RankHint);
        assert_eq!(
            rank.message,
            "Error: Domain \"x\" not found in config. Available domains: d\n\
             Hint: Run `mulch add x` to create this domain, or check .mulch/mulch.config.yaml"
        );
        let rank_json =
            unknown_domain_failure("rank", "x", &available, true, DomainFailure::RankHint);
        assert_eq!(
            rank_json.message,
            "Domain \"x\" not found in config. Available domains: d"
        );
        let ready =
            unknown_domain_failure("ready", "x", &available, false, DomainFailure::ReadyOneLine);
        assert_eq!(ready.message, "Error: domain \"x\" not found. Available: d");
        let ready_empty =
            unknown_domain_failure("ready", "x", &[], false, DomainFailure::ReadyOneLine);
        assert!(ready_empty.message.ends_with("Available: "));
        let prime = unknown_domain_failure("prime", "x", &available, false, DomainFailure::Prime {
            excluded: false,
        });
        assert_eq!(
            prime.message,
            "Error: Domain \"x\" not found in config. Available domains: d\n\
             Hint: Run `mulch add x` to create this domain, or check .mulch/mulch.config.yaml"
        );
        let excluded =
            unknown_domain_failure("prime", "x", &available, false, DomainFailure::Prime {
                excluded: true,
            });
        assert_eq!(
            excluded.message,
            "Error: Excluded domain \"x\" not found in config. Available domains: d\n\
             Hint: Run `mulch add x` to create this domain, or check .mulch/mulch.config.yaml"
        );
        assert_eq!(
            excluded.envelope["error"],
            "Excluded domain \"x\" not found in config. Available domains: d"
        );
    }
}

#[cfg(test)]
mod payload_flags_tests {
    use super::*;

    fn record_args(args: &[&str]) -> RecordArgs {
        use clap::Parser as _;
        let cli = crate::cli::Cli::parse_from(std::iter::once("mulch").chain(args.iter().copied()));
        let Some(crate::cli::Command::Record(args)) = cli.command else {
            panic!("test args must parse as a record invocation");
        };
        *args
    }

    fn edit_args(args: &[&str]) -> EditArgs {
        use clap::Parser as _;
        let cli = crate::cli::Cli::parse_from(std::iter::once("mulch").chain(args.iter().copied()));
        let Some(crate::cli::Command::Edit(args)) = cli.command else {
            panic!("test args must parse as an edit invocation");
        };
        *args
    }

    fn record_args_all_set() -> RecordArgs {
        record_args(&[
            "record",
            "d",
            "--content",
            "content",
            "--name",
            "name",
            "--description",
            "description",
            "--resolution",
            "resolution",
            "--title",
            "title",
            "--rationale",
            "rationale",
        ])
    }

    /// The adapter is the one flag-table owner: its universe is the
    /// registry's payload universe plus `files` (the declared flag
    /// the extracts-files row loop reads; the merged mulch-a3de pin —
    /// a registry row gaining a field without adapter plumbing fails
    /// here, and so does a table row the registry dropped).
    #[test]
    fn table_universe_is_the_registry_payload_universe() {
        let mut universe: Vec<&str> = mulch::REGISTRY
            .iter()
            .flat_map(|spec| spec.payload.iter().copied())
            .chain(["files"])
            .collect();
        universe.sort_unstable();
        universe.dedup();
        let args = record_args_all_set();
        let flags = PayloadFlags::from_record(&args, "pattern");
        let mut table: Vec<&str> = flags.fields().collect();
        table.sort_unstable();
        table.dedup();
        assert_eq!(
            table, universe,
            "a registry payload field has no flag plumbing"
        );
    }

    /// Every registry payload field resolves to its own flag value
    /// through both constructors (the record and edit rows stay the
    /// same table; unknown fields read as absent).
    #[test]
    fn registry_fields_reach_their_flag_values() {
        let record_args = record_args_all_set();
        let record_flags = PayloadFlags::from_record(&record_args, "pattern");
        let edit_args = edit_args(&[
            "edit",
            "d",
            "mx-x",
            "--content",
            "content",
            "--name",
            "name",
            "--description",
            "description",
            "--resolution",
            "resolution",
            "--title",
            "title",
            "--rationale",
            "rationale",
        ]);
        let edit_flags = PayloadFlags::from_edit(&edit_args);
        for spec in &mulch::REGISTRY {
            for field in spec.payload {
                assert_eq!(record_flags.get(field), Some(*field));
                assert_eq!(edit_flags.get(field), Some(*field));
            }
        }
        assert_eq!(record_flags.get("date"), None);
    }

    /// `record`'s content law: `--content` wins over the positional
    /// shorthand — in the RESOLVED view (collectField reads the flag
    /// first, even when empty; the raw view never sees the
    /// positional). An empty flag value survives resolution and
    /// becomes missing at record's non-empty gate (mulch-1209).
    #[test]
    fn record_content_flag_wins_over_the_positional() {
        let both = record_args(&["record", "d", "positional", "--content", "flag"]);
        let flags = PayloadFlags::from_record(&both, "convention");
        assert_eq!(flags.get("content"), Some("flag"));
        assert_eq!(flags.resolved("content"), Some("flag"));

        let positional = record_args(&["record", "d", "positional"]);
        let flags = PayloadFlags::from_record(&positional, "convention");
        assert_eq!(flags.get("content"), None);
        assert_eq!(flags.resolved("content"), Some("positional"));

        let empty_flag = record_args(&["record", "d", "positional", "--content", ""]);
        let flags = PayloadFlags::from_record(&empty_flag, "convention");
        assert_eq!(flags.get("content"), Some(""));
        assert_eq!(flags.resolved("content"), Some(""));
    }

    /// The positional fallback is type-specific (reference
    /// `positionalFallbackField`, mulch-e2cf): convention keys on
    /// content, pattern/reference/guide on description, and
    /// failure/decision drop the positional entirely.
    #[test]
    fn positional_fallback_follows_the_record_type() {
        let positional = record_args(&["record", "d", "content-x"]);
        for record_type in ["convention", "pattern", "reference", "guide"] {
            let flags = PayloadFlags::from_record(&positional, record_type);
            let fallback = if record_type == "convention" {
                "content"
            } else {
                "description"
            };
            assert_eq!(flags.resolved(fallback), Some("content-x"));
        }
        for record_type in ["failure", "decision"] {
            let flags = PayloadFlags::from_record(&positional, record_type);
            assert_eq!(flags.resolved("content"), None);
            assert_eq!(flags.resolved("description"), None);
        }
    }

    /// The retry-echo pair shape (the missing-flags failure's
    /// `Retry:` line): `--{field} "<value>"` JSON.stringify-quoted,
    /// absent without a value or with an empty one (buildRetryCommand
    /// reads flag values alone, JS truthiness).
    #[test]
    fn retry_pairs_render_the_reference_echo_shape() {
        let args = record_args(&["record", "d", "--name", "n v"]);
        let flags = PayloadFlags::from_record(&args, "pattern");
        assert_eq!(flags.retry_pair("name"), Some("--name \"n v\"".to_string()));
        assert_eq!(flags.retry_pair("description"), None);

        let quoted = record_args(&["record", "d", "--name", "a\"b"]);
        let flags = PayloadFlags::from_record(&quoted, "pattern");
        assert_eq!(
            flags.retry_pair("name"),
            Some("--name \"a\\\"b\"".to_string())
        );

        let empty = record_args(&["record", "d", "--name", ""]);
        let flags = PayloadFlags::from_record(&empty, "pattern");
        assert_eq!(flags.retry_pair("name"), None);
    }

    /// `edit`'s law is the raw flag view: an empty value is a
    /// provided value and stores (edit.ts `value === undefined` is
    /// its only gate) — and the positional fallback never applies.
    #[test]
    fn edit_reads_the_raw_flag_view_and_stores_empty_values() {
        let args = edit_args(&["edit", "d", "mx-x", "--name", ""]);
        let flags = PayloadFlags::from_edit(&args);
        assert_eq!(flags.get("name"), Some(""));
        assert_eq!(flags.resolved("name"), Some(""));
    }
}
