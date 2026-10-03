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
}

/// Flags of `mulch record` (reference surface).
#[derive(Debug, Args)]
pub(crate) struct RecordArgs {
    /// Target domain.
    pub(crate) domain: String,

    /// Positional content shorthand (convention).
    pub(crate) content: Option<String>,

    /// Record type.
    #[arg(
        long = "type",
        value_parser = ["convention", "pattern", "failure", "decision", "reference", "guide"]
    )]
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
    #[arg(long = "outcome-duration")]
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

    /// Read a JSON array of records from a file.
    #[arg(long)]
    pub(crate) batch: Option<std::path::PathBuf>,

    /// Preview without writing.
    #[arg(long = "dry-run")]
    pub(crate) dry_run: bool,
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
    #[arg(long = "outcome-duration")]
    pub(crate) duration: Option<String>,

    /// Test results summary.
    #[arg(long = "outcome-test-results")]
    pub(crate) test_results: Option<String>,

    /// Recording agent name.
    #[arg(long = "outcome-agent")]
    pub(crate) agent: Option<String>,
}

/// Outcome flags of `record --outcome-*` and the `outcome` command.
#[derive(Debug, Args)]
pub(crate) struct OutcomeFlags {
    /// Outcome verdict.
    #[arg(long = "status", value_parser = ["success", "failure", "partial"])]
    pub(crate) status: Option<String>,

    /// Duration in milliseconds.
    #[arg(long)]
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
