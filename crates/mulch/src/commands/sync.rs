//! `mulch sync` — validate, stage, and commit `.mulch/` changes
//! (reference sync.ts, mulch-176f). Law map (probe-pinned 2026-10-26,
//! ml 0.10.7): git-repo check FIRST (its plain and json texts differ —
//! `Error: not in a git repository.` vs the full sentence); then the
//! validate step (unless `--no-validate`), which IGNORES the
//! `--allow-unknown-types` escape hatch — gatekeeping stops at the
//! commit gate; then the worktree skip (`isInsideWorktree`, a
//! config-resolution concern: `rev-parse --git-common-dir` ending in
//! `.git` whose parent is not the cwd — submodules end in a `modules/`
//! path and never skip); then the `.mulch/`-scoped has-changes check;
//! then add + commit through the git process adapter (mulch-cc7a),
//! whose failures render `Git error: <Node throw shape>`.
//!
//! Channels: every success envelope prints to stdout, the error
//! envelopes to stderr (outputJsonError); plain successes are
//! quiet-gated, plain failures never are. The validation-failure JSON
//! rides the SUCCESS channel with `success: false` (outputJson, not
//! outputJsonError) — sync prints it itself and keeps only the exit
//! code in the Failure.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::cli::GlobalOpts;
use crate::commands::git::{self, GitError};
use crate::output::{Failure, print_json, print_line, success_envelope};

/// The default commit message (reference sync.ts:272).
const DEFAULT_MESSAGE: &str = "mulch: update expertise";

/// The no-store sentence the reference's config reader throws
/// (utils/config.ts readConfig), which sync renders under its
/// `Validation error:` prefix.
const NO_STORE: &str = "No .mulch/ directory found. Run `mulch init` to set up this project.";

/// Runs `sync` from the current directory.
pub(super) fn run(
    opts: &GlobalOpts,
    message: Option<String>,
    no_validate: bool,
) -> Result<(), Failure> {
    let cwd = std::env::current_dir()
        .map_err(|source| Failure::handled_on_stderr("sync", format!("resolving cwd: {source}")))?;

    // Repo first (sync.ts order): the no-repo error precedes every
    // store read. Plain and json texts DIFFER here — the reference's
    // plain line is the short lowercase sentence.
    if !git::is_git_repo(&cwd) {
        return Err(Failure::divergent(
            "sync",
            "Error: not in a git repository.",
            "Not in a git repository. Run this command from within a git repository.",
        ));
    }

    // The validate step. A store that cannot even open is a thrown
    // error (`Validation error: <message>`), never a findings list.
    if !no_validate {
        let result = validate_expertise(&cwd)?;
        if !result.valid {
            return Err(validation_failure(opts, &result));
        }
        if !opts.json && !opts.quiet {
            print_line(
                false,
                &format!("✓ Validated {} records", result.total_records),
            );
        }
    }

    // The worktree skip: changes belong to the main repo's .mulch/.
    if inside_worktree(&cwd) {
        finish(
            opts,
            no_validate,
            false,
            "Skipped commit: running inside a git worktree. Changes are written to the main repo's .mulch/.",
            Some("ℹ Skipped commit: running inside a git worktree. Changes are written to the main repo's .mulch/.".into()),
        );
        return Ok(());
    }

    // No changes: a clean success, never an error.
    if !git::has_changes(&cwd) {
        finish(opts, no_validate, false, "No changes to commit", None);
        return Ok(());
    }

    // Stage + commit through the adapter; a git failure renders the
    // Node throw shape under the `Git error:` prefix.
    let commit_message = message.unwrap_or_else(|| DEFAULT_MESSAGE.into());
    match git::add(&cwd).and_then(|()| git::commit(&cwd, &commit_message)) {
        Ok(_) => {
            finish(
                opts,
                no_validate,
                true,
                &commit_message,
                Some(format!("✓ Committed .mulch/ changes: \"{commit_message}\"")),
            );
            Ok(())
        }
        Err(source) => Err(git_failure(&source)),
    }
}

/// One validation finding (`domain:line` addressed).
struct Finding {
    domain:  String,
    line:    usize,
    message: String,
}

/// The validate step's outcome (reference ValidateResult).
struct ValidateResult {
    valid:         bool,
    total_records: usize,
    errors:        Vec<Finding>,
}

/// The sync-shaped validation walk (sync.ts:60-172): every configured
/// domain's file, line-addressed; JSON failures, unknown types,
/// schema verdicts and the domain rules (allowed_types /
/// required_fields) are findings; escape hatches do NOT apply. The
/// registry re-reads from disk — sync validates against the config
/// as it is NOW (worktree/CI lag reconciles at the commit gate).
fn validate_expertise(cwd: &Path) -> Result<ValidateResult, Failure> {
    let store = match mulch::StoreFiles::locate(cwd) {
        Ok(mulch::StoreLocation::Open(store)) => store,
        Ok(_) => return Err(validation_error("sync", NO_STORE)),
        Err(source) => {
            return Err(validation_error(
                "sync",
                &crate::output::chain_message(&source),
            ));
        }
    };
    let config = store.config();
    let mut errors = Vec::new();
    let mut total_records = 0usize;
    for domain in store.domains() {
        // Empty/missing rule lists mean "no rule" (domain-rules.ts).
        let allowed = config
            .allowed_types(&domain)
            .ok()
            .flatten()
            .filter(|list| !list.is_empty());
        let required = config
            .required_fields(&domain)
            .ok()
            .flatten()
            .filter(|list| !list.is_empty());
        // The lenient reader (aliases applied at the seam, `#` lines
        // are findings): exactly the reference's raw split/trim loop.
        for finding in store.read_lenient(&domain) {
            total_records += 1;
            match finding {
                mulch::LenientLine::Malformed { line } => errors.push(Finding {
                    domain: domain.clone(),
                    line,
                    message: "Invalid JSON: failed to parse".into(),
                }),
                mulch::LenientLine::Record { line, record } => {
                    // Unknown type is the record's only finding
                    // (validate_message's bytes are shared with
                    // validate.ts).
                    if let Some(message) =
                        mulch::schema::validate_message(store.registry(), &record)
                    {
                        errors.push(Finding {
                            domain: domain.clone(),
                            line,
                            message,
                        });
                        continue;
                    }
                    let Some(object) = record.as_object() else {
                        continue;
                    };
                    // The domain rules: allowed types first, then the
                    // missing-field subset (undefined/null/""/empty
                    // array count as missing — domain-rules.ts).
                    let record_type = object
                        .get("type")
                        .and_then(Value::as_str)
                        .unwrap_or("<unknown>");
                    let id_part = object
                        .get("id")
                        .and_then(Value::as_str)
                        .map_or(String::new(), |id| format!(" (id={id})"));
                    if let Some(allowed) = allowed.as_ref()
                        && !allowed.iter().any(|kind| kind == record_type)
                    {
                        errors.push(Finding {
                            domain: domain.clone(),
                            line,
                            message: format!(
                                "type \"{record_type}\"{id_part} is not allowed in domain \
                                 \"{domain}\". Allowed types: {}. Fix the record or relax \
                                 allowed_types in mulch.config.yaml.",
                                allowed.join(", ")
                            ),
                        });
                        continue;
                    }
                    if let Some(required) = required.as_ref() {
                        let missing = mulch::schema::missing_domain_fields(&record, required);
                        if !missing.is_empty() {
                            let quoted = missing
                                .iter()
                                .map(|field| format!("\"{field}\""))
                                .collect::<Vec<_>>()
                                .join(", ");
                            errors.push(Finding {
                                domain: domain.clone(),
                                line,
                                message: format!(
                                    "record{id_part} missing required field(s) {quoted} in \
                                     domain \"{domain}\". Fix the record or relax required_fields \
                                     in mulch.config.yaml."
                                ),
                            });
                        }
                    }
                }
            }
        }
    }
    let valid = errors.is_empty();
    Ok(ValidateResult {
        valid,
        total_records,
        errors,
    })
}

/// `isInsideWorktree` (utils/config.ts:178-189): inside a linked
/// worktree `--git-common-dir` names the MAIN repo's `.git` (a path
/// ENDING in `.git` — submodules end in `.git/modules/<name>` and are
/// NOT worktrees), and that dir's parent is not the cwd.
fn inside_worktree(cwd: &Path) -> bool {
    let Some(raw) = git::run(cwd, &["rev-parse", "--git-common-dir"]) else {
        return false;
    };
    let raw = raw.trim();
    if raw.is_empty() {
        return false;
    }
    // Node's resolve is lexical (no symlink walk): absolute stays,
    // relative joins the cwd.
    let common: PathBuf = if Path::new(raw).is_absolute() {
        PathBuf::from(raw)
    } else {
        cwd.join(raw)
    };
    if !common.ends_with(".git") {
        return false;
    }
    let Some(main_root) = common.parent() else {
        return false;
    };
    main_root != cwd
}

/// The shared success rendering: the JSON envelope on stdout, the
/// plain line (quiet-gated) with the `ℹ`/`✓` prefix when there is
/// one — the no-changes success has no prefix at all.
fn finish(
    opts: &GlobalOpts,
    no_validate: bool,
    committed: bool,
    message: &str,
    plain_line: Option<String>,
) {
    if opts.json {
        let mut fields = Map::new();
        fields.insert("validated".into(), Value::Bool(!no_validate));
        fields.insert("committed".into(), Value::Bool(committed));
        fields.insert("message".into(), Value::String(message.into()));
        print_json(&success_envelope("sync", fields), false);
    } else if !opts.quiet {
        match plain_line {
            Some(line) => print_line(false, &line),
            None => print_line(false, "No .mulch/ changes to commit."),
        }
    }
}

/// The validation-failed surface: JSON on the SUCCESS channel with
/// `success: false` (outputJson), plain details on stderr — both
/// printed by the command; the Failure carries only the exit code.
fn validation_failure(opts: &GlobalOpts, result: &ValidateResult) -> Failure {
    if opts.json {
        let errors: Vec<Value> = result
            .errors
            .iter()
            .map(|finding| {
                let mut item = Map::new();
                item.insert("domain".into(), Value::String(finding.domain.clone()));
                item.insert(
                    "line".into(),
                    Value::from(u64::try_from(finding.line).unwrap_or(0)),
                );
                item.insert("message".into(), Value::String(finding.message.clone()));
                Value::Object(item)
            })
            .collect();
        let mut fields = Map::new();
        fields.insert("validated".into(), Value::Bool(false));
        fields.insert("committed".into(), Value::Bool(false));
        fields.insert("errors".into(), Value::Array(errors));
        print_json(&failure_body("sync", fields), false);
    } else {
        // The reference prints the failure block on stderr (validate's
        // channel law for details).
        #[allow(clippy::print_stderr, reason = "the failure details render on stderr")]
        {
            eprintln!("Validation failed:");
            for finding in &result.errors {
                eprintln!(
                    "  {}:{} - {}",
                    finding.domain, finding.line, finding.message
                );
            }
            eprintln!("\nFix errors and retry, or use --no-validate to skip.");
        }
    }
    Failure::rendered_exit("sync")
}

/// The `Validation error: <message>` surface (a THROWN error, not a
/// findings list): plain stderr without the `Error:` idiom, the json
/// envelope on stderr.
fn validation_error(command: &str, message: &str) -> Failure {
    let mut failure = Failure::handled(command, format!("Validation error: {message}"));
    failure.envelope_to_stderr = true;
    failure
}

/// The `Git error: <Node throw shape>` surface: plain stderr, json
/// envelope on stderr, exit 1.
fn git_failure(source: &GitError) -> Failure {
    let mut failure = Failure::handled("sync", format!("Git error: {source}"));
    failure.envelope_to_stderr = true;
    failure
}

/// The reference's failure envelope with caller-ordered fields
/// (`outputJson` of a `success: false` body — the SUCCESS channel,
/// unlike [`Failure::handled`]'s stderr contract).
fn failure_body(command: &str, fields: Map<String, Value>) -> Value {
    let mut body = Map::new();
    body.insert("success".into(), Value::Bool(false));
    body.insert("command".into(), Value::String(command.into()));
    for (key, value) in fields {
        body.insert(key, value);
    }
    Value::Object(body)
}
