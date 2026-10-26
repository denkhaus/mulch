//! Command-line surface: global flags and the implemented subcommands.
//!
//! Parity slice `mulch-da8b`: `init`, `status`, `validate`, `doctor`.
//! Commands join the tree only when their parity slice lands
//! (`mulch-0e39` help-honesty rule).

use clap::{Args, Parser, Subcommand};

/// Reference-compatible global options.
#[derive(Debug, Args)]
pub(crate) struct GlobalOpts {
    /// Output as structured JSON.
    #[arg(long, global = true)]
    pub json: bool,

    /// Suppress non-error output.
    #[arg(short = 'q', long, global = true)]
    pub quiet: bool,

    /// Show full details in output.
    #[arg(long, global = true)]
    pub verbose: bool,

    /// Print execution time to stderr.
    #[arg(long, global = true)]
    pub timing: bool,

    /// Tolerate on-disk records of unregistered types.
    #[arg(long, global = true)]
    pub allow_unknown_types: bool,

    /// Tolerate records violating per-domain type/field rules.
    #[arg(long, global = true)]
    pub allow_domain_mismatch: bool,

    /// Output format for record-rendering commands. The value set is
    /// the reference commander's GLOBAL choice list (cli.ts:130);
    /// the `ids` form query/search help advertises is not parseable
    /// through this flag (`--format ids` rejects, probed 2026-10-23).
    #[arg(
        long,
        global = true,
        value_name = "format",
        value_parser = ["markdown", "compact", "xml", "plain"]
    )]
    pub format: Option<String>,
}

/// mulch — structured expertise management (native implementation).
#[derive(Debug, Parser)]
#[command(
    name = "mulch",
    version,
    disable_help_subcommand = true,
    disable_version_flag = true
)]
pub(crate) struct Cli {
    /// Print version (reference short form `-v`; top-level only — the
    /// reference's version flag is not subcommand-global).
    #[arg(short = 'v', long)]
    pub(crate) version: bool,

    #[command(flatten)]
    pub opts: GlobalOpts,

    #[command(subcommand)]
    pub command: Option<Command>,
}

/// The implemented command surface (help honesty: nothing else listed).
#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Initialize .mulch/ in the current project.
    Init,

    /// Add a new expertise domain.
    Add {
        /// Domain name (`[a-zA-Z0-9][a-zA-Z0-9_-]*`).
        domain: String,
    },

    /// Record an expertise record.
    Record(Box<RecordArgs>),

    /// Edit an existing expertise record.
    Edit(Box<EditArgs>),

    /// Delete an expertise record (or several).
    Delete {
        /// Domain to delete from.
        domain: String,

        /// Record id (`mx-…`); omit with --records/--all-except.
        id: Option<String>,

        /// Comma-separated record IDs to delete.
        #[arg(long)]
        records: Option<String>,

        /// Delete all records except these comma-separated IDs.
        #[arg(long = "all-except")]
        all_except: Option<String>,

        /// Preview without making changes.
        #[arg(long = "dry-run")]
        dry_run: bool,
    },

    /// Delete an expertise domain and its expertise file.
    #[command(name = "delete-domain")]
    DeleteDomain {
        /// Domain to remove.
        domain: String,

        /// Skip the confirmation prompt.
        #[arg(long)]
        yes: bool,

        /// Preview without making changes.
        #[arg(long = "dry-run")]
        dry_run: bool,
    },

    /// Move a record from one domain to another (appends at the end).
    #[command(name = "move")]
    MoveRecord {
        /// Source domain.
        source_domain: String,

        /// Record id (`mx-…`).
        id: String,

        /// Target domain.
        target_domain: String,

        /// Preview the move without making changes.
        #[arg(long = "dry-run")]
        dry_run: bool,

        /// Bypass the target domain's allowed_types gate.
        #[arg(long)]
        force: bool,
    },

    /// Append an outcome to an existing record.
    Outcome {
        /// Domain of the record.
        domain: String,

        /// Record id (`mx-…`).
        id: String,

        #[command(flatten)]
        outcome: OutcomeFlags,
    },

    /// Show status of expertise records.
    Status,

    /// Validate expertise records against schemas.
    Validate,

    /// Run health checks on expertise records.
    Doctor {
        /// Apply repairs where the reference supports it.
        #[arg(long)]
        fix: bool,
    },

    /// Soft-archive or hard-delete stale records, plus tier-demote
    /// superseded ones (mulch-7c55).
    Prune(PruneArgs),

    /// Soft-archive one or more live records (mulch-7c55).
    Archive {
        /// Expertise domain.
        domain: String,

        /// Record ID (exact, bare hash, or unique prefix).
        id: Option<String>,

        /// Comma-separated list of record IDs to archive.
        #[arg(long)]
        records: Option<String>,

        /// Why this record is being archived (required, free-text).
        #[arg(long)]
        reason: String,

        /// Preview without making changes.
        #[arg(long = "dry-run")]
        dry_run: bool,
    },

    /// Revive an archived record (mulch-7c55).
    Restore {
        /// Archived record ID (exact, bare hash, or unique prefix).
        id: String,
    },

    /// Compact records: analyze candidates or apply a compaction
    /// (mulch-8a1b).
    Compact(CompactArgs),

    /// Query records across domains (mulch-16da).
    Query(Box<QueryArgs>),

    /// Search expertise records across domains (mulch-16da).
    Search(Box<SearchArgs>),

    /// Rank records by confirmation-frequency score (highest first)
    /// (mulch-16da).
    Rank {
        /// Limit ranking to a specific domain.
        domain: Option<String>,

        /// Record type (the choice set is the store's registry —
        /// filled in by [`augment`], rank.ts).
        #[arg(long = "type")]
        record_type: Option<String>,

        /// Return at most N records.
        // allow_hyphen_values: commander passes `-1` to the command's
        // own parse (probed); clap would abort pre-dispatch.
        #[arg(long, default_value = "10", allow_hyphen_values = true)]
        limit: String,

        /// Exclude records whose confirmation score is below N.
        #[arg(long = "min-score", default_value = "0", allow_hyphen_values = true)]
        min_score: String,
    },

    /// Show recently added or updated expertise records (mulch-16da).
    Ready {
        /// Maximum number of records to show.
        // allow_hyphen_values: commander hands `-1` to the command's
        // own parse (probed); clap would abort pre-dispatch.
        #[arg(long, default_value = "10", allow_hyphen_values = true)]
        limit: String,

        /// Limit to a specific domain.
        #[arg(long, allow_hyphen_values = true)]
        domain: Option<String>,

        /// Show records from the last duration (e.g. 24h, 7d, 2w).
        #[arg(long, allow_hyphen_values = true)]
        since: Option<String>,
    },

    /// Show changed files and suggest domains for recording
    /// learnings (mulch-16da).
    Learn {
        /// Git ref to diff against.
        // allow_hyphen_values: commander passes `-ref` verbatim
        // (probed); clap would abort pre-dispatch.
        #[arg(long, default_value = "HEAD~1", allow_hyphen_values = true)]
        since: String,
    },

    /// Prime a session with relevant expertise (mulch-16da).
    Prime(Box<PrimeArgs>),

    /// Validate, stage, and commit .mulch/ changes (mulch-176f).
    Sync {
        /// Custom commit message.
        #[arg(long)]
        message: Option<String>,

        /// Skip validation step.
        #[arg(long = "no-validate")]
        no_validate: bool,
    },
}

/// Flags of `mulch record` (reference surface).
#[derive(Debug, Args)]
pub(crate) struct RecordArgs {
    /// Target domain.
    pub(crate) domain: String,

    /// Positional content shorthand (convention).
    pub(crate) content: Option<String>,

    /// Record type (the choice set is the store's registry —
    /// builtins plus configured custom types, filled in by
    /// [`augment`] before parsing; mulch-d524).
    #[arg(long = "type")]
    pub(crate) record_type: Option<String>,

    /// Classification (default tactical).
    #[arg(
        long,
        value_parser = ["foundational", "tactical", "observational"]
    )]
    pub(crate) classification: Option<String>,

    #[arg(long)]
    pub(crate) name: Option<String>,

    #[arg(long = "content")]
    pub(crate) content_flag: Option<String>,

    #[arg(long)]
    pub(crate) description: Option<String>,

    #[arg(long)]
    pub(crate) resolution: Option<String>,

    #[arg(long)]
    pub(crate) title: Option<String>,

    #[arg(long)]
    pub(crate) rationale: Option<String>,

    /// Related files, comma-separated.
    #[arg(long)]
    pub(crate) files: Option<String>,

    /// Repo-relative directory anchors (repeatable).
    #[arg(long = "dir-anchor")]
    pub(crate) dir_anchors: Vec<String>,

    /// Tags, comma-separated.
    #[arg(long)]
    pub(crate) tags: Option<String>,

    #[arg(long = "evidence-commit")]
    pub(crate) evidence_commit: Option<String>,

    #[arg(long = "evidence-issue")]
    pub(crate) evidence_issue: Option<String>,

    #[arg(long = "evidence-file")]
    pub(crate) evidence_file: Option<String>,

    #[arg(long = "evidence-bead")]
    pub(crate) evidence_bead: Option<String>,

    #[arg(long = "evidence-seeds")]
    pub(crate) evidence_seeds: Option<String>,

    #[arg(long = "evidence-gh")]
    pub(crate) evidence_gh: Option<String>,

    #[arg(long = "evidence-linear")]
    pub(crate) evidence_linear: Option<String>,

    /// Related record ids, comma-separated.
    #[arg(long = "relates-to")]
    pub(crate) relates_to: Option<String>,

    /// Superseded record ids, comma-separated.
    #[arg(long)]
    pub(crate) supersedes: Option<String>,

    #[arg(long = "outcome-status", value_parser = ["success", "failure", "partial"])]
    pub(crate) outcome_status: Option<String>,

    /// Outcome duration in milliseconds.
    // allow_hyphen_values: hyphen values reach the strict parse (the
    // reference error), not a clap usage error.
    #[arg(long = "outcome-duration", allow_hyphen_values = true)]
    pub(crate) outcome_duration: Option<String>,

    #[arg(long = "outcome-test-results")]
    pub(crate) outcome_test_results: Option<String>,

    #[arg(long = "outcome-agent")]
    pub(crate) outcome_agent: Option<String>,

    /// Append even when a duplicate id exists.
    #[arg(long)]
    pub(crate) force: bool,

    /// Read record JSON from stdin.
    #[arg(long)]
    pub(crate) stdin: bool,

    /// Read a JSON array of records from a file. A `String` arg (not
    /// `PathBuf`): clap rejects EMPTY values for path args but the
    /// reference's `if (options.batch)` truthiness treats `--batch ""`
    /// as no batch at all — the flag path runs (mulch-1209).
    #[arg(long)]
    pub(crate) batch: Option<String>,

    /// Preview without writing.
    #[arg(long = "dry-run")]
    pub(crate) dry_run: bool,

    /// Custom-type field flags (`--<field>` per the store's registry,
    /// registered dynamically by [`augment`] and filled by
    /// [`inject_dynamic`] — the reference declares them on the
    /// command at startup, record.ts:510-524; mulch-d524). Field
    /// names keep their underscores; the flag name hyphenates.
    #[arg(skip)]
    pub(crate) custom: Vec<(String, String)>,
}

/// Flags of `mulch edit` (reference surface: no tags/evidence/dir-anchor).
#[derive(Debug, Args)]
pub(crate) struct EditArgs {
    /// Domain of the record.
    pub(crate) domain: String,

    /// Record id (`mx-…`).
    pub(crate) id: String,

    #[arg(
        long,
        value_parser = ["foundational", "tactical", "observational"]
    )]
    pub(crate) classification: Option<String>,

    #[arg(long)]
    pub(crate) content: Option<String>,

    #[arg(long)]
    pub(crate) name: Option<String>,

    #[arg(long)]
    pub(crate) description: Option<String>,

    #[arg(long)]
    pub(crate) resolution: Option<String>,

    #[arg(long)]
    pub(crate) title: Option<String>,

    #[arg(long)]
    pub(crate) rationale: Option<String>,

    /// Related files, comma-separated (appends at line end).
    #[arg(long)]
    pub(crate) files: Option<String>,

    /// Related record ids, comma-separated (appends at line end).
    #[arg(long = "relates-to")]
    pub(crate) relates_to: Option<String>,

    /// Superseded record ids, comma-separated (appends at line end).
    #[arg(long)]
    pub(crate) supersedes: Option<String>,

    #[command(flatten)]
    pub(crate) outcome: EditOutcomeFlags,

    /// Custom-type field flags — the edit twin of
    /// [`RecordArgs::custom`] (edit.ts:51-61).
    #[arg(skip)]
    pub(crate) custom: Vec<(String, String)>,
}

/// Outcome flags of `mulch edit` (reference longs carry the
/// `--outcome-` prefix here, unlike the `outcome` command).
#[derive(Debug, Args)]
pub(crate) struct EditOutcomeFlags {
    /// Outcome verdict.
    #[arg(
        long = "outcome-status",
        value_parser = ["success", "failure", "partial"]
    )]
    pub(crate) status: Option<String>,

    /// Duration in milliseconds.
    // allow_hyphen_values: hyphen values reach the strict parse (the
    // reference error), not a clap usage error.
    #[arg(long = "outcome-duration", allow_hyphen_values = true)]
    pub(crate) duration: Option<String>,

    /// Test results summary.
    #[arg(long = "outcome-test-results")]
    pub(crate) test_results: Option<String>,

    /// Recording agent name.
    #[arg(long = "outcome-agent")]
    pub(crate) agent: Option<String>,
}

/// Flags of `mulch prune` (mulch-7c55).
#[derive(Debug, Args)]
pub(crate) struct PruneArgs {
    /// Show what would be pruned without removing.
    #[arg(long = "dry-run")]
    pub(crate) dry_run: bool,

    /// Permanently delete stale records instead of archiving.
    #[arg(long)]
    pub(crate) hard: bool,

    /// Collapse superseded records straight to archived in one pass
    /// instead of one tier at a time.
    #[arg(long)]
    pub(crate) aggressive: bool,

    /// Demote records whose file/dir anchors no longer resolve.
    #[arg(long = "check-anchors")]
    pub(crate) check_anchors: bool,

    /// Print per-record reasons for each demotion.
    #[arg(long)]
    pub(crate) explain: bool,
}

/// Flags of `mulch compact` (mulch-8a1b; the reference registers the
/// numeric flags as strings and parses them strictly).
#[derive(Debug, Args)]
pub(crate) struct CompactArgs {
    /// Expertise domain (required for --apply).
    pub(crate) domain: Option<String>,

    /// Show compaction candidates.
    #[arg(long)]
    pub(crate) analyze: bool,

    /// Apply a compaction (replace records with a summary).
    #[arg(long)]
    pub(crate) apply: bool,

    /// Automatically compact all candidates.
    #[arg(long)]
    pub(crate) auto: bool,

    /// Preview what --auto would do without writing.
    #[arg(long = "dry-run")]
    pub(crate) dry_run: bool,

    /// Minimum group size for auto-compaction.
    #[arg(long = "min-group")]
    pub(crate) min_group: Option<String>,

    /// Maximum records to compact in one run.
    #[arg(long = "max-records")]
    pub(crate) max_records: Option<String>,

    /// Skip confirmation prompts.
    #[arg(long)]
    pub(crate) yes: bool,

    /// Comma-separated record IDs to compact.
    #[arg(long)]
    pub(crate) records: Option<String>,

    /// Record type for the replacement.
    #[arg(long = "type")]
    pub(crate) record_type: Option<String>,

    /// Name for the replacement.
    #[arg(long)]
    pub(crate) name: Option<String>,

    /// Title for the replacement (decision).
    #[arg(long)]
    pub(crate) title: Option<String>,

    /// Description for the replacement.
    #[arg(long)]
    pub(crate) description: Option<String>,

    /// Content for the replacement (convention).
    #[arg(long)]
    pub(crate) content: Option<String>,

    /// Resolution for the replacement (failure).
    #[arg(long)]
    pub(crate) resolution: Option<String>,

    /// Rationale for the replacement (decision).
    #[arg(long)]
    pub(crate) rationale: Option<String>,
}

/// Flags of `mulch query` (reference surface: query.ts declares NO
/// choices on `--type` — a free string, unlike search/rank).
#[derive(Debug, Args)]
pub(crate) struct QueryArgs {
    /// Expertise domain to query.
    pub(crate) domain: Option<String>,

    /// Filter by record type.
    #[arg(long = "type")]
    pub(crate) record_type: Option<String>,

    /// Filter by classification.
    #[arg(
        long,
        value_parser = ["foundational", "tactical", "observational"]
    )]
    pub(crate) classification: Option<String>,

    /// Filter by associated file path (substring match).
    #[arg(long)]
    pub(crate) file: Option<String>,

    /// Filter by outcome status.
    #[arg(long = "outcome-status", value_parser = ["success", "failure"])]
    pub(crate) outcome_status: Option<String>,

    /// Sort results by confirmation-frequency score (highest first).
    #[arg(long = "sort-by-score")]
    pub(crate) sort_by_score: bool,

    /// Show all domains.
    #[arg(long)]
    pub(crate) all: bool,
}

/// Flags of `mulch search` (reference surface, search.ts).
#[derive(Debug, Args)]
pub(crate) struct SearchArgs {
    /// Search string (case-insensitive substring match).
    pub(crate) query: Option<String>,

    /// Limit search to a specific domain.
    #[arg(long)]
    pub(crate) domain: Option<String>,

    /// Record type (the choice set is the store's registry —
    /// filled in by [`augment`], search.ts:72).
    #[arg(long = "type")]
    pub(crate) record_type: Option<String>,

    /// Filter by tag.
    #[arg(long)]
    pub(crate) tag: Option<String>,

    /// Filter by classification.
    #[arg(
        long,
        value_parser = ["foundational", "tactical", "observational"]
    )]
    pub(crate) classification: Option<String>,

    /// Filter by associated file path (substring match).
    #[arg(long)]
    pub(crate) file: Option<String>,

    /// Filter by outcome status.
    #[arg(
        long = "outcome-status",
        value_parser = ["success", "failure", "partial"]
    )]
    pub(crate) outcome_status: Option<String>,

    /// Sort results by confirmation-frequency score (highest first).
    #[arg(long = "sort-by-score")]
    pub(crate) sort_by_score: bool,

    /// Disable confirmation-frequency boost on BM25 ranking (pure
    /// BM25).
    #[arg(long = "no-boost")]
    pub(crate) no_boost: bool,

    /// Include soft-archived records from .mulch/archive/ (excluded
    /// by default).
    #[arg(long)]
    pub(crate) archived: bool,
}

/// Flags of `mulch prime` (reference surface, prime.ts: the
/// positional `[domains...]` scopes the output; --budget's default
/// lives in the command, not clap).
#[derive(Debug, Args)]
pub(crate) struct PrimeArgs {
    /// Optional domain(s) to scope output to.
    pub(crate) domains: Vec<String>,

    /// Alias for --format compact.
    #[arg(long)]
    pub(crate) compact: bool,

    /// Alias for --format markdown (full record details).
    #[arg(long)]
    pub(crate) full: bool,

    /// Emit a domain index instead of full records (for monolith
    /// projects).
    #[arg(long)]
    pub(crate) manifest: bool,

    /// Domain(s) to include.
    #[arg(long = "domain")]
    pub(crate) domain: Vec<String>,

    /// Domain(s) to exclude.
    #[arg(long = "exclude-domain")]
    pub(crate) exclude_domain: Vec<String>,

    /// Filter records to only those relevant to changed files.
    #[arg(long)]
    pub(crate) context: bool,

    /// Filter records to only those relevant to specified files.
    #[arg(long)]
    pub(crate) files: Vec<String>,

    /// Opt out of auto-context-scope and emit the full corpus (full
    /// mode only; manifest mode is unaffected).
    #[arg(long)]
    pub(crate) all: bool,

    /// Export output to a file.
    #[arg(long)]
    pub(crate) export: Option<String>,

    /// Token budget for output (default: 4000).
    #[arg(long)]
    pub(crate) budget: Option<String>,

    /// Disable token budget limit.
    #[arg(long = "no-limit")]
    pub(crate) no_limit: bool,

    /// Emit JSON summary of records that would be primed (id, type,
    /// domain, tokens) without rendering content.
    #[arg(long = "dry-run")]
    pub(crate) dry_run: bool,
}

/// Outcome flags of `record --outcome-*` and the `outcome` command.
#[derive(Debug, Args)]
pub(crate) struct OutcomeFlags {
    /// Outcome verdict.
    #[arg(long = "status", value_parser = ["success", "failure", "partial"])]
    pub(crate) status: Option<String>,

    /// Duration in milliseconds.
    // allow_hyphen_values: hyphen values reach the strict parse (the
    // reference error), not a clap usage error.
    #[arg(long, allow_hyphen_values = true)]
    pub(crate) duration: Option<String>,

    /// Recording agent name.
    #[arg(long)]
    pub(crate) agent: Option<String>,

    /// Free-text notes.
    #[arg(long)]
    pub(crate) notes: Option<String>,

    /// Test results summary.
    #[arg(long = "test-results")]
    pub(crate) test_results: Option<String>,
}

// ---- dynamic custom-type flags (mulch-d524) ----
//
// The reference declares custom-type field flags on `record`/`edit`
// at command registration (record.ts:510-524, edit.ts:51-61) and
// feeds `--type`'s choices from the registry's names — both read the
// store config BEFORE the command parses. We mirror that: augment
// the two subcommands from the store's merged registry, parse, then
// carry the dynamic values into the args structs.

/// `record`'s own flag names (the reference's `declaredOptionNames`
/// set plus `files`): a custom field with one of these names reads
/// the existing flag instead of getting a new one.
const RECORD_FLAGS: [&str; 28] = [
    "type",
    "classification",
    "name",
    "content",
    "description",
    "resolution",
    "title",
    "rationale",
    "files",
    "dir-anchor",
    "tags",
    "evidence-commit",
    "evidence-issue",
    "evidence-file",
    "evidence-bead",
    "evidence-seeds",
    "evidence-gh",
    "evidence-linear",
    "relates-to",
    "supersedes",
    "outcome-status",
    "outcome-duration",
    "outcome-test-results",
    "outcome-agent",
    "force",
    "stdin",
    "batch",
    "dry-run",
];

/// `edit`'s own flag names (same law as [`RECORD_FLAGS`]).
const EDIT_FLAGS: [&str; 14] = [
    "classification",
    "content",
    "name",
    "description",
    "resolution",
    "title",
    "rationale",
    "files",
    "relates-to",
    "supersedes",
    "outcome-status",
    "outcome-duration",
    "outcome-test-results",
    "outcome-agent",
];

/// `search`'s own flag names (same law as [`RECORD_FLAGS`]; search
/// only gates `--type`'s choices on the registry — it registers NO
/// dynamic custom-field flags).
const SEARCH_FLAGS: [&str; 9] = [
    "domain",
    "type",
    "tag",
    "classification",
    "file",
    "outcome-status",
    "sort-by-score",
    "no-boost",
    "archived",
];

/// `rank`'s own flag names (same law as [`SEARCH_FLAGS`]).
const RANK_FLAGS: [&str; 3] = ["type", "limit", "min-score"];

/// The custom flags registered onto the command tree:
/// (subcommand, flag id, field name) rows, consumed by
/// [`inject_dynamic`] after parsing.
#[derive(Default)]
pub(crate) struct DynamicFlags {
    entries: Vec<(&'static str, String, String)>,
}

/// Augments the [`AUGMENTED`] subcommands from the current store
/// when argv invokes one of them: a registry-gated choice set
/// becomes the registry's names and every undeclared custom field
/// gains its `--<field>` flag. Returns the registry failure when the
/// custom_types config is broken (the global config-error surface
/// renders it — the same failure `open_store` would raise, earlier,
/// like the reference's registry init).
pub(crate) fn augment(command: &mut clap::Command) -> Result<DynamicFlags, crate::output::Failure> {
    let Some(sub) = invoked_subcommand() else {
        return Ok(DynamicFlags::default());
    };
    if !AUGMENTED.iter().any(|row| row.sub == sub) {
        return Ok(DynamicFlags::default());
    }
    // a cwd-resolution failure stays with `open_store`'s rendering
    // (the same error resurfaces there)
    let Ok(cwd) = std::env::current_dir() else {
        return Ok(DynamicFlags::default());
    };
    let registry = match mulch::StoreFiles::locate(&cwd) {
        Ok(mulch::StoreLocation::Open(store)) => store.registry().clone(),
        // no store: the builtins parse, the command's own no-store
        // contract fires later (parity — the reference's fallback
        // registry is builtins-only)
        Ok(mulch::StoreLocation::Missing | mulch::StoreLocation::NoConfig) => {
            mulch::Registry::builtins()
        }
        // a broken custom_types config fails EVERY command through
        // the global surface (reference registry init); other errors
        // stay with `open_store`'s rendering
        Err(mulch::Error::CustomTypes { message }) => {
            return Err(crate::output::Failure::config_error(&message));
        }
        Err(_) => mulch::Registry::builtins(),
    };

    let mut dynamic = DynamicFlags::default();
    for row in AUGMENTED {
        let Some(slot) = command.find_subcommand_mut(row.sub) else {
            continue;
        };
        let mut taken = std::mem::take(slot);
        if let Some(choices_arg) = row.choices_arg {
            let choices: Vec<clap::builder::PossibleValue> = registry
                .rows()
                .iter()
                .map(|r| clap::builder::PossibleValue::new(r.name.clone()))
                .collect();
            // the derive's static choice set becomes the registry's
            // names (same id, same rejection wording family)
            taken = taken.mut_arg(choices_arg, |arg| {
                arg.value_parser(clap::builder::PossibleValuesParser::new(choices))
            });
        }
        if row.custom_flags {
            taken =
                register_custom_flags(taken, row.sub, &registry, row.declared_flags, &mut dynamic);
        }
        *slot = taken;
    }
    Ok(dynamic)
}

/// The per-subcommand augmentation table: which subcommands read the
/// store's registry before parsing, and what each augments. A later
/// registry-gated command (prune/archive) is a new row, not a new
/// if-chain arm.
struct AugmentRow {
    sub:            &'static str,
    /// Argument id whose choice set becomes the registry's names
    /// (`record`'s positional); `None` = no registry-gated choices.
    choices_arg:    Option<&'static str>,
    /// The command's own flag names: a custom field with one of
    /// these names reads the existing flag instead of getting a new
    /// one (the reference's `declaredOptionNames` set).
    declared_flags: &'static [&'static str],
    /// Whether the reference registers dynamic custom-field flags on
    /// this subcommand: record.ts/edit.ts do; search/rank only gate
    /// `--type`'s choices on the registry and declare no field
    /// flags of their own.
    custom_flags:   bool,
}

const AUGMENTED: [AugmentRow; 4] = [
    AugmentRow {
        sub:            "record",
        choices_arg:    Some("record_type"),
        declared_flags: &RECORD_FLAGS,
        custom_flags:   true,
    },
    AugmentRow {
        sub:            "edit",
        choices_arg:    None,
        declared_flags: &EDIT_FLAGS,
        custom_flags:   true,
    },
    AugmentRow {
        sub:            "search",
        choices_arg:    Some("record_type"),
        declared_flags: &SEARCH_FLAGS,
        custom_flags:   false,
    },
    AugmentRow {
        sub:            "rank",
        choices_arg:    Some("record_type"),
        declared_flags: &RANK_FLAGS,
        custom_flags:   false,
    },
];

/// Adds `--<field>` for every custom row field whose hyphenated name
/// the subcommand does not already declare (first come, like the
/// reference's `declaredOptionNames` set).
fn register_custom_flags(
    subcommand: clap::Command,
    name: &'static str,
    registry: &mulch::Registry,
    declared: &[&str],
    dynamic: &mut DynamicFlags,
) -> clap::Command {
    // the help prefix mirrors the reference's per-command wording
    // (record.ts "… field: …", edit.ts "update … field: …")
    let help_prefix = if name == "edit" { "update " } else { "" };
    let mut seen: Vec<String> = declared.iter().map(|flag| (*flag).to_string()).collect();
    let mut command = subcommand;
    for row in registry.rows().iter().filter(|row| !row.builtin) {
        for field in row.payload.iter().chain(row.optional.iter()) {
            let flag = field.replace('_', "-");
            if seen.contains(&flag) {
                continue;
            }
            seen.push(flag.clone());
            command = command.arg(
                clap::Arg::new(flag.clone())
                    .long(flag.clone())
                    .value_name(field.clone())
                    .help(format!("{help_prefix}{} field: {field}", row.name)),
            );
            dynamic.entries.push((name, flag, field.clone()));
        }
    }
    command
}

/// Carries the parsed dynamic values into the args structs.
pub(crate) fn inject_dynamic(cli: &mut Cli, matches: &clap::ArgMatches, dynamic: &DynamicFlags) {
    for (sub, flag, field) in &dynamic.entries {
        let Some(sub_matches) = matches.subcommand_matches(sub) else {
            continue;
        };
        let Some(value) = sub_matches.get_one::<String>(flag.as_str()) else {
            continue;
        };
        match &mut cli.command {
            Some(Command::Record(args)) if *sub == "record" => {
                args.custom.push((field.clone(), value.clone()));
            }
            Some(Command::Edit(args)) if *sub == "edit" => {
                args.custom.push((field.clone(), value.clone()));
            }
            _ => {}
        }
    }
}

/// Global flags that take a separate value token. The reference's
/// only value-taking global today is `--format <format>`
/// (cli.ts:130); `--format=x` carries its value inline and needs no
/// skip.
const VALUE_TAKING_GLOBALS: [&str; 1] = ["format"];

/// The first non-flag argv token (the subcommand position). Boolean
/// globals skip as-is; a [`VALUE_TAKING_GLOBALS`] long flag without
/// `=` also swallows its value token, so `--format compact record`
/// still resolves to `record`. `args_os` + lossy conversion: a
/// non-UTF-8 argv must fall through to clap's parse error, not
/// panic in the pre-parse scan.
fn invoked_subcommand() -> Option<String> {
    subcommand_from_argv(std::env::args_os().skip(1))
}

/// The scan law over an arbitrary token stream: skip flag tokens
/// (any token starting with `-`); a bare long flag naming a
/// value-taking global skips the next token too; the first
/// non-flag token is the subcommand.
fn subcommand_from_argv<I>(tokens: I) -> Option<String>
where
    I: Iterator<Item = std::ffi::OsString>,
{
    let mut tokens = tokens.peekable();
    while let Some(token) = tokens.next() {
        let token = token.to_string_lossy();
        let Some(long) = token.strip_prefix("--") else {
            if token.starts_with('-') {
                continue;
            }
            return Some(token.into_owned());
        };
        if long.contains('=') {
            continue;
        }
        if VALUE_TAKING_GLOBALS.contains(&long) {
            tokens.next();
        }
    }
    None
}

#[cfg(test)]
mod dynamic_flag_tests {
    use clap::CommandFactory as _;

    use super::*;

    /// The declared-flag lists are the drift pins for the dynamic
    /// custom-flag registration: every long flag the derive actually
    /// declares on `record`/`edit` must appear in the matching list,
    /// or a custom field of a new flag's name would register a
    /// colliding `--flag` instead of reading the builtin (the
    /// reference's declaredOptionNames set, record.ts:513-524).
    #[test]
    fn declared_flag_lists_cover_the_derive_surface() {
        let command = Cli::command();
        for row in &AUGMENTED {
            let (sub, declared) = (row.sub, row.declared_flags);
            let mut longs: Vec<String> = command
                .find_subcommand(sub)
                .unwrap_or_else(|| panic!("{sub} subcommand missing"))
                .get_arguments()
                .filter_map(clap::Arg::get_long_and_visible_aliases)
                .flatten()
                .map(std::string::ToString::to_string)
                .collect();
            longs.sort_unstable();
            longs.dedup();
            for long in longs {
                assert!(
                    declared.contains(&long.as_str()),
                    "{sub} declares --{long} but the {sub} skip-list misses it"
                );
            }
        }
    }

    /// The pre-parse's drift pin: a top-level arg that takes a value
    /// must be listed in [`VALUE_TAKING_GLOBALS`], or its value token
    /// would swallow the subcommand position, skip augmentation, and
    /// send a registry-gated `--type` to the flag path's `expect`
    /// (the law pinned against the reference's `--format compact
    /// record ... --type <custom>`, which records fine).
    #[test]
    fn value_taking_globals_are_listed() {
        let command = Cli::command();
        let unlisted: Vec<String> = command
            .get_arguments()
            .filter(|arg| {
                matches!(
                    arg.get_action(),
                    clap::ArgAction::Set | clap::ArgAction::Append
                )
            })
            .filter_map(clap::Arg::get_long_and_visible_aliases)
            .flatten()
            .map(std::string::ToString::to_string)
            .filter(|long| !VALUE_TAKING_GLOBALS.contains(&long.as_str()))
            .collect();
        assert!(
            unlisted.is_empty(),
            "top-level --{unlisted:?} takes a value but the pre-parse does not skip it"
        );
    }

    /// The scan law over synthetic argv: value-taking globals
    /// (`--format <val>`), inline `--format=val`, boolean globals,
    /// and the no-subcommand tail.
    #[test]
    fn subcommand_scan_skips_value_taking_globals() {
        use std::ffi::OsString;
        let argv = |parts: &[&str]| {
            parts
                .iter()
                .map(OsString::from)
                .collect::<Vec<_>>()
                .into_iter()
        };
        assert_eq!(
            subcommand_from_argv(argv(&["--format", "compact", "record", "dev"])),
            Some("record".into())
        );
        assert_eq!(
            subcommand_from_argv(argv(&["--format=compact", "record", "dev"])),
            Some("record".into())
        );
        assert_eq!(
            subcommand_from_argv(argv(&["--json", "-q", "edit", "dev", "mx-1"])),
            Some("edit".into())
        );
        // a flag after the subcommand never reaches the scan
        assert_eq!(
            subcommand_from_argv(argv(&["record", "--format", "compact"])),
            Some("record".into())
        );
        // value flag with no subcommand after it: clap owns the error
        assert_eq!(subcommand_from_argv(argv(&["--format", "compact"])), None);
    }

    /// A non-UTF-8 argv token falls through lossy: a flag-ish one is
    /// skipped, a positional one resolves lossily — the parse error
    /// stays clap's.
    #[cfg(unix)]
    #[test]
    fn subcommand_scan_takes_non_utf8_lossy() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt as _;
        let flagish = OsString::from_vec(vec![0x2d, 0xff, 0x2d, 0xff]);
        assert_eq!(
            subcommand_from_argv([flagish, "record".into()].into_iter()),
            Some("record".into())
        );
        let positional = OsString::from_vec(vec![0xff, 0xfe]);
        assert_eq!(
            subcommand_from_argv([positional].into_iter()),
            Some("\u{fffd}\u{fffd}".into())
        );
    }
}
