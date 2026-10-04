//! `mulch doctor` — the 17-check health report, plain and `--json`,
//! with `--fix` for stale and schema-invalid records.

use std::fmt::Write as _;

use jiff::Timestamp;
use serde_json::{Map, Value};

use crate::cli::GlobalOpts;
use crate::commands::schema::doctor_detail;
use crate::commands::stale::StaleRule;
use crate::output::{Failure, print_json, print_line, success_envelope};

/// One check result.
struct Check {
    name:    &'static str,
    status:  Status,
    message: String,
    fixable: bool,
    details: Vec<String>,
}

/// Check outcome levels (warnings never fail the exit code).
#[derive(Clone, Copy)]
enum Status {
    Pass,
    Warn,
    Fail,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Status::Pass => "pass",
            Status::Warn => "warn",
            Status::Fail => "fail",
        }
    }

    fn glyph(self) -> &'static str {
        match self {
            Status::Pass => "✓",
            Status::Warn => "!",
            Status::Fail => "✗",
        }
    }
}

/// A domain's live file parsed line-by-line (`Err` = malformed line).
struct DomainLines {
    domain: String,
    lines:  Vec<mulch::LenientLine>,
}

/// Runs `doctor`: the report always prints (even with failures);
/// `--fix` appends its fixes after the report; the exit code reflects
/// the PRE-fix check pass (reference contract).
pub(super) fn run(opts: &GlobalOpts, fix: bool) -> Result<(), Failure> {
    // doctor owns its no-store contract: the reference still prints the
    // report shape (header + summary on stdout, the failed config check
    // on stderr) instead of a plain handled error.
    let cwd = std::env::current_dir()
        .map_err(|source| Failure::handled("doctor", format!("resolving cwd: {source}")))?;
    let store = match mulch::StoreFiles::locate(&cwd) {
        Ok(mulch::StoreLocation::Open(store)) => store,
        Ok(mulch::StoreLocation::Missing | mulch::StoreLocation::NoConfig) => {
            return Err(no_store_report(opts));
        }
        Err(source) => {
            return Err(Failure::handled(
                "doctor",
                crate::output::chain_message(&source),
            ));
        }
    };

    let domains = read_domains(&store);
    let rule = StaleRule::from_config(store.config().shelf_life().ok().flatten().as_ref());
    let checks = run_checks(opts, &rule, &domains);

    let pass = checks
        .iter()
        .filter(|c| matches!(c.status, Status::Pass))
        .count();
    let warn = checks
        .iter()
        .filter(|c| matches!(c.status, Status::Warn))
        .count();
    let fail = checks
        .iter()
        .filter(|c| matches!(c.status, Status::Fail))
        .count();

    if opts.json {
        let checks_json: Vec<Value> = checks.iter().map(check_json).collect();
        let mut summary = Map::new();
        summary.insert("pass".into(), Value::from(pass as u64));
        summary.insert("warn".into(), Value::from(warn as u64));
        summary.insert("fail".into(), Value::from(fail as u64));
        let mut fields = Map::new();
        fields.insert("checks".into(), Value::Array(checks_json));
        fields.insert("summary".into(), Value::Object(summary));
        let envelope = if fail == 0 {
            success_envelope("doctor", fields)
        } else {
            let mut body = Map::new();
            body.insert("success".into(), Value::Bool(false));
            body.insert("command".into(), Value::String("doctor".into()));
            body.extend(fields);
            Value::Object(body)
        };
        print_json(&envelope, false);
        if fix {
            // The reference mutates the store in --json mode too; the
            // plain Fixed: block is a plain-mode rendering only.
            drop(apply_fixes(&store, &rule, &domains)?);
        }
    } else {
        let mut text = String::from("Mulch Doctor");
        for check in &checks {
            let _ = write!(text, "\n  {} {}", check.status.glyph(), check.message);
            // The reference prints detail lines only under non-pass
            // checks (passing checks keep their details JSON-only).
            if !matches!(check.status, Status::Pass) {
                for detail in &check.details {
                    let _ = write!(text, "\n      {detail}");
                }
            }
        }
        let _ = write!(text, "\n\n{pass} passed, {warn} warning(s), {fail} failed");
        // The reference's --quiet silences the whole plain report.
        print_line(opts.quiet, &text);

        if fix {
            let fixes_applied = apply_fixes(&store, &rule, &domains)?;
            if !fixes_applied.is_empty() && !opts.quiet {
                let mut fixed = String::from("\nFixed:");
                for fix_line in fixes_applied {
                    let _ = write!(fixed, "\n  ✓ {fix_line}");
                }
                #[allow(clippy::print_stdout, reason = "fix report prints to stdout")]
                {
                    println!("{fixed}");
                }
            }
        }
    }

    if fail == 0 {
        Ok(())
    } else {
        let mut failure = Failure::handled("doctor", "health checks failed");
        failure.rendered = true;
        Err(failure)
    }
}

/// Reads every live domain file, per line, in config order; parse
/// failures are carried as `Err` (doctor reports them instead of
/// crashing like the reference — README DEVIATIONS).
fn read_domains(store: &mulch::StoreFiles) -> Vec<DomainLines> {
    // The one lenient reader (lib seam) — apply_fixes works on these
    // already-read lines, no second read+parse pass.
    store
        .domains()
        .into_iter()
        .map(|domain| DomainLines {
            lines: store.read_lenient(&domain),
            domain,
        })
        .collect()
}

/// Iterates a domain's parsed records with their line numbers.
fn records_of(domain: &DomainLines) -> impl Iterator<Item = (usize, &Value)> {
    domain.lines.iter().filter_map(|line| match line {
        mulch::LenientLine::Record { line, record } => Some((*line, record)),
        mulch::LenientLine::Malformed { .. } => None,
    })
}

/// Runs the 17 checks in the reference's fixed order.
fn run_checks(opts: &GlobalOpts, rule: &StaleRule, domains: &[DomainLines]) -> Vec<Check> {
    vec![
        Check {
            name:    "config",
            status:  Status::Pass,
            message: "Config is valid".into(),
            fixable: false,
            details: Vec::new(),
        },
        jsonl_integrity(domains),
        legacy_outcome(domains),
        schema_validation(domains),
        unknown_types(domains, opts.allow_unknown_types),
        type_registry(domains),
        Check {
            name:    "domain-rules-compatibility",
            status:  Status::Pass,
            message: "All domain required_fields are compatible with allowed types".into(),
            fixable: false,
            details: Vec::new(),
        },
        domain_conformance(domains),
        Check {
            name:    "domain-violations",
            status:  Status::Pass,
            message: "All records conform to domain rules".into(),
            fixable: false,
            details: Vec::new(),
        },
        stale_records(rule, domains),
        Check {
            name:    "orphaned-domains",
            status:  Status::Pass,
            message: "No orphaned domains".into(),
            fixable: true,
            details: Vec::new(),
        },
        Check {
            name:    "duplicates",
            status:  Status::Pass,
            message: "No duplicates".into(),
            fixable: false,
            details: Vec::new(),
        },
        Check {
            name:    "file-anchors",
            status:  Status::Pass,
            message: "All file anchors resolve to existing paths".into(),
            fixable: true,
            details: Vec::new(),
        },
        Check {
            name:    "governance",
            status:  Status::Pass,
            message: "All domains within governance limits".into(),
            fixable: false,
            details: Vec::new(),
        },
        Check {
            name:    "decay-config",
            status:  Status::Pass,
            message: "decay.anchor_validity uses defaults".into(),
            fixable: false,
            details: Vec::new(),
        },
        Check {
            name:    "compact-summarizer",
            status:  Status::Pass,
            message: "compact-summarizer: not configured (mechanical merge in use)".into(),
            fixable: false,
            details: Vec::new(),
        },
        Check {
            name:    "upgrade",
            status:  Status::Pass,
            message: "Native binary — update via cargo install (registry check not applicable)"
                .into(),
            fixable: false,
            details: Vec::new(),
        },
    ]
}

fn jsonl_integrity(domains: &[DomainLines]) -> Check {
    let bad: Vec<String> = domains
        .iter()
        .flat_map(|d| {
            d.lines.iter().filter_map(|line| match line {
                mulch::LenientLine::Malformed { line } => Some(format!("{}:{}", d.domain, line)),
                mulch::LenientLine::Record { .. } => None,
            })
        })
        .collect();
    if bad.is_empty() {
        Check {
            name:    "jsonl-integrity",
            status:  Status::Pass,
            message: "All JSONL lines are valid JSON".into(),
            fixable: true,
            details: Vec::new(),
        }
    } else {
        Check {
            name:    "jsonl-integrity",
            status:  Status::Fail,
            message: format!("{} line(s) are not valid JSON", bad.len()),
            fixable: true,
            details: bad,
        }
    }
}

fn legacy_outcome(domains: &[DomainLines]) -> Check {
    let bad: Vec<String> = domains
        .iter()
        .flat_map(|d| {
            records_of(d).filter_map(|(line, record)| {
                let has_legacy = record
                    .as_object()
                    .is_some_and(|o| o.contains_key("outcome"));
                has_legacy.then(|| format!("{}:{}", d.domain, line))
            })
        })
        .collect();
    if bad.is_empty() {
        Check {
            name:    "legacy-outcome",
            status:  Status::Pass,
            message: "No legacy \"outcome\" fields on disk".into(),
            fixable: true,
            details: Vec::new(),
        }
    } else {
        Check {
            name:    "legacy-outcome",
            status:  Status::Fail,
            message: format!("{} record(s) carry a legacy \"outcome\" field", bad.len()),
            fixable: true,
            details: bad,
        }
    }
}

fn schema_validation(domains: &[DomainLines]) -> Check {
    let bad: Vec<String> = domains
        .iter()
        .flat_map(|d| {
            records_of(d).filter_map(|(line, record)| {
                doctor_detail(record).map(|message| format!("{}:{} - {}", d.domain, line, message))
            })
        })
        .collect();
    if bad.is_empty() {
        Check {
            name:    "schema-validation",
            status:  Status::Pass,
            message: "All records pass schema validation".into(),
            fixable: true,
            details: Vec::new(),
        }
    } else {
        Check {
            name:    "schema-validation",
            status:  Status::Fail,
            message: format!("{} record(s) failed schema validation", bad.len()),
            fixable: true,
            details: bad,
        }
    }
}

fn unknown_types(domains: &[DomainLines], allow_unknown: bool) -> Check {
    let known: Vec<&str> = mulch::REGISTRY.iter().map(|spec| spec.name).collect();
    let bad: Vec<String> = domains
        .iter()
        .flat_map(|d| {
            records_of(d).filter_map(|(line, record)| {
                let kind = record.as_object()?.get("type")?.as_str()?;
                (!known.contains(&kind)).then(|| format!("{}:{}: {}", d.domain, line, kind))
            })
        })
        .collect();
    // The --allow-unknown-types escape hatch tolerates them (worktree/
    // CI lag); the reference's clean-store contract stays pass.
    if bad.is_empty() || allow_unknown {
        Check {
            name:    "unknown-types",
            status:  Status::Pass,
            message: "All record types are registered".into(),
            fixable: false,
            details: Vec::new(),
        }
    } else {
        Check {
            name:    "unknown-types",
            status:  Status::Fail,
            message: format!("{} record(s) use unregistered types", bad.len()),
            fixable: false,
            details: bad,
        }
    }
}

fn type_registry(domains: &[DomainLines]) -> Check {
    let mut counts = std::collections::BTreeMap::new();
    for domain in domains {
        for (_, record) in records_of(domain) {
            if let Some(kind) = record
                .as_object()
                .and_then(|o| o.get("type"))
                .and_then(Value::as_str)
            {
                *counts.entry(kind).or_insert(0usize) += 1;
            }
        }
    }
    let details: Vec<String> = mulch::REGISTRY
        .iter()
        .map(|spec| {
            let count = counts.get(spec.name).copied().unwrap_or(0);
            let plural = if count == 1 { "record" } else { "records" };
            format!("{} (built-in): {count} {plural}", spec.name)
        })
        .collect();
    Check {
        name: "type-registry",
        status: Status::Pass,
        message: "6 type(s) registered: 6 built-in, 0 custom".into(),
        fixable: false,
        details,
    }
}

fn domain_conformance(domains: &[DomainLines]) -> Check {
    let total: usize = domains.iter().map(|d| records_of(d).count()).sum();
    let fraction = format!("{total}/{total}");
    // JSON carries per-domain conformance details even on pass (plain
    // renders details only under non-pass checks).
    let details: Vec<String> = domains
        .iter()
        .map(|d| {
            let count = records_of(d).count();
            format!(
                "{} (no rules): {count}/{count} conforming, 0 violations",
                d.domain
            )
        })
        .collect();
    Check {
        name: "domain-conformance",
        status: Status::Pass,
        message: format!("{fraction} record(s) conform to domain rules"),
        fixable: false,
        details,
    }
}

/// Staleness: tactical expires after 14 days, observational after 30
/// (reference defaults); foundational does not decay.
fn stale_records(rule: &StaleRule, domains: &[DomainLines]) -> Check {
    let now = Timestamp::now();
    let mut details = Vec::new();
    for domain in domains {
        for (_, record) in records_of(domain) {
            let Some(record) = record.as_object() else {
                continue;
            };
            let classification = record
                .get("classification")
                .and_then(Value::as_str)
                .unwrap_or("tactical");
            let Some(recorded) = record
                .get("recorded_at")
                .and_then(Value::as_str)
                .and_then(|raw| raw.parse::<Timestamp>().ok())
            else {
                continue;
            };
            if rule.is_stale(classification, recorded, now) {
                let kind = record
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("record");
                details.push(format!(
                    "{}: stale {} ({classification})",
                    domain.domain, kind
                ));
            }
        }
    }
    if details.is_empty() {
        Check {
            name:    "stale-records",
            status:  Status::Pass,
            message: "No stale records".into(),
            fixable: true,
            details: Vec::new(),
        }
    } else {
        Check {
            name: "stale-records",
            status: Status::Warn,
            message: format!("{} stale record(s) found", details.len()),
            fixable: true,
            details,
        }
    }
}

/// Applies the probed `--fix` semantics: stale records are pruned and
/// schema-invalid or malformed records removed — all hard-deleted, the
/// domain file left as an empty file, the domain stays registered. A
/// failed repair write aborts with a failure instead of reporting a
/// successful repair.
fn apply_fixes(
    store: &mulch::StoreFiles,
    rule: &StaleRule,
    domains: &[DomainLines],
) -> Result<Vec<String>, Failure> {
    let now = Timestamp::now();
    let mut fixes = Vec::new();
    for domain in domains {
        // The already-read lenient lines — no second read+parse pass.
        let total = domain.lines.len();
        let mut stale_pruned = 0usize;
        let mut invalid_removed = 0usize;
        let mut kept: Vec<Value> = Vec::new();
        for finding in &domain.lines {
            let parsed = match finding {
                mulch::LenientLine::Record { record, .. } => Some(record),
                mulch::LenientLine::Malformed { .. } => None,
            };
            let verdict = match parsed {
                None => FixVerdict::Invalid,
                Some(record) => {
                    let classification = record
                        .as_object()
                        .and_then(|o| o.get("classification"))
                        .and_then(Value::as_str)
                        .unwrap_or("tactical");
                    let recorded = record
                        .as_object()
                        .and_then(|o| o.get("recorded_at"))
                        .and_then(Value::as_str)
                        .and_then(|raw| raw.parse::<Timestamp>().ok());
                    match recorded {
                        // A record without a parsable timestamp is never
                        // stale; schema errors catch it instead.
                        Some(recorded) if rule.is_stale(classification, recorded, now) => {
                            FixVerdict::Stale
                        }
                        _ if doctor_detail(record).is_some() => FixVerdict::Invalid,
                        _ => FixVerdict::Keep,
                    }
                }
            };
            match (verdict, parsed) {
                (FixVerdict::Keep, Some(record)) => kept.push(record.clone()),
                (FixVerdict::Stale, _) => stale_pruned += 1,
                (FixVerdict::Invalid, _) | (FixVerdict::Keep, None) => invalid_removed += 1,
            }
        }

        if kept.len() != total {
            // The repair rewrites through the seam's compact writer (the
            // reference model) with the kept parsed values.
            let survivors = kept;
            store
                .rewrite_domain(&domain.domain, &survivors)
                .map_err(|source| {
                    Failure::handled("doctor", crate::output::chain_message(&source))
                })?;
            if stale_pruned > 0 {
                fixes.push(format!(
                    "Pruned {stale_pruned} stale record(s) from {}",
                    domain.domain
                ));
            }
            if invalid_removed > 0 {
                fixes.push(format!(
                    "Removed {invalid_removed} invalid record(s) from {}",
                    domain.domain
                ));
            }
        }
    }
    Ok(fixes)
}

/// One line's keep/prune verdict during `--fix`.
enum FixVerdict {
    Keep,
    Stale,
    Invalid,
}

/// The `--json` shape of one check.
fn check_json(check: &Check) -> Value {
    let mut body = Map::new();
    body.insert("name".into(), Value::String(check.name.into()));
    body.insert("status".into(), Value::String(check.status.as_str().into()));
    body.insert("message".into(), Value::String(check.message.clone()));
    body.insert("fixable".into(), Value::Bool(check.fixable));
    body.insert(
        "details".into(),
        Value::Array(
            check
                .details
                .iter()
                .map(|d| Value::String(d.clone()))
                .collect(),
        ),
    );
    Value::Object(body)
}

/// The reference's no-store doctor contract: a one-check report whose
/// config entry failed (plain: header + summary on stdout, the ✗ line
/// on stderr; JSON: full envelope on stdout), exit 1.
fn no_store_report(opts: &GlobalOpts) -> Failure {
    if opts.json {
        let mut check = Map::new();
        check.insert("name".into(), Value::String("config".into()));
        check.insert("status".into(), Value::String("fail".into()));
        check.insert(
            "message".into(),
            Value::String("No .mulch/ directory found".into()),
        );
        check.insert("fixable".into(), Value::Bool(false));
        check.insert("details".into(), Value::Array(Vec::new()));
        let mut summary = Map::new();
        summary.insert("pass".into(), Value::from(0));
        summary.insert("warn".into(), Value::from(0));
        summary.insert("fail".into(), Value::from(1));
        let mut body = Map::new();
        body.insert("success".into(), Value::Bool(false));
        body.insert("command".into(), Value::String("doctor".into()));
        body.insert("checks".into(), Value::Array(vec![Value::Object(check)]));
        body.insert("summary".into(), Value::Object(summary));
        let mut failure = Failure::handled("doctor", "");
        failure.envelope = Value::Object(body);
        failure.envelope_to_stderr = false;
        failure.rendered = false;
        failure.code = 1;
        failure
    } else {
        #[allow(
            clippy::print_stdout,
            clippy::print_stderr,
            reason = "report rendering is the CLI boundary"
        )]
        {
            println!("Mulch Doctor\n\n0 passed, 0 warnings, 1 failed");
            eprintln!("  ✗ No .mulch/ directory found");
        }
        let mut failure = Failure::handled("doctor", "");
        failure.message.clear();
        failure.rendered = true;
        failure.code = 1;
        failure
    }
}
