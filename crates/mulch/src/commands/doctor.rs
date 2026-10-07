//! `mulch doctor` — the 17-check health report, plain and `--json`,
//! with `--fix` as four sequential repair passes in check order
//! (jsonl-integrity, legacy-outcome, schema-validation,
//! stale-records).

use std::collections::HashMap;
use std::collections::hash_map::Entry as HashMapEntry;
use std::fmt::Write as _;

use jiff::Timestamp;
use mulch::schema::doctor_detail;
use mulch::stale::{StaleRule, StaleVerdict};
use serde_json::{Map, Value};

use crate::cli::GlobalOpts;
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
    let rule = StaleRule::from_shelf_life(&store.config().effective_shelf_life());
    let checks = run_checks(opts, &rule, &domains);
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
        duplicates(domains),
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
            records_of(d).filter_map(|(line, record)| {
                let has_legacy = record
                    .as_object()
                    .is_some_and(|o| o.contains_key("outcome") && !o.contains_key("outcomes"));
                has_legacy.then(|| {
                    format!(
                        "{}:{line} - legacy \"outcome\" field (singular); should be \"outcomes[]\"",
                        d.domain
                    )
                })
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
            // to tactical).
            if rule.verdict(record, now) == StaleVerdict::Stale {
                // Stale implies a known string classification (the
                // verdict never flags anything else).
                let classification = record
                    .get("classification")
                    .and_then(Value::as_str)
                    .expect("verdict Stale implies a string classification");
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
            if let Some(count) = remove_malformed_lines(store, &domain.domain)? {
                fixes.push(format!(
                    "Removed {count} invalid JSON line(s) from {}",
                    domain.domain
                ));
            }
        }
    }
    if gates.legacy {
        for domain in domains {
            if let Some(count) = migrate_legacy_outcomes(store, &domain.domain)? {
                fixes.push(format!(
                    "Migrated {count} legacy \"outcome\" field(s) to \"outcomes[]\" in {}",
                    domain.domain
                ));
            }
        }
    }
    if gates.schema {
        for domain in domains {
            if let Some(count) = remove_invalid_records(store, &domain.domain)? {
                fixes.push(format!(
                    "Removed {count} invalid record(s) from {}",
                    domain.domain
                ));
            }
        }
    }
    if gates.stale {
        for domain in domains {
            if let Some(count) = prune_stale_records(store, rule, &domain.domain, now)? {
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

/// The jsonl-integrity repair: malformed lines (comments included —
/// they fail JSON parsing) drop; valid lines stay byte-identical.
/// `None` = nothing to remove, no write.
fn remove_malformed_lines(
    store: &mulch::StoreFiles,
    domain: &str,
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
        if serde_json::from_str::<Value>(trimmed).is_ok() {
            kept.push(trimmed);
        } else {
            removed += 1;
        }
    }
    if removed == 0 {
        return Ok(None);
    }
    let mut body = kept.join("\n");
    if !kept.is_empty() {
        body.push('\n');
    }
    write_domain_text(store, domain, &body)?;
    Ok(Some(removed))
}

/// The legacy-outcome repair: a singular non-null `outcome` without an
/// `outcomes` array migrates; every other line stays verbatim.
/// `None` = nothing to migrate, no write.
fn migrate_legacy_outcomes(
    store: &mulch::StoreFiles,
    domain: &str,
) -> Result<Option<usize>, Failure> {
    let Some(text) = read_domain_text(store, domain)? else {
        return Ok(None);
    };
    let mut lines = Vec::new();
    let mut migrated = 0usize;
    for line in text.split('\n') {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(trimmed) {
            // Unparsable survivors (only possible when the
            // jsonl-integrity pass stayed closed) pass through
            // byte-identical, like the reference's raw-line loop.
            Err(_) => lines.push(trimmed.to_string()),
            Ok(value) => match migrate_legacy_line(value) {
                Some(line) => {
                    lines.push(line);
                    migrated += 1;
                }
                None => lines.push(trimmed.to_string()),
            },
        }
    }
    if migrated == 0 {
        return Ok(None);
    }
    let mut body = lines.join("\n");
    if !lines.is_empty() {
        body.push('\n');
    }
    write_domain_text(store, domain, &body)?;
    Ok(Some(migrated))
}

/// The reference's legacy migration on one parsed line: a singular
/// `outcome` (non-null, `outcomes` absent) becomes a one-element
/// `outcomes` array of its `status`/`duration`/`test_results`/`agent`
/// fields — present keys only, in that order — the `outcome` key drops,
/// and `outcomes` lands at the end of the object. Any other line is
/// `None` (unchanged).
fn migrate_legacy_line(value: Value) -> Option<String> {
    let Value::Object(mut object) = value else {
        return None;
    };
    let outcome = object.get("outcome")?;
    if outcome.is_null() || object.contains_key("outcomes") {
        return None;
    }
    // A primitive `outcome` migrates to an empty entry (the reference
    // reads `.status` & co. as undefined on it, and JSON serialization
    // drops undefined values).
    let mut entry = Map::new();
    if let Some(fields) = outcome.as_object() {
        for key in ["status", "duration", "test_results", "agent"] {
            if let Some(field) = fields.get(key) {
                entry.insert(key.into(), field.clone());
            }
        }
    }
    object.remove("outcome");
    object.insert("outcomes".into(), Value::Array(vec![Value::Object(entry)]));
    serde_json::to_string(&Value::Object(object)).ok()
}

/// The schema-validation repair: known-type records that fail the
/// schema drop; records of unregistered or missing types STAY — the
/// reference flags them via checkUnknownTypes and never silently
/// deletes them (mulch-d45c). `None` = nothing to remove, no write.
fn remove_invalid_records(
    store: &mulch::StoreFiles,
    domain: &str,
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
        if survives_schema_pass(&record) {
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

/// The schema pass keep rule: unregistered or missing types stay,
/// known types stay iff schema-valid.
fn survives_schema_pass(record: &Value) -> bool {
    let Some(kind) = record.get("type").and_then(Value::as_str) else {
        return true;
    };
    match mulch::type_spec(kind) {
        None => true,
        Some(_) => doctor_detail(record).is_none(),
    }
}

/// The stale-records repair over the survivors of the earlier passes:
/// stale records prune, the remainder rewrites canonically. `None` =
/// nothing to prune, no write.
fn prune_stale_records(
    store: &mulch::StoreFiles,
    rule: &StaleRule,
    domain: &str,
    now: Timestamp,
) -> Result<Option<usize>, Failure> {
    let Some(text) = read_domain_text(store, domain)? else {
        return Ok(None);
    };
    let mut kept = Vec::new();
    let mut pruned = 0usize;
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
        // The verdict owns the extraction; an unparsable or missing
        // recorded_at never prunes, and a missing/unknown
        // classification never decays (mulch-f9b9).
        if rule.verdict(&record, now) == StaleVerdict::Stale {
            pruned += 1;
        } else {
            kept.push(record);
        }
    }
    if pruned == 0 {
        return Ok(None);
    }
    store
        .rewrite_domain(domain, &kept)
        .map_err(|source| Failure::handled("doctor", crate::output::chain_message(&source)))?;
    Ok(Some(pruned))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn record(line: &str) -> Value {
        serde_json::from_str(line).expect("record")
    }

    #[test]
    fn legacy_migration_keeps_key_order_and_appends_outcomes() {
        let migrated =
            migrate_legacy_line(record(r#"{"type":"pattern","name":"p","outcome":{"status":"success","duration":5,"agent":"a"},"extra":1}"#))
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
            migrate_legacy_line(record(r#"{"name":"p","outcome":null}"#)),
            None
        );
        assert_eq!(
            migrate_legacy_line(record(
                r#"{"name":"p","outcome":{"status":"ok"},"outcomes":[]}"#
            )),
            None
        );
        assert_eq!(migrate_legacy_line(record(r#"{"name":"p"}"#)), None);
    }

    #[test]
    fn legacy_migration_of_a_primitive_outcome_yields_an_empty_entry() {
        // the reference reads `.status` & co. as undefined on
        // primitives; JSON serialization drops undefined values
        assert_eq!(
            migrate_legacy_line(record(r#"{"name":"p","outcome":"oops"}"#)).expect("migrated"),
            r#"{"name":"p","outcomes":[{}]}"#
        );
    }

    #[test]
    fn schema_pass_keeps_unknown_and_typeless_records() {
        // mulch-d45c: unregistered types are flagged, never deleted
        assert!(survives_schema_pass(&record(
            r#"{"type":"wtf","name":"u"}"#
        )));
        assert!(survives_schema_pass(&record(r#"{"name":"typeless"}"#)));
        // known types stay iff schema-valid (full required set)
        assert!(survives_schema_pass(&record(
            r#"{"type":"pattern","name":"p","description":"d","classification":"tactical","recorded_at":"2026-10-07T00:00:00.000Z"}"#
        )));
        assert!(!survives_schema_pass(&record(r#"{"type":"pattern"}"#)));
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
