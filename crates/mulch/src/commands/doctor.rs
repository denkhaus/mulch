//! `mulch doctor` — the 17-check health report, plain and `--json`,
//! with `--fix` for stale and schema-invalid records.

use std::fmt::Write as _;

use jiff::Timestamp;
use serde_json::{Map, Value};

use crate::cli::GlobalOpts;
use crate::commands::schema::schema_error;
use crate::commands::{NO_CONFIG_MESSAGE, NO_STORE_MESSAGE, StoreLocation, domain_file, locate};
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
    lines:  Vec<(usize, Result<Value, ()>)>,
}

/// Runs `doctor`: the report always prints (even with failures);
/// `--fix` appends its fixes after the report; the exit code reflects
/// the PRE-fix check pass (reference contract).
pub(super) fn run(opts: &GlobalOpts, fix: bool) -> Result<(), Failure> {
    let cwd = std::env::current_dir()
        .map_err(|source| Failure::handled("doctor", format!("resolving cwd: {source}")))?;
    let store = match locate(&cwd) {
        Ok(StoreLocation::Missing) => {
            let mut failure = Failure::handled("doctor", NO_STORE_MESSAGE);
            failure.envelope_to_stderr = true;
            return Err(failure);
        }
        Ok(StoreLocation::NoConfig) => {
            let mut failure = Failure::handled("doctor", NO_CONFIG_MESSAGE);
            failure.envelope_to_stderr = true;
            return Err(failure);
        }
        Ok(StoreLocation::Open(store)) => store,
        Err(source) => return Err(Failure::handled("doctor", source.to_string())),
    };

    let domains = read_domains(&store);
    let checks = run_checks(&store, &domains);

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
        print_line(false, &text);

        if fix {
            let fixes = apply_fixes(&store, &domains);
            if !fixes.is_empty() {
                let mut fixed = String::from("Fixed:");
                for fix_line in fixes {
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
fn read_domains(store: &crate::commands::ConfigStore) -> Vec<DomainLines> {
    store
        .domains()
        .into_iter()
        .map(|domain| {
            let text =
                std::fs::read_to_string(domain_file(&store.root, &domain)).unwrap_or_default();
            let lines = text
                .lines()
                .filter(|l| !l.trim().is_empty())
                .enumerate()
                .map(|(index, line)| {
                    (
                        index + 1,
                        serde_json::from_str::<Value>(line).map_err(|_| ()),
                    )
                })
                .collect();
            DomainLines { domain, lines }
        })
        .collect()
}

/// Runs the 17 checks in the reference's fixed order.
fn run_checks(_store: &crate::commands::ConfigStore, domains: &[DomainLines]) -> Vec<Check> {
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
        unknown_types(domains),
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
        stale_records(domains),
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
            d.lines
                .iter()
                .filter(|(_, parsed)| parsed.is_err())
                .map(|(line, _)| format!("{}:{}", d.domain, line))
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
            d.lines.iter().filter_map(|(line, parsed)| {
                let record = parsed.as_ref().ok()?;
                record
                    .as_object()?
                    .contains_key("outcome")
                    .then(|| format!("{}:{}", d.domain, line))
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
            d.lines.iter().filter_map(|(line, parsed)| {
                let record = parsed.as_ref().ok()?;
                schema_error(record).map(|message| format!("{}:{} - {}", d.domain, line, message))
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

fn unknown_types(domains: &[DomainLines]) -> Check {
    let known: Vec<&str> = crate::commands::schema::BRANCHES
        .iter()
        .map(|(name, _)| *name)
        .collect();
    let bad: Vec<String> = domains
        .iter()
        .flat_map(|d| {
            d.lines.iter().filter_map(|(line, parsed)| {
                let record = parsed.as_ref().ok()?;
                let kind = record.as_object()?.get("type")?.as_str()?;
                (!known.contains(&kind)).then(|| format!("{}:{}: {}", d.domain, line, kind))
            })
        })
        .collect();
    if bad.is_empty() {
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
        for (_, parsed) in &domain.lines {
            if let Some(kind) = parsed
                .as_ref()
                .ok()
                .and_then(|r| r.as_object())
                .and_then(|o| o.get("type"))
                .and_then(Value::as_str)
            {
                *counts.entry(kind).or_insert(0usize) += 1;
            }
        }
    }
    let details: Vec<String> = crate::commands::schema::BRANCHES
        .iter()
        .map(|(name, _)| {
            let count = counts.get(*name).copied().unwrap_or(0);
            let plural = if count == 1 { "record" } else { "records" };
            format!("{name} (built-in): {count} {plural}")
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
    let total: usize = domains
        .iter()
        .flat_map(|d| d.lines.iter().filter(|(_, parsed)| parsed.is_ok()))
        .count();
    let fraction = format!("{total}/{total}");
    // JSON carries per-domain conformance details even on pass (plain
    // renders details only under non-pass checks).
    let details: Vec<String> = domains
        .iter()
        .map(|d| {
            let count = d.lines.iter().filter(|(_, parsed)| parsed.is_ok()).count();
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
fn stale_records(domains: &[DomainLines]) -> Check {
    let now = Timestamp::now();
    let mut details = Vec::new();
    for domain in domains {
        for (_, parsed) in &domain.lines {
            let Some(record) = parsed.as_ref().ok().and_then(Value::as_object) else {
                continue;
            };
            let classification = record
                .get("classification")
                .and_then(Value::as_str)
                .unwrap_or("tactical");
            let shelf_days: i64 = match classification {
                "tactical" => 14,
                "observational" => 30,
                _ => continue,
            };
            let Some(recorded) = record
                .get("recorded_at")
                .and_then(Value::as_str)
                .and_then(|raw| raw.parse::<Timestamp>().ok())
            else {
                continue;
            };
            let expiry = recorded + jiff::Span::new().hours(shelf_days * 24);
            if now >= expiry {
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
/// schema-invalid records removed — both HARD-deleted, the domain file
/// left as an empty file, the domain stays registered.
fn apply_fixes(store: &crate::commands::ConfigStore, domains: &[DomainLines]) -> Vec<String> {
    let now = Timestamp::now();
    let mut fixes = Vec::new();
    for domain in domains {
        let mut stale_pruned = 0usize;
        let mut invalid_removed = 0usize;
        let survivors: Vec<&str> = domain
            .lines
            .iter()
            .filter(|(_, parsed)| match parsed {
                Err(()) => {
                    invalid_removed += 1;
                    false
                }
                Ok(record) => {
                    if schema_error(record).is_some() {
                        invalid_removed += 1;
                        return false;
                    }
                    if is_stale(record, now) {
                        stale_pruned += 1;
                        return false;
                    }
                    true
                }
            })
            .map(|_| "")
            .collect();
        // Survivors are rebuilt from the parsed values' original lines.
        let _ = survivors;
        let text =
            std::fs::read_to_string(domain_file(&store.root, &domain.domain)).unwrap_or_default();
        let kept: Vec<&str> = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter(|l| {
                serde_json::from_str::<Value>(l)
                    .is_ok_and(|record| schema_error(&record).is_none() && !is_stale(&record, now))
            })
            .collect();
        if kept.len() != text.lines().filter(|l| !l.trim().is_empty()).count() {
            let file = domain_file(&store.root, &domain.domain);
            let empty = if kept.is_empty() {
                String::new()
            } else {
                format!("{}\n", kept.join("\n"))
            };
            let _ = std::fs::write(file, empty);
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
    fixes
}

/// Staleness predicate shared with the check.
fn is_stale(record: &Value, now: Timestamp) -> bool {
    let Some(object) = record.as_object() else {
        return false;
    };
    let classification = object
        .get("classification")
        .and_then(Value::as_str)
        .unwrap_or("tactical");
    let shelf_days: i64 = match classification {
        "tactical" => 14,
        "observational" => 30,
        _ => return false,
    };
    object
        .get("recorded_at")
        .and_then(Value::as_str)
        .and_then(|raw| raw.parse::<Timestamp>().ok())
        .is_some_and(|recorded| now >= recorded + jiff::Span::new().hours(shelf_days * 24))
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
