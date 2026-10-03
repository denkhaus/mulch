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
#[command(name = "mulch", version, disable_help_subcommand = true)]
pub(crate) struct Cli {
    #[command(flatten)]
    pub opts: GlobalOpts,

    #[command(subcommand)]
    pub command: Command,
}

/// The implemented command surface (help honesty: nothing else listed).
#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Initialize .mulch/ in the current project.
    Init,

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
