//! `mulch prime` — session priming with relevance-scoped expertise
//! (mulch-16da).
//!
//! Probed law (ml 0.10.7, 2026-10-23, /tmp/sprint35b/probes):
//! - Flag-combination guards run FIRST (`--manifest --full`, `--manifest
//!   --dry-run`, `--manifest` + scoping), then the strict `--budget` parse
//!   (after the `--context` gate, before any store read).
//! - Unscoped, unconfigured invocations auto-flip to the manifest above 100
//!   records or 5 domains (strict `>`); scoping flags, `--dry-run`, config
//!   `prime.default_mode` and json mode all opt out or override.
//! - `--context` outside git is an error; with no changed files the plain run
//!   prints the notice on STDOUT with exit 0 and the json run prints the
//!   failure envelope on STDERR, still exit 0.
//! - Full mode auto-scopes to the git working set (status + tracker resolvers)
//!   unless `--all`; the scope note goes to stderr.
//! - `--export` writes `output + \n` and confirms on stdout unless
//!   `--quiet`/`--json`; the file content is byte-identical to the console
//!   output.
//! - Hooks: the reference runs `pre-prime` hooks here; this port has no hook
//!   engine yet (README DEVIATIONS, seed mulch-e3a1) — a config that declares
//!   them surfaces in the contract block only.

use std::collections::HashMap;

use jiff::Timestamp;
use mulch::budget::{DomainRecords, apply_budget, estimate_tokens};
use mulch::prime_ranking::{
    ActiveContext, build_surface_annotations, resolve_tier_weights, sort_by_trust,
};
use mulch::{Config, Governance, StoreFiles};
use serde_json::{Map, Value};

use crate::cli::{GlobalOpts, PrimeArgs};
use crate::commands::active_work::resolve_active_work;
use crate::commands::git_context::GitContext;
use crate::commands::prime::format::{
    ManifestDomain, contract_block, manifest_output, manifest_payload, prime_output_compact,
    prime_output_markdown, prime_output_plain, prime_output_xml, session_close,
};
use crate::commands::render::{
    estimate_record_text, format_domain_expertise, format_domain_expertise_compact,
    format_domain_expertise_plain, format_domain_expertise_xml,
};
use crate::output::{Failure, print_line};

mod format;

/// One loaded domain: records plus the file's mtime.
struct Loaded {
    domain:       String,
    records:      Vec<Value>,
    last_updated: Option<Timestamp>,
}

/// Runs `prime` against the store at the current directory.
pub(super) fn run(opts: &GlobalOpts, args: &PrimeArgs) -> Result<(), Failure> {
    let store = crate::commands::open_store("prime", true)?;
    let config = store.config();
    let registry = store.registry();
    let json_mode = opts.json;

    let prime_format = opts
        .format
        .clone()
        .unwrap_or_else(|| if args.full { "markdown" } else { "compact" }.to_string());

    if args.manifest && args.full {
        return Err(prime_error(
            json_mode,
            "Cannot combine --manifest with --full.",
        ));
    }
    if args.dry_run && args.manifest {
        return Err(prime_error(
            json_mode,
            "Cannot combine --dry-run with --manifest. Manifest mode lists domains, not records; --dry-run previews which records would be primed.",
        ));
    }

    // positional domains first, then --domain, deduped first-wins.
    let mut requested: Vec<String> = Vec::new();
    for domain in args.domains.iter().chain(args.domain.iter()) {
        if !requested.contains(domain) {
            requested.push(domain.clone());
        }
    }
    let excluded: Vec<String> = args.exclude_domain.clone();

    let is_scoped =
        !requested.is_empty() || !excluded.is_empty() || args.context || !args.files.is_empty();

    if args.manifest && is_scoped {
        return Err(prime_error(
            json_mode,
            "--manifest cannot be combined with scoping arguments. Manifest mode lists available domains; use `mulch prime <domain>` or `mulch prime --files <path>` to load records.",
        ));
    }

    let default_mode = config
        .prime()
        .ok()
        .flatten()
        .and_then(|prime| prime.default_mode.clone());

    // Unknown-domain validation, requested first, then exclusions.
    let available = store.domains();
    for domain in &requested {
        if !available.contains(domain) {
            return Err(unknown_domain_failure(json_mode, domain, &available, false));
        }
    }
    for domain in &excluded {
        if !available.contains(domain) {
            return Err(unknown_domain_failure(json_mode, domain, &available, true));
        }
    }
    let target_domains = mulch::domain_set(&available, &mulch::DomainSpec::Union {
        domains: requested,
        exclude: excluded,
    })
    .unwrap_or_default();

    // --context resolves the changed-file set before the budget parse.
    let mut files_to_filter: Option<Vec<String>> = None;
    if args.context {
        let git = GitContext::process_cwd();
        if !git.is_git_repo() {
            return Err(prime_error(
                json_mode,
                "Not in a git repository. --context requires git.",
            ));
        }
        let changed = git.changed_files_since("HEAD~1");
        if changed.is_empty() {
            if json_mode {
                let mut fields = Map::new();
                fields.insert(
                    "error".into(),
                    Value::String("No changed files found. Nothing to filter by.".into()),
                );
                let mut failure = Failure::handled_on_stderr(
                    "prime",
                    "No changed files found. Nothing to filter by.",
                );
                failure.code = 0;
                failure.envelope = crate::output::failure_envelope("prime", fields);
                return Err(failure);
            }
            print_line(opts.quiet, "No changed files found. Nothing to filter by.");
            return Ok(());
        }
        files_to_filter = Some(changed);
    } else if !args.files.is_empty() {
        files_to_filter = Some(args.files.clone());
    }

    // Budget: enabled in plain mode unless --no-limit.
    let budget_enabled = !json_mode && !args.no_limit;
    let budget = match args.budget.as_deref() {
        Some(raw) => match parse_strict_positive_int(raw) {
            Some(value) => value,
            None => {
                return Err(prime_error(
                    json_mode,
                    &format!("--budget must be a positive integer (got \"{raw}\")."),
                ));
            }
        },
        None => mulch::budget::DEFAULT_BUDGET,
    };

    // Load every target domain once — the flip decision and the
    // format pipeline share the dataset.
    let mut loaded: Vec<Loaded> = Vec::new();
    for domain in &target_domains {
        let records = store
            .read_records(domain, opts.allow_unknown_types)
            .map_err(|source| {
                Failure::handled_on_stderr("prime", crate::commands::render_core_error(&source))
            })?;
        let last_updated = modified(&store, domain)?;
        loaded.push(Loaded {
            domain: domain.clone(),
            records: records.into_iter().map(|row| row.record).collect(),
            last_updated,
        });
    }

    // Effective mode: scoping forces records, explicit flags/config
    // win, --dry-run previews records, else the size threshold.
    let use_manifest = if is_scoped {
        false
    } else if args.manifest {
        true
    } else if args.full || default_mode.as_deref() == Some("full") {
        false
    } else if default_mode.as_deref() == Some("manifest") {
        true
    } else if args.dry_run {
        false
    } else {
        should_auto_flip_to_manifest(
            loaded.iter().map(|domain| domain.records.len()).sum(),
            loaded.len(),
        )
    };

    // Auto-context-scope: narrow to the git working set in plain,
    // unscoped, full-mode runs unless --all.
    let mut active_context: Option<ActiveContext> = None;
    let mut scoped_from_git = false;
    if !use_manifest && !is_scoped && !json_mode && !args.all && files_to_filter.is_none() {
        let git = GitContext::process_cwd();
        if git.is_git_repo() {
            let changed_files = git.active_files();
            let work = resolve_active_work(&std::env::current_dir().unwrap_or_default());
            let context = ActiveContext {
                changed_files,
                trackers: work.trackers(),
            };
            if mulch::git_context::active_context_has_signal(&context) {
                scoped_from_git = true;
                active_context = Some(context);
            }
        }
    }

    if let Some(files) = &files_to_filter {
        for domain in &mut loaded {
            domain.records = mulch::git_context::filter_by_context(&domain.records, files);
        }
        if !json_mode {
            loaded.retain(|domain| !domain.records.is_empty());
        }
    } else if let Some(context) = active_context.as_ref() {
        let mut scoped_total = 0;
        let mut scoped_kept = 0;
        for domain in &mut loaded {
            scoped_total += domain.records.len();
            domain.records = mulch::git_context::filter_by_active_context(&domain.records, context);
            scoped_kept += domain.records.len();
        }
        if scoped_from_git {
            let mut sources: Vec<String> = Vec::new();
            if !context.changed_files.is_empty() {
                sources.push("git status".into());
            }
            let tracker_bits: Vec<String> = crate::commands::active_work::TRACKERS
                .iter()
                .filter_map(|tracker| {
                    context
                        .trackers
                        .iter()
                        .find(|(name, _)| name == tracker)
                        .map(|(_, value)| format!("{tracker}:{value}"))
                })
                .collect();
            if !tracker_bits.is_empty() {
                sources.push(format!("active-work ({})", tracker_bits.join(", ")));
            }
            let source = if sources.is_empty() {
                "git status".to_string()
            } else {
                sources.join(" + ")
            };
            scope_note(scoped_kept, scoped_total, &source);
        }
    }

    let session_close_config = config
        .prime()
        .ok()
        .flatten()
        .and_then(|prime| prime.session_close.clone());
    let contract = if json_mode || args.dry_run {
        None
    } else {
        contract_block(config, registry, &prime_format)
    };

    // The annotation context mirrors the filter step: explicit
    // --files/--context paths treat the requested paths as the
    // working set, auto-scope passes its signal through.
    let annotation_context = files_to_filter.as_ref().map_or_else(
        || active_context.clone(),
        |files| {
            Some(ActiveContext {
                changed_files: files.clone(),
                trackers:      Vec::new(),
            })
        },
    );

    let output = if use_manifest {
        let domains: Vec<ManifestDomain> = loaded
            .iter()
            .map(|domain| ManifestDomain {
                domain:       domain.domain.clone(),
                count:        domain.records.len(),
                last_updated: domain.last_updated,
                type_counts:  type_counts(&domain.records),
            })
            .collect();
        if json_mode {
            manifest_payload(&domains, &effective_governance(config))
        } else {
            let mut text = manifest_output(
                &domains,
                &effective_governance(config),
                &prime_format,
                Timestamp::now(),
            );
            if prime_format != "plain" {
                let reminder = session_close(&prime_format, session_close_config.as_ref());
                if !reminder.is_empty() {
                    text.push_str("\n\n");
                    text.push_str(&reminder);
                }
            }
            Value::String(text)
        }
    } else {
        full_mode_output(
            opts,
            args,
            &store,
            &loaded,
            &prime_format,
            budget_enabled,
            budget,
            annotation_context.as_ref(),
            files_to_filter.is_some(),
            session_close_config.as_ref(),
        )
    };
    let output = match output {
        Value::String(text) => text,
        value => serde_json::to_string_pretty(&value).unwrap_or_default(),
    };

    let output = contract.map_or_else(
        || output.clone(),
        |contract| format!("{contract}\n\n{output}"),
    );

    if let Some(path) = args.export.as_deref() {
        std::fs::write(path, format!("{output}\n"))
            .map_err(|source| Failure::handled_on_stderr("prime", format!("Error: {source}")))?;
        if !json_mode && !opts.quiet {
            print_line(false, &format!("✓ Exported to {path}"));
        }
    } else {
        print_line(false, &output);
    }
    Ok(())
}

/// The auto-scope note (console.error): a stderr side channel the
/// reference emits before any stdout rendering.
#[allow(
    clippy::print_stderr,
    reason = "the reference's console.error note; the output module's boundary rule covers stdout printing"
)]
fn scope_note(kept: usize, total: usize, source: &str) {
    eprintln!(
        "prime: scoped to {kept} of {total} records based on {source}; run with --all for the full corpus"
    );
}

/// The full-mode (record-bearing) pipeline: tier sort, dry-run
/// preview, budget, annotations, the four families.
#[allow(clippy::too_many_arguments, reason = "the pipeline's inputs")]
fn full_mode_output(
    opts: &GlobalOpts,
    args: &PrimeArgs,
    store: &StoreFiles,
    loaded: &[Loaded],
    prime_format: &str,
    budget_enabled: bool,
    budget: usize,
    annotation_context: Option<&ActiveContext>,
    files_filter: bool,
    session_close_config: Option<&mulch::SessionClose>,
) -> Value {
    let registry = store.registry();
    let prime_config = store.config().prime().ok().flatten();
    let tier_weights = resolve_tier_weights(
        prime_config
            .as_ref()
            .and_then(|prime| prime.tier_weights.as_ref()),
    );
    let loaded: Vec<(String, Vec<Value>, Option<Timestamp>)> = loaded
        .iter()
        .map(|domain| {
            (
                domain.domain.clone(),
                sort_by_trust(&domain.records, &tier_weights),
                domain.last_updated,
            )
        })
        .collect();

    // --dry-run: JSON preview under the same budget rules.
    if args.dry_run {
        let now = Timestamp::now();
        let domains: Vec<DomainRecords> = loaded
            .iter()
            .map(|(domain, records, _)| DomainRecords {
                domain:  domain.clone(),
                records: records.clone(),
            })
            .collect();
        let kept = if budget_enabled {
            apply_budget(&domains, budget, |record, _| {
                estimate_record_text(registry, record, now)
            })
            .kept
        } else {
            domains
        };
        let mut would_prime: Vec<Value> = Vec::new();
        let mut total_tokens = 0_usize;
        for group in &kept {
            for record in &group.records {
                let tokens = estimate_tokens(&estimate_record_text(registry, record, now));
                let mut row = Map::new();
                row.insert(
                    "id".into(),
                    Value::String(
                        record
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .into(),
                    ),
                );
                row.insert(
                    "type".into(),
                    Value::String(
                        record
                            .get("type")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .into(),
                    ),
                );
                row.insert("domain".into(), Value::String(group.domain.clone()));
                row.insert("tokens".into(), Value::from(tokens));
                would_prime.push(Value::Object(row));
                total_tokens += tokens;
            }
        }
        let mut payload = Map::new();
        payload.insert("wouldPrime".into(), Value::Array(would_prime));
        payload.insert("totalTokens".into(), Value::from(total_tokens));
        payload.insert(
            "budgetUsed".into(),
            if budget_enabled {
                js_number(total_tokens as f64 / budget as f64)
            } else {
                Value::Null
            },
        );
        payload.insert(
            "budgetTotal".into(),
            if budget_enabled {
                Value::from(budget)
            } else {
                Value::Null
            },
        );
        return Value::Object(payload);
    }

    if opts.json {
        let mut domains: Vec<Value> = Vec::new();
        for (domain, records, _) in &loaded {
            if files_filter && records.is_empty() {
                continue;
            }
            let mut row = Map::new();
            row.insert("domain".into(), Value::String(domain.clone()));
            row.insert("entry_count".into(), Value::from(records.len()));
            row.insert("records".into(), Value::Array(records.clone()));
            domains.push(Value::Object(row));
        }
        let mut payload = Map::new();
        payload.insert("type".into(), Value::String("expertise".into()));
        payload.insert("domains".into(), Value::Array(domains));
        return Value::Object(payload);
    }

    // Plain rendering: budget first, then annotations, then the
    // per-domain sections and the family envelope.
    let now = Timestamp::now();
    let all_domains: Vec<DomainRecords> = loaded
        .iter()
        .map(|(domain, records, _)| DomainRecords {
            domain:  domain.clone(),
            records: records.clone(),
        })
        .collect();
    let (domain_records, dropped_count, dropped_domain_count) = if budget_enabled {
        let result = apply_budget(&all_domains, budget, |record, _| {
            estimate_record_text(registry, record, now)
        });
        (
            result.kept,
            result.dropped_count,
            result.dropped_domain_count,
        )
    } else {
        (all_domains, 0, 0)
    };

    let mut sections: Vec<String> = Vec::new();
    for group in &domain_records {
        let last_updated = loaded
            .iter()
            .find(|(domain, _, _)| domain == &group.domain)
            .and_then(|(_, _, updated)| *updated);
        let annotations: HashMap<String, String> =
            build_surface_annotations(&group.records, annotation_context)
                .into_iter()
                .collect();
        let annotations = (!annotations.is_empty()).then_some(annotations);
        sections.push(match prime_format {
            "xml" => format_domain_expertise_xml(
                registry,
                &group.domain,
                &group.records,
                last_updated,
                annotations.as_ref(),
                now,
            ),
            "plain" => format_domain_expertise_plain(
                registry,
                &group.domain,
                &group.records,
                last_updated,
                annotations.as_ref(),
                now,
            ),
            "compact" => format_domain_expertise_compact(
                registry,
                &group.domain,
                &group.records,
                last_updated,
                annotations.as_ref(),
                now,
            ),
            _ => format_domain_expertise(
                registry,
                &group.domain,
                &group.records,
                last_updated,
                args.full || opts.verbose,
                annotations.as_ref(),
                now,
            ),
        });
    }

    let mut output = match prime_format {
        "xml" => prime_output_xml(&sections),
        "plain" => prime_output_plain(&sections),
        "compact" => prime_output_compact(&sections),
        _ => prime_output_markdown(&sections),
    };
    if dropped_count > 0 {
        output.push_str("\n\n");
        output.push_str(&mulch::budget::format_budget_summary(
            dropped_count,
            dropped_domain_count,
        ));
    }
    if prime_format != "plain" {
        let reminder = session_close(prime_format, session_close_config);
        if !reminder.is_empty() {
            output.push_str("\n\n");
            output.push_str(&reminder);
        }
    }
    Value::String(output)
}

/// `shouldAutoFlipToManifest`: strict `>` on both thresholds.
fn should_auto_flip_to_manifest(total_records: usize, total_domains: usize) -> bool {
    total_records > 100 || total_domains > 5
}

/// `parseStrictPositiveInt`: digits-only, finite, ≥ 1 — JS `Number`
/// semantics, so huge digit strings stay finite and clamp to the
/// usize budget.
fn parse_strict_positive_int(raw: &str) -> Option<usize> {
    if raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let value: f64 = raw.parse().ok()?;
    if value.is_finite() && value >= 1.0 {
        // `as` saturates (Rust 1.45+): a huge budget clamps to
        // usize::MAX, which drops nothing — the reference's Number
        // behaves the same way at every real store size.
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "saturating float-to-int cast; value >= 1.0 was checked"
        )]
        Some(value as usize)
    } else {
        None
    }
}

/// A JS-number JSON value: integer-valued floats serialize without
/// the decimal point (`JSON.stringify(0)` is `0`, not `0.0`).
fn js_number(value: f64) -> Value {
    if value.fract() == 0.0 && value.abs() < 9.007_199_254_740_992e15 {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the bound check keeps the integer exact"
        )]
        Value::from(value as i64)
    } else {
        Value::from(value)
    }
}

/// The json/plain error surface of the guard clauses (the handled
/// failure's envelope already carries the unprefixed sentence).
fn prime_error(_json_mode: bool, message: &str) -> Failure {
    Failure::handled_on_stderr("prime", format!("Error: {message}"))
}

/// prime's unknown-domain surfaces: the capital-D sentence (json)
/// or the two-line hint (plain), `Excluded domain` for exclusions.
/// The hint promotes THIS binary (user directive 2026-10-13).
fn unknown_domain_failure(
    json_mode: bool,
    domain: &str,
    available: &[String],
    excluded: bool,
) -> Failure {
    let list = if available.is_empty() {
        "(none)".to_string()
    } else {
        available.join(", ")
    };
    let one_line = if excluded {
        format!("Excluded domain \"{domain}\" not found in config. Available domains: {list}")
    } else {
        format!("Domain \"{domain}\" not found in config. Available domains: {list}")
    };
    let mut fields = Map::new();
    fields.insert("error".into(), Value::String(one_line.clone()));
    let mut failure = Failure::handled_on_stderr("prime", format!("Error: {one_line}"));
    failure.envelope = crate::output::failure_envelope("prime", fields);
    if !json_mode {
        failure.message = format!(
            "Error: {one_line}\nHint: Run `mulch add {domain}` to create this domain, or check .mulch/mulch.config.yaml"
        );
    }
    failure
}

/// A domain file's mtime (`getFileModTime`; `None` when absent).
fn modified(store: &StoreFiles, domain: &str) -> Result<Option<Timestamp>, Failure> {
    match store.domain_modified(domain) {
        Ok(Some(time)) => Timestamp::try_from(time)
            .map(Some)
            .map_err(|source| Failure::handled_on_stderr("prime", format!("Error: {source}"))),
        Ok(None) => Ok(None),
        Err(source) => Err(Failure::handled_on_stderr(
            "prime",
            format!("Error: {source}"),
        )),
    }
}

/// `computeTypeCounts`: first-seen type order (a missing type
/// counts under the literal `undefined` key).
fn type_counts(records: &[Value]) -> Vec<(String, u64)> {
    let mut counts = mulch::Counter::seeded(&[]);
    for record in records {
        counts.bump_field(record.get("type"));
    }
    match counts.to_distribution() {
        Value::Object(map) => map
            .into_iter()
            .filter_map(|(name, count)| count.as_u64().map(|count| (name, count)))
            .collect(),
        _ => Vec::new(),
    }
}

/// The governance thresholds with the reference defaults backfilled.
fn effective_governance(config: &Config) -> Governance {
    config.effective_governance()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_int_matches_the_reference_parse() {
        assert_eq!(parse_strict_positive_int("1"), Some(1));
        assert_eq!(parse_strict_positive_int("007"), Some(7));
        assert_eq!(
            parse_strict_positive_int("999999999999999999999999"),
            Some(usize::MAX)
        );
        assert_eq!(parse_strict_positive_int("0"), None);
        assert_eq!(parse_strict_positive_int("3.7"), None);
        assert_eq!(parse_strict_positive_int("10abc"), None);
        assert_eq!(parse_strict_positive_int(""), None);
    }

    #[test]
    fn auto_flip_is_strict_greater() {
        assert!(!should_auto_flip_to_manifest(100, 5));
        assert!(should_auto_flip_to_manifest(101, 1));
        assert!(should_auto_flip_to_manifest(1, 6));
    }
}
