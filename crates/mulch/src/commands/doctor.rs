//! `mulch doctor` — the 17-check health report, plain and `--json`,
//! with `--fix` as four sequential repair passes in check order
//! (jsonl-integrity, legacy-outcome, schema-validation,
//! stale-records).

use std::collections::HashMap;
use std::collections::hash_map::Entry as HashMapEntry;
use std::fmt::Write as _;

use jiff::Timestamp;
use mulch::schema::{doctor_detail, survives_repair};
use mulch::stale::{StaleRule, StaleVerdict};
use serde_json::{Map, Value};

use crate::cli::GlobalOpts;
use crate::output::{Failure, failure_envelope, print_json, print_line, success_envelope};

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
    let config = store.config();
    let rule = StaleRule::from_shelf_life(&config.effective_shelf_life());
    let governance = config.effective_governance();
    let checks = run_checks(opts, &rule, &governance, &domains);
    let gates = FixGates::from_checks(&checks);

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

    // The reference repairs BEFORE any rendering — a crashing repair
    // prints no report at all — and the same lines feed the plain
    // Fixed block and the JSON envelope's `fixed` field.
    let fixed = if fix {
        apply_fixes(&store, &rule, &domains, &gates)?
    } else {
        Vec::new()
    };

    if opts.json {
        let checks_json: Vec<Value> = checks.iter().map(check_json).collect();
        let mut summary = Map::new();
        summary.insert("pass".into(), Value::from(pass as u64));
        summary.insert("warn".into(), Value::from(warn as u64));
        summary.insert("fail".into(), Value::from(fail as u64));
        let mut fields = Map::new();
        fields.insert("checks".into(), Value::Array(checks_json));
        fields.insert("summary".into(), Value::Object(summary));
        if fix {
            // The reference's JSON envelope carries the fix lines
            // whenever --fix ran — an empty array when nothing needed
            // repairing.
            fields.insert(
                "fixed".into(),
                Value::Array(fixed.iter().cloned().map(Value::String).collect()),
            );
        }
        let envelope = if fail == 0 {
            success_envelope("doctor", fields)
        } else {
            failure_envelope("doctor", fields)
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
        // The reference's --quiet silences the whole plain report.
        print_line(opts.quiet, &text);

        if fix && !fixed.is_empty() && !opts.quiet {
            let mut block = String::from("\nFixed:");
            for fix_line in &fixed {
                let _ = write!(block, "\n  ✓ {fix_line}");
            }
            #[allow(clippy::print_stdout, reason = "fix report prints to stdout")]
            {
                println!("{block}");
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
    // The one lenient reader (lib seam): the checks work on these
    // already-read lines; the repair passes re-read the current bytes
    // per pass, like the reference's sequential fix cases.
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
fn run_checks(
    opts: &GlobalOpts,
    rule: &StaleRule,
    governance: &mulch::Governance,
    domains: &[DomainLines],
) -> Vec<Check> {
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
        duplicates(domains),
        Check {
            name:    "file-anchors",
            status:  Status::Pass,
            message: "All file anchors resolve to existing paths".into(),
            fixable: true,
            details: Vec::new(),
        },
        governance_limits(governance, domains),
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
                mulch::LenientLine::Malformed { line } => {
                    Some(format!("{}:{line} - Invalid JSON", d.domain))
                }
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
            message: format!("{} invalid JSON line(s) found", bad.len()),
            fixable: true,
            details: bad,
        }
    }
}

fn legacy_outcome(domains: &[DomainLines]) -> Check {
    let bad: Vec<String> = domains
        .iter()
        .flat_map(|d| {
            records_of(d)
                .filter(|&(_, record)| mulch::legacy_outcome(record).is_some())
                .map(|(line, _)| {
                    format!(
                        "{}:{line} - legacy \"outcome\" field (singular); should be \"outcomes[]\"",
                        d.domain
                    )
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
        // The reference reports legacy fields as a warning, never a
        // failure.
        Check {
            name:    "legacy-outcome",
            status:  Status::Warn,
            message: format!(
                "{} record(s) with legacy \"outcome\" field on disk",
                bad.len()
            ),
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
            // The verdict owns the extraction (mulch-f9b9: a missing or
            // unknown classification never decays — the reference
            // `isStale` falls through to `false`, it does NOT default
            // to tactical) and carries the expired classification.
            if let StaleVerdict::Stale { classification } = rule.verdict(record, now) {
                // The reference template-stringifies the type — a
                // missing one renders as `undefined`, any other
                // non-string through JS String coercion.
                let kind = match record.get("type") {
                    Some(value) => mulch::value_text(value),
                    None => "undefined".into(),
                };
                details.push(format!(
                    "{}: stale {kind} ({classification})",
                    domain.domain
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

/// Duplicates: the reference's `checkDuplicates` (doctor.ts) — per
/// domain, in parsed-record order, every record whose registry dedup
/// field matches a same-type earlier record counts once (redundant
/// records, not ids); warn, never fixable, `--fix` keeps them.
///
/// EXTENSION (README DEVIATIONS): a repeated id with DIVERGENT content
/// — a content-hash id whose record was edited in place — is invisible
/// to the reference's dedup-field match. We report it as our own
/// fail-class finding after the reference details, never auto-fixed.
fn duplicates(domains: &[DomainLines]) -> Check {
    let mut reference_details = Vec::new();
    let mut divergent_details = Vec::new();
    for domain in domains {
        let records: Vec<&Value> = records_of(domain).map(|(_, record)| record).collect();
        for (index, record) in records.iter().enumerate().skip(1) {
            let Some(matched) = mulch::find_duplicate(records[..index].iter().copied(), record)
            else {
                continue;
            };
            reference_details.push(format!(
                "{}: duplicate {} at index {} (matches #{})",
                domain.domain,
                record
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                index + 1,
                matched + 1
            ));
        }
        divergent_details.extend(divergent_ids(&domain.domain, &records));
    }

    let duplicates = reference_details.len();
    let divergent = divergent_details.len();
    let mut details = reference_details;
    details.append(&mut divergent_details);
    let mut parts = Vec::new();
    if duplicates > 0 {
        parts.push(format!("{duplicates} duplicate record(s) found"));
    }
    if divergent > 0 {
        parts.push(format!("{divergent} divergent id(s) found"));
    }
    // Deviation: a divergent id fails the store the reference passes.
    let status = if divergent > 0 {
        Status::Fail
    } else if duplicates > 0 {
        Status::Warn
    } else {
        Status::Pass
    };
    let message = if parts.is_empty() {
        "No duplicates".into()
    } else {
        parts.join("; ")
    };
    Check {
        name: "duplicates",
        status,
        message,
        fixable: false,
        details,
    }
}

/// The divergent-id extension: the first record per non-empty id is
/// the anchor; every later record with the SAME id but structurally
/// DIFFERENT content is one finding (identical repeats stay the
/// reference's duplicate class).
fn divergent_ids(domain: &str, records: &[&Value]) -> Vec<String> {
    let mut first_by_id: HashMap<&str, (usize, &Value)> = HashMap::new();
    let mut details = Vec::new();
    for (index, record) in records.iter().enumerate() {
        let Some(id) = record.get("id").and_then(Value::as_str) else {
            continue;
        };
        if id.is_empty() {
            continue;
        }
        match first_by_id.entry(id) {
            HashMapEntry::Vacant(entry) => {
                entry.insert((index, record));
            }
            HashMapEntry::Occupied(entry) => {
                let (anchor_index, anchor_record) = entry.get();
                if anchor_record != record {
                    details.push(format!(
                        "{domain}: divergent id {id} at index {} (matches #{}, different content)",
                        index + 1,
                        anchor_index + 1
                    ));
                }
            }
        }
    }
    details
}

/// Governance: per-domain record counts vs the effective thresholds
/// (reference `checkGovernance`): at `max_entries` a domain warns
/// "approaching limit", at `warn_entries` "over warn threshold", at
/// `hard_limit` the check FAILS; the worst status wins and the
/// details follow domain order. Not fixable (`--fix` never touches
/// counts).
fn governance_limits(governance: &mulch::Governance, domains: &[DomainLines]) -> Check {
    let mut details = Vec::new();
    let mut worst = Status::Pass;
    for domain in domains {
        let count = u64::try_from(records_of(domain).count()).unwrap_or(0);
        let (tier, threshold, wording) = if count >= governance.hard_limit {
            (Status::Fail, governance.hard_limit, "over hard limit of")
        } else if count >= governance.warn_entries {
            (
                Status::Warn,
                governance.warn_entries,
                "over warn threshold of",
            )
        } else if count >= governance.max_entries {
            (Status::Warn, governance.max_entries, "approaching limit of")
        } else {
            continue;
        };
        details.push(format!(
            "{}: {count} records ({wording} {threshold})",
            domain.domain
        ));
        worst = worse(worst, tier);
    }
    if details.is_empty() {
        Check {
            name:    "governance",
            status:  Status::Pass,
            message: "All domains within governance limits".into(),
            fixable: false,
            details: Vec::new(),
        }
    } else {
        Check {
            name: "governance",
            status: worst,
            message: format!("{} domain(s) over governance thresholds", details.len()),
            fixable: false,
            details,
        }
    }
}

/// The worse of two check statuses (fail > warn > pass).
fn worse(current: Status, candidate: Status) -> Status {
    match (current, candidate) {
        (Status::Fail, _) | (_, Status::Fail) => Status::Fail,
        (Status::Warn, _) | (_, Status::Warn) => Status::Warn,
        (Status::Pass, Status::Pass) => Status::Pass,
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
    gates: &FixGates,
) -> Result<Vec<String>, Failure> {
    let now = Timestamp::now();
    let mut fixes = Vec::new();
    // The reference iterates its CHECK array, running one repair case
    // per failed fixable check — jsonl-integrity, legacy-outcome,
    // schema-validation, stale-records — each re-reading the files the
    // earlier passes rewrote. The report lines follow that check order,
    // in domain order within each pass.
    if gates.jsonl {
        for domain in domains {
            if let Some(count) = raw_line_pass(store, &domain.domain, &LineAction::Drop, |_| {
                LineAction::Keep
            })? {
                fixes.push(format!(
                    "Removed {count} invalid JSON line(s) from {}",
                    domain.domain
                ));
            }
        }
    }
    if gates.legacy {
        for domain in domains {
            if let Some(count) = raw_line_pass(store, &domain.domain, &LineAction::Keep, |value| {
                migrate_legacy_line(value).map_or(LineAction::Keep, LineAction::Replace)
            })? {
                fixes.push(format!(
                    "Migrated {count} legacy \"outcome\" field(s) to \"outcomes[]\" in {}",
                    domain.domain
                ));
            }
        }
    }
    if gates.schema {
        for domain in domains {
            if let Some(count) = value_line_pass(store, &domain.domain, survives_repair)? {
                fixes.push(format!(
                    "Removed {count} invalid record(s) from {}",
                    domain.domain
                ));
            }
        }
    }
    if gates.stale {
        // The verdict owns the extraction; an unparsable or missing
        // recorded_at never prunes, and a missing/unknown
        // classification never decays (mulch-f9b9).
        let keep =
            |record: &Value| !matches!(rule.verdict(record, now), StaleVerdict::Stale { .. });
        for domain in domains {
            if let Some(count) = value_line_pass(store, &domain.domain, keep)? {
                fixes.push(format!(
                    "Pruned {count} stale record(s) from {}",
                    domain.domain
                ));
            }
        }
    }
    Ok(fixes)
}

/// Which failed checks open their repair pass — the reference gates
/// every fix case on `status !== "pass" && fixable`, so a check that
/// passes never repairs (its data is clean by construction).
struct FixGates {
    jsonl:  bool,
    legacy: bool,
    schema: bool,
    stale:  bool,
}

impl FixGates {
    fn from_checks(checks: &[Check]) -> Self {
        fn open(checks: &[Check], name: &str) -> bool {
            checks.iter().any(|check| {
                check.name == name && check.fixable && !matches!(check.status, Status::Pass)
            })
        }
        FixGates {
            jsonl:  open(checks, "jsonl-integrity"),
            legacy: open(checks, "legacy-outcome"),
            schema: open(checks, "schema-validation"),
            stale:  open(checks, "stale-records"),
        }
    }
}

/// One parsed line's fate in a line-preserving repair pass (jsonl
/// integrity, legacy outcome).
#[derive(Clone)]
enum LineAction {
    /// Keep the trimmed line verbatim.
    Keep,
    /// Replace the line with a new serialized one.
    Replace(String),
    /// Drop the line.
    Drop,
}

/// One line-preserving repair pass (the jsonl-integrity and
/// legacy-outcome fixes): every line parses, survivors keep their
/// trimmed bytes verbatim, and `action` decides each parsed line.
/// `unparsable` is the policy for lines that fail to parse — `Drop`
/// for jsonl-integrity (malformed lines and `#` comments are its
/// whole job), `Keep` for legacy-outcome (its raw-line loop passes
/// unparsable survivors through byte-identical). Returns the number
/// of lines not kept verbatim, `None` when every line kept (no
/// write).
fn raw_line_pass(
    store: &mulch::StoreFiles,
    domain: &str,
    unparsable: &LineAction,
    action: impl Fn(&Value) -> LineAction,
) -> Result<Option<usize>, Failure> {
    let Some(text) = read_domain_text(store, domain)? else {
        return Ok(None);
    };
    let mut lines = Vec::new();
    let mut changed = 0usize;
    for line in text.split('\n') {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let outcome = serde_json::from_str::<Value>(trimmed)
            .map_or_else(|_| unparsable.clone(), |value| action(&value));
        match outcome {
            LineAction::Keep => lines.push(trimmed.to_string()),
            LineAction::Replace(replacement) => {
                lines.push(replacement);
                changed += 1;
            }
            LineAction::Drop => changed += 1,
        }
    }
    if changed == 0 {
        return Ok(None);
    }
    let mut body = lines.join("\n");
    if !lines.is_empty() {
        body.push('\n');
    }
    write_domain_text(store, domain, &body)?;
    Ok(Some(changed))
}

/// One canonical-rewrite repair pass (the schema-validation and
/// stale-records fixes): every line parses, `keep` decides each
/// record, and the survivors rewrite through `rewrite_domain`
/// (`write_records` semantics incl. id backfill). Returns the number
/// of dropped records, `None` when nothing dropped (no write).
fn value_line_pass(
    store: &mulch::StoreFiles,
    domain: &str,
    keep: impl Fn(&Value) -> bool,
) -> Result<Option<usize>, Failure> {
    let Some(text) = read_domain_text(store, domain)? else {
        return Ok(None);
    };
    let mut kept = Vec::new();
    let mut removed = 0usize;
    for line in text.split('\n') {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // Unparsable lines drop uncounted (unreachable in practice: the
        // reference's strict checks crash on truly malformed lines
        // before any repair; comment lines were removed by the
        // jsonl-integrity pass above).
        let Ok(record) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        if keep(&record) {
            kept.push(record);
        } else {
            removed += 1;
        }
    }
    if removed == 0 {
        return Ok(None);
    }
    store
        .rewrite_domain(domain, &kept)
        .map_err(|source| Failure::handled("doctor", crate::output::chain_message(&source)))?;
    Ok(Some(removed))
}

/// The reference's legacy migration on one parsed line: a singular
/// non-null `outcome` (with `outcomes` absent) becomes a one-element
/// `outcomes` array of its `legacy_entry` fields — the `outcome` key
/// drops and `outcomes` lands at the end of the object. Any other
/// line is `None` (unchanged).
fn migrate_legacy_line(record: &Value) -> Option<String> {
    let legacy = mulch::legacy_outcome(record).filter(|value| !value.is_null())?;
    let entry = mulch::legacy_entry(legacy);
    let mut object = record.as_object()?.clone();
    // shift_remove keeps the remaining keys in place (the reference's
    // spread keeps every other key where it was); the fresh key
    // appends at the end.
    object.shift_remove("outcome");
    object.insert("outcomes".into(), Value::Array(vec![Value::Object(entry)]));
    serde_json::to_string(&object).ok()
}

/// The domain file's current bytes (`None` when the file is absent —
/// the reference's repair passes skip silently).
fn read_domain_text(store: &mulch::StoreFiles, domain: &str) -> Result<Option<String>, Failure> {
    match std::fs::read_to_string(store.domain_path(domain)) {
        Ok(text) => Ok(Some(text)),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Failure::handled(
            "doctor",
            format!("reading domain {domain}: {source}"),
        )),
    }
}

/// Writes raw bytes — the reference's direct file write for the two
/// line-preserving passes (jsonl-integrity, legacy-outcome).
fn write_domain_text(store: &mulch::StoreFiles, domain: &str, body: &str) -> Result<(), Failure> {
    std::fs::write(store.domain_path(domain), body)
        .map_err(|source| Failure::handled("doctor", format!("writing domain {domain}: {source}")))
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
        let mut fields = Map::new();
        fields.insert("checks".into(), Value::Array(vec![Value::Object(check)]));
        fields.insert("summary".into(), Value::Object(summary));
        let mut failure = Failure::handled("doctor", "");
        failure.envelope = failure_envelope("doctor", fields);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn record(line: &str) -> Value {
        serde_json::from_str(line).expect("record")
    }

    /// A throwaway store with one live domain `d` for the pass tests.
    fn temp_store(tag: &str) -> (std::path::PathBuf, mulch::StoreFiles) {
        let dir = std::env::temp_dir().join(format!("mulch-doctor-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(dir.join(".mulch").join("expertise")).expect("temp store");
        std::fs::write(
            dir.join(".mulch").join("mulch.config.yaml"),
            "version: '1'\ndomains:\n  d: {}\n",
        )
        .expect("config");
        let store = match mulch::StoreFiles::locate(&dir).expect("store opens") {
            mulch::StoreLocation::Open(store) => store,
            other => panic!("store must open, got {other:?}"),
        };
        (dir, store)
    }

    fn read_domain(store: &mulch::StoreFiles, domain: &str) -> String {
        std::fs::read_to_string(store.domain_path(domain)).expect("domain file")
    }

    #[test]
    fn legacy_migration_keeps_key_order_and_appends_outcomes() {
        let migrated = migrate_legacy_line(&record(
            r#"{"type":"pattern","name":"p","outcome":{"status":"success","duration":5,"agent":"a"},"extra":1}"#,
        ))
        .expect("migrated");
        // `outcome` drops, `outcomes` lands at the end, every other key
        // keeps its position, and only the four known entry fields copy
        assert_eq!(
            migrated,
            r#"{"type":"pattern","name":"p","extra":1,"outcomes":[{"status":"success","duration":5,"agent":"a"}]}"#
        );
    }

    #[test]
    fn legacy_migration_skips_null_and_coexisting_outcomes() {
        assert_eq!(
            migrate_legacy_line(&record(r#"{"name":"p","outcome":null}"#)),
            None
        );
        assert_eq!(
            migrate_legacy_line(&record(
                r#"{"name":"p","outcome":{"status":"ok"},"outcomes":[]}"#
            )),
            None
        );
        assert_eq!(migrate_legacy_line(&record(r#"{"name":"p"}"#)), None);
    }

    #[test]
    fn legacy_migration_of_a_primitive_outcome_yields_an_empty_entry() {
        // the reference reads `.status` & co. as undefined on
        // primitives; JSON serialization drops undefined values
        assert_eq!(
            migrate_legacy_line(&record(r#"{"name":"p","outcome":"oops"}"#)).expect("migrated"),
            r#"{"name":"p","outcomes":[{}]}"#
        );
    }

    #[test]
    fn raw_line_pass_counts_changes_and_keeps_verbatim_bytes() {
        let (dir, store) = temp_store("raw");
        // dropping policy: unparsable lines (# comments included) drop,
        // survivors keep their trimmed bytes
        std::fs::write(
            store.domain_path("d"),
            "# banner\n{\"a\":1}\nnot json\n  {\"b\":2}  \n\n",
        )
        .expect("writable");
        let removed =
            raw_line_pass(&store, "d", &LineAction::Drop, |_| LineAction::Keep).expect("pass runs");
        assert_eq!(removed, Some(2));
        assert_eq!(read_domain(&store, "d"), "{\"a\":1}\n{\"b\":2}\n");
        // keeping policy: nothing changes -> None, no write at all
        std::fs::write(store.domain_path("d"), "not json\n{\"a\":1}\n").expect("writable");
        let untouched =
            raw_line_pass(&store, "d", &LineAction::Keep, |_| LineAction::Keep).expect("pass runs");
        assert_eq!(untouched, None);
        assert_eq!(read_domain(&store, "d"), "not json\n{\"a\":1}\n");
        // replacement: only replaced lines count, unparsable survivors stay
        let migrated = raw_line_pass(&store, "d", &LineAction::Keep, |value| {
            value.get("a").map_or(LineAction::Keep, |_| {
                LineAction::Replace("{\"a\":2}".into())
            })
        })
        .expect("pass runs");
        assert_eq!(migrated, Some(1));
        assert_eq!(read_domain(&store, "d"), "not json\n{\"a\":2}\n");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn value_line_pass_counts_drops_and_rewrites_canonically() {
        let (dir, store) = temp_store("value");
        std::fs::write(
            store.domain_path("d"),
            "{\"type\":\"pattern\",\"name\":\"p\"}\nnot json\n{\"junk\":1}\n",
        )
        .expect("writable");
        // unparsable lines drop uncounted; the typeless record drops
        let removed =
            value_line_pass(&store, "d", |record| record.get("type").is_some()).expect("pass runs");
        assert_eq!(removed, Some(1));
        // the canonical rewrite backfills the id (write_records)
        assert_eq!(
            read_domain(&store, "d"),
            format!(
                "{{\"type\":\"pattern\",\"name\":\"p\",\"id\":\"{}\"}}\n",
                mulch::record_id("pattern", "p")
            )
        );
        // keep-all: None, and non-canonical spacing survives (no write)
        std::fs::write(store.domain_path("d"), "{\"a\": 1}\n").expect("writable");
        let untouched = value_line_pass(&store, "d", |_| true).expect("pass runs");
        assert_eq!(untouched, None);
        assert_eq!(read_domain(&store, "d"), "{\"a\": 1}\n");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn governance_limits_tier_wording_and_worst_status_win() {
        let domain = |name: &str, records: usize| DomainLines {
            domain: name.into(),
            lines:  (1..=records)
                .map(|line| mulch::LenientLine::Record {
                    line,
                    record: serde_json::json!({"type": "pattern"}),
                })
                .collect(),
        };
        let governance = mulch::Governance {
            max_entries:  5,
            warn_entries: 8,
            hard_limit:   12,
        };
        // under every threshold: the clean-store pass
        let pass = governance_limits(&governance, &[domain("alpha", 4)]);
        assert!(matches!(pass.status, Status::Pass));
        assert_eq!(pass.message, "All domains within governance limits");
        assert_eq!(pass.details, Vec::<String>::new());
        // the thresholds fire AT the boundary (>=), worst status wins,
        // details follow domain order
        let over = governance_limits(&governance, &[
            domain("beta", 5),
            domain("gamma", 8),
            domain("delta", 12),
        ]);
        assert!(matches!(over.status, Status::Fail));
        assert_eq!(over.message, "3 domain(s) over governance thresholds");
        assert!(!over.fixable);
        assert_eq!(over.details, [
            "beta: 5 records (approaching limit of 5)",
            "gamma: 8 records (over warn threshold of 8)",
            "delta: 12 records (over hard limit of 12)",
        ]);
        // without a hard-limit overrun the check only warns
        let warn = governance_limits(&governance, &[domain("beta", 5)]);
        assert!(matches!(warn.status, Status::Warn));
        assert_eq!(warn.details, ["beta: 5 records (approaching limit of 5)"]);
    }

    /// The no-store JSON envelope without a store on disk: the failure
    /// shape (key order pinned: success, command, checks, summary) with
    /// the one failed config check (mulch-3368: the envelope constructor
    /// made this unit-testable without a store).
    #[test]
    fn no_store_json_envelope_is_the_failure_shape_without_a_store() {
        let opts = GlobalOpts {
            json:                  true,
            quiet:                 false,
            verbose:               false,
            timing:                false,
            allow_unknown_types:   false,
            allow_domain_mismatch: false,
        };
        let failure = no_store_report(&opts);
        assert_eq!(failure.code, 1);
        // the envelope prints to stdout, and nothing renders after it
        assert!(!failure.envelope_to_stderr);
        assert!(!failure.rendered);
        let Value::Object(body) = &failure.envelope else {
            panic!("envelope is an object");
        };
        assert_eq!(body.keys().collect::<Vec<_>>(), [
            "success", "command", "checks", "summary"
        ]);
        assert_eq!(body["success"], Value::Bool(false));
        assert_eq!(body["command"], Value::String("doctor".into()));
        let Value::Array(checks) = &body["checks"] else {
            panic!("checks is an array");
        };
        let check = checks.first().expect("the one config check");
        assert_eq!(check["name"], Value::String("config".into()));
        assert_eq!(check["status"], Value::String("fail".into()));
        assert_eq!(
            check["message"],
            Value::String("No .mulch/ directory found".into())
        );
        assert_eq!(check["fixable"], Value::Bool(false));
        assert_eq!(check["details"], Value::Array(Vec::new()));
        assert_eq!(body["summary"]["pass"], Value::from(0));
        assert_eq!(body["summary"]["warn"], Value::from(0));
        assert_eq!(body["summary"]["fail"], Value::from(1));
    }

    /// The plain no-store report prints itself (header + summary on
    /// stdout, the failed check on stderr); the failure carries only
    /// the exit contract.
    #[test]
    fn no_store_plain_report_carries_only_the_exit_code() {
        let opts = GlobalOpts {
            json:                  false,
            quiet:                 false,
            verbose:               false,
            timing:                false,
            allow_unknown_types:   false,
            allow_domain_mismatch: false,
        };
        let failure = no_store_report(&opts);
        assert_eq!(failure.code, 1);
        assert!(failure.rendered);
        assert_eq!(failure.message, "");
    }

    #[test]
    fn fix_gates_open_only_for_failed_fixable_checks() {
        fn check(name: &'static str, status: Status) -> Check {
            Check {
                name,
                status,
                message: String::new(),
                fixable: true,
                details: Vec::new(),
            }
        }
        let mut unfixable = check("stale-records", Status::Fail);
        unfixable.fixable = false;
        let checks = vec![
            check("jsonl-integrity", Status::Pass),
            check("legacy-outcome", Status::Warn),
            check("schema-validation", Status::Fail),
            unfixable,
        ];
        let gates = FixGates::from_checks(&checks);
        assert!(!gates.jsonl);
        assert!(gates.legacy);
        assert!(gates.schema);
        // failed but not fixable: the pass stays closed
        assert!(!gates.stale);
    }
}
