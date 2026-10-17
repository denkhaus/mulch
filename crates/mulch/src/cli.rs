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

/// The custom flags registered onto the command tree:
/// (subcommand, flag id, field name) rows, consumed by
/// [`inject_dynamic`] after parsing.
#[derive(Default)]
pub(crate) struct DynamicFlags {
    entries: Vec<(&'static str, String, String)>,
}

/// Augments `record`/`edit` from the current store when argv invokes
/// one of them: `--type`'s choice set becomes the registry's names
/// and every undeclared custom field gains its `--<field>` flag.
/// Returns the registry failure when the custom_types config is
/// broken (the global config-error surface renders it — the same
/// failure `open_store` would raise, earlier, like the reference's
/// registry init).
pub(crate) fn augment(command: &mut clap::Command) -> Result<DynamicFlags, crate::output::Failure> {
    let Some(sub) = invoked_subcommand() else {
        return Ok(DynamicFlags::default());
    };
    if sub != "record" && sub != "edit" {
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
    if let Some(record) = command.find_subcommand_mut("record") {
        let choices: Vec<clap::builder::PossibleValue> = registry
            .rows()
            .iter()
            .map(|row| clap::builder::PossibleValue::new(row.name.clone()))
            .collect();
        let mut taken = std::mem::take(record);
        // the derive's static choice set becomes the registry's names
        // (same id, same rejection wording family)
        taken = taken.mut_arg("record_type", |arg| {
            arg.value_parser(clap::builder::PossibleValuesParser::new(choices))
        });
        taken = register_custom_flags(taken, "record", &registry, &RECORD_FLAGS, &mut dynamic);
        *record = taken;
    }
    if let Some(edit) = command.find_subcommand_mut("edit") {
        let taken = std::mem::take(edit);
        let rebuilt = register_custom_flags(taken, "edit", &registry, &EDIT_FLAGS, &mut dynamic);
        *edit = rebuilt;
    }
    Ok(dynamic)
}

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

/// The first non-flag argv token (the subcommand position — every
/// global flag is a boolean). `args_os` + lossy conversion: a
/// non-UTF-8 argv must fall through to clap's parse error, not
/// panic in the pre-parse scan.
fn invoked_subcommand() -> Option<String> {
    std::env::args_os()
        .skip(1)
        .find(|token| !token.to_string_lossy().starts_with('-'))
        .map(|token| token.to_string_lossy().into_owned())
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
        for (sub, declared) in [("record", &RECORD_FLAGS[..]), ("edit", &EDIT_FLAGS[..])] {
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

    /// The pre-parse ([`invoked_subcommand`]) assumes every global
    /// flag is a boolean: a value-taking global flag would swallow the
    /// subcommand token, skip augmentation, and send a registry-gated
    /// `--type` to the flag path's `expect`. Pins the derive's
    /// top-level args to take no values.
    #[test]
    fn global_flags_take_no_values() {
        let command = Cli::command();
        let value_taking: Vec<String> = command
            .get_arguments()
            .filter(|arg| {
                matches!(
                    arg.get_action(),
                    clap::ArgAction::Set | clap::ArgAction::Append
                )
            })
            .map(|arg| arg.get_id().to_string())
            .collect();
        assert!(
            value_taking.is_empty(),
            "value-taking top-level args break the augment pre-parse: {value_taking:?}"
        );
    }
}
