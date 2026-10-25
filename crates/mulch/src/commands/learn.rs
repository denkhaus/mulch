//! `mulch learn` — changed files plus domain suggestions for
//! recording learnings (mulch-16da). Reference: commands/learn.ts —
//! git check, `.mulch/` existence (a plain-mode SILENT skip without
//! it), `getChangedFiles` (committed-since + staged + unstaged,
//! untracked never appear), `matchFilesToDomains` (config-order
//! domains, registry-driven file sets, count-descending suggestions).
//! Probed live vs ml 0.10.7, 2026-10-23 (/tmp/opencode/childC).

use mulch::StoreLocation;
use mulch::git_context::{DomainFileMatches, match_files_to_domains};
use serde_json::{Map, Value};

use crate::cli::GlobalOpts;
use crate::commands::failure::render_core_error;
use crate::commands::git_context::GitContext;
use crate::commands::render::promote_mulch;
use crate::commands::store::{NO_STORE_CONFIG_MESSAGE, locate};
use crate::output::{Failure, print_json, print_line, success_envelope};

/// Runs `learn` (plain and `--json`).
pub(crate) fn run(opts: &GlobalOpts, since: &str) -> Result<(), Failure> {
    // The registry read precedes every command's own logic (the
    // reference bootstraps the registry globally — a broken
    // custom_types block fails even before learn's git check, probed).
    let cwd = std::env::current_dir()
        .map_err(|source| Failure::handled("learn", format!("resolving cwd: {source}")))?;
    let located = locate(&cwd).map_err(|source| match &source {
        mulch::Error::CustomTypes { message } => Failure::config_error(message),
        other => Failure::handled("learn", crate::output::chain_message(other)),
    })?;

    let git = GitContext::process_cwd();
    if !git.is_git_repo() {
        // The two surfaces carry DIFFERENT sentences (not a prefix
        // strip): plain is lowercase, json adds the remediation.
        let mut failure = Failure::handled_on_stderr("learn", "Error: not in a git repository.");
        if opts.json
            && let Some(body) = failure.envelope.as_object_mut()
        {
            body.insert(
                "error".into(),
                Value::String(
                    "Not in a git repository. Run this command from within a git repository."
                        .into(),
                ),
            );
        }
        return Err(failure);
    }

    // No .mulch/ directory — exit cleanly (hook usage in non-mulch
    // projects): plain prints NOTHING, json prints the skip envelope.
    if matches!(located, StoreLocation::Missing) {
        if opts.json {
            print_json(&skip_envelope(), false);
        }
        return Ok(());
    }

    let changed = git.changed_files_since(since);
    if changed.is_empty() {
        if opts.json {
            let mut envelope = skip_envelope();
            if let Some(body) = envelope.as_object_mut() {
                body.insert(
                    "message".into(),
                    Value::String("No changed files found".into()),
                );
            }
            print_json(&envelope, false);
        } else {
            print_line(false, "No changed files found. Nothing to learn from.");
        }
        return Ok(());
    }

    // `.mulch/` exists but the config does not: readConfig's ENOENT
    // fires only now, inside matchFilesToDomains (a no-changes repo
    // never reaches it — probed).
    let StoreLocation::Open(store) = located else {
        return Err(Failure::handled_on_stderr("learn", NO_STORE_CONFIG_MESSAGE));
    };

    let found = match_files_to_domains(&store, &changed, opts.allow_unknown_types)
        .map_err(|source| Failure::handled_on_stderr("learn", render_core_error(&source)))?;
    let DomainFileMatches { matches, unmatched } = found;
    // Suggestions sort by match count descending, ties keep config
    // order (JS sort is stable).
    let mut matches = matches;
    matches.sort_by_key(|m| std::cmp::Reverse(m.matched_files.len()));

    if opts.json {
        let mut fields = Map::new();
        fields.insert("changedFiles".into(), Value::from(changed.clone()));
        fields.insert(
            "suggestedDomains".into(),
            Value::Array(
                matches
                    .iter()
                    .map(|m| {
                        serde_json::json!({
                            "domain": m.domain,
                            "matchCount": m.matched_files.len(),
                            "files": m.matched_files,
                        })
                    })
                    .collect(),
            ),
        );
        fields.insert("unmatchedFiles".into(), Value::from(unmatched.clone()));
        print_json(&success_envelope("learn", fields), false);
        return Ok(());
    }

    print_line(false, &plain_output(&changed, &matches, &unmatched));
    Ok(())
}

/// The skip envelope shape shared by the no-store and no-changes json
/// paths (the caller overwrites `message` for the latter).
fn skip_envelope() -> Value {
    success_envelope(
        "learn",
        [
            ("changedFiles".to_string(), Value::Array(Vec::new())),
            ("suggestedDomains".to_string(), Value::Array(Vec::new())),
            ("unmatchedFiles".to_string(), Value::Array(Vec::new())),
            (
                "message".to_string(),
                Value::String("No .mulch/ directory found — skipping".into()),
            ),
        ]
        .into_iter()
        .collect(),
    )
}

/// The plain body: session header, changed files, suggestions,
/// unmatched files, and the closing record hint (the hint names THIS
/// binary — the promote law, user directive 2026-10-13; the battery
/// normalizes the name). The trailing blank line is print_line's own.
fn plain_output(
    changed: &[String],
    matches: &[mulch::git_context::DomainMatch],
    unmatched: &[String],
) -> String {
    use std::fmt::Write as _;

    let mut text = String::from("\nSession learnings check\n\n");
    let _ = writeln!(text, "Changed files ({}):", changed.len());
    for file in changed {
        let _ = writeln!(text, "  {file}");
    }
    if !matches.is_empty() {
        text.push_str("\nSuggested domains:\n");
        for m in matches {
            let label = if m.matched_files.len() == 1 {
                "file matches"
            } else {
                "files match"
            };
            let _ = writeln!(
                text,
                "  {} ({} {label} existing records)",
                m.domain,
                m.matched_files.len()
            );
        }
    }
    if !unmatched.is_empty() {
        text.push_str("\nUnmatched files (no domain association):\n");
        for file in unmatched {
            let _ = writeln!(text, "  {file}");
        }
    }
    text.push_str("\nRecord learnings with:\n");
    let _ = writeln!(
        text,
        "{}",
        promote_mulch("  ml record <domain> --type <type> --description \"...\"")
    );
    text
}
