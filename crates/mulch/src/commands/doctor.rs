//! `mulch doctor` — the 17-check health report, plain and `--json`,
//! with `--fix` as six sequential repair passes in check order
//! (jsonl-integrity, legacy-outcome, schema-validation,
//! stale-records, orphaned-domains, file-anchors).

use std::collections::HashMap;
use std::collections::hash_map::Entry as HashMapEntry;
use std::fmt::Write as _;

use jiff::Timestamp;
use mulch::schema::doctor_detail;
use mulch::stale::{StaleRule, StaleVerdict};
use mulch::{LineAction, parsed_records};
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

impl Check {
    /// A FINDINGS check (mulch-f8c7): Pass with `pass_message` when
    /// nothing was found; `bad_status` with `bad_message(len)` and
    /// the details otherwise. The two-branch template ~ten builders
    /// hand-rolled; `fixable` applies to BOTH arms (the report
    /// renders it either way — the gate ignores it on Pass).
    /// Genuinely different
    /// laws (governance's worst-status, duplicates' two-class
    /// message) stay hand-built.
    fn findings(
        name: &'static str,
        details: Vec<String>,
        bad_status: Status,
        fixable: bool,
        pass_message: &str,
        bad_message: impl Fn(usize) -> String,
    ) -> Check {
        if details.is_empty() {
            Check {
                name,
                status: Status::Pass,
                message: pass_message.into(),
                fixable,
                details: Vec::new(),
            }
        } else {
            Check {
                name,
                status: bad_status,
                message: bad_message(details.len()),
                fixable,
                details,
            }
        }
    }

    /// An always-pass check with no details — the hardcoded passes
    /// (config, decay-config, upgrade & friends) whose reference
    /// implementations we satisfy by construction.
    fn stub(name: &'static str, message: &str) -> Check {
        Check {
            name,
            status: Status::Pass,
            message: message.into(),
            fixable: false,
            details: Vec::new(),
        }
    }
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
    domain:    String,
    /// Canonicalized (aliases applied — the lib seam most checks
    /// judge over, mirroring the reference's pre-Ajv application).
    lines:     Vec<mulch::LenientLine>,
    /// The PRE-alias parse: `checkLegacyOutcome` reads raw bytes
    /// (doctor.ts parses its own lines without `applyAliases`), so a
    /// config whose aliases use `outcome` as a legacy name must not
    /// hide or fake the warn (spec review 2026-10-17, mulch-b1cb).
    raw_lines: Vec<mulch::LenientLine>,
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
    // `--fix` repairs under the writer lock (the whole re-read/repair
    // cycle, mulch-228d); the report-only mode stays lock-free.
    let located = if fix {
        mulch::StoreFiles::locate_exclusive(&cwd)
    } else {
        mulch::StoreFiles::locate(&cwd)
    };
    let store = match located {
        Ok(mulch::StoreLocation::Open(store)) => store,
        Ok(mulch::StoreLocation::Missing | mulch::StoreLocation::NoConfig) => {
            return Err(no_store_report(opts));
        }
        Err(source) => {
            return Err(crate::commands::store::locate_error("doctor", &source));
        }
    };

    let domains = read_domains(&store);
    let config = store.config();
    let rule = StaleRule::from_shelf_life(&config.effective_shelf_life());
    let governance = config.effective_governance();
    let checks = run_checks(&rule, &governance, &domains, &store, &cwd);

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
    let mut store = store;
    let fixed = if fix {
        apply_fixes(&mut store, &rule, &domains, &checks, &cwd)?
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
            raw_lines: mulch::read_lenient(&store.domain_path(&domain)),
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
    rule: &StaleRule,
    governance: &mulch::Governance,
    domains: &[DomainLines],
    store: &mulch::StoreFiles,
    project_root: &std::path::Path,
) -> Vec<Check> {
    vec![
        Check::stub("config", "Config is valid"),
        jsonl_integrity(domains),
        legacy_outcome(domains),
        schema_validation(store.registry(), domains),
        unknown_types(store.registry(), domains),
        type_registry(store.registry(), domains),
        Check::stub(
            "domain-rules-compatibility",
            "All domain required_fields are compatible with allowed types",
        ),
        domain_conformance(domains),
        Check::stub("domain-violations", "All records conform to domain rules"),
        stale_records(rule, domains),
        orphaned_domains(store),
        duplicates(store.registry(), domains),
        file_anchors(domains, project_root),
        governance_limits(governance, domains),
        Check::stub("decay-config", "decay.anchor_validity uses defaults"),
        Check::stub(
            "compact-summarizer",
            "compact-summarizer: not configured (mechanical merge in use)",
        ),
        Check::stub(
            "upgrade",
            "Native binary — update via cargo install (registry check not applicable)",
        ),
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
    Check::findings(
        Repair::JsonlIntegrity.check_name(),
        bad,
        Status::Fail,
        true,
        "All JSONL lines are valid JSON",
        |count| format!("{count} invalid JSON line(s) found"),
    )
}

fn legacy_outcome(domains: &[DomainLines]) -> Check {
    // Raw records (see `DomainLines::raw_lines`): the outcome key on
    // disk is what the check judges, pre-alias.
    let bad: Vec<String> = domains
        .iter()
        .flat_map(|d| {
            d.raw_lines
                .iter()
                .filter_map(|line| match line {
                    mulch::LenientLine::Record { line, record } => Some((*line, record)),
                    mulch::LenientLine::Malformed { .. } => None,
                })
                .filter(|&(_, record)| mulch::legacy_outcome(record).is_some())
                .map(|(line, _)| {
                    format!(
                        "{}:{line} - legacy \"outcome\" field (singular); should be \"outcomes[]\"",
                        d.domain
                    )
                })
        })
        .collect();
    // The reference reports legacy fields as a warning, never a
    // failure.
    Check::findings(
        Repair::LegacyOutcome.check_name(),
        bad,
        Status::Warn,
        true,
        "No legacy \"outcome\" fields on disk",
        |count| format!("{count} record(s) with legacy \"outcome\" field on disk"),
    )
}

fn schema_validation(registry: &mulch::Registry, domains: &[DomainLines]) -> Check {
    let bad: Vec<String> = domains
        .iter()
        .flat_map(|d| {
            records_of(d).filter_map(|(line, record)| {
                doctor_detail(registry, record)
                    .map(|message| format!("{}:{} - {}", d.domain, line, message))
            })
        })
        .collect();
    Check::findings(
        Repair::SchemaValidation.check_name(),
        bad,
        Status::Fail,
        true,
        "All records pass schema validation",
        |count| format!("{count} record(s) failed schema validation"),
    )
}

fn unknown_types(registry: &mulch::Registry, domains: &[DomainLines]) -> Check {
    let known: Vec<&str> = registry
        .rows()
        .iter()
        .map(|row| row.name.as_str())
        .collect();
    let bad: Vec<String> = domains
        .iter()
        .flat_map(|d| {
            records_of(d).filter_map(|(line, record)| {
                let kind = record.as_object()?.get("type")?.as_str()?;
                (!known.contains(&kind)).then(|| {
                    format!(
                        "{}:{line}{} - unknown type \"{kind}\"",
                        d.domain,
                        unknown_id_part(record.get("id"))
                    )
                })
            })
        })
        .collect();
    // `checkUnknownTypes` never consults --allow-unknown-types (the
    // flag is validate's read-through hatch only — probe-pinned vs
    // ml 0.10.7: doctor still exits 1 with it).
    Check::findings(
        "unknown-types",
        bad,
        Status::Fail,
        false,
        "All record types are registered",
        |count| format!("{count} record(s) with unregistered types"),
    )
}

/// The unknown-types detail id suffix: ` [{id}]` when the record's
/// `id` is a JSON string — the empty string renders ` []` (probed vs
/// ml 0.10.7). This is the typeof law (reference doctor.ts
/// `checkUnknownTypes`: `typeof id === "string"`), NOT the
/// file-anchors truthiness bracket ([`id_bracket`], doctor.ts
/// `checkFileAnchors`: `record.id ? [...] : ""`) — a truthy
/// non-string id brackets there but never here.
fn unknown_id_part(id: Option<&Value>) -> String {
    match id.and_then(Value::as_str) {
        Some(text) => format!(" [{text}]"),
        None => String::new(),
    }
}

fn type_registry(registry: &mulch::Registry, domains: &[DomainLines]) -> Check {
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
    let details: Vec<String> = registry
        .rows()
        .iter()
        .map(|row| {
            let count = counts.get(row.name.as_str()).copied().unwrap_or(0);
            let plural = if count == 1 { "record" } else { "records" };
            let kind = if row.builtin { "built-in" } else { "custom" };
            format!("{} ({kind}): {count} {plural}", row.name)
        })
        .collect();
    let builtins = registry.rows().iter().filter(|row| row.builtin).count();
    let customs = registry.rows().len() - builtins;
    Check {
        name: "type-registry",
        status: Status::Pass,
        message: format!(
            "{} type(s) registered: {builtins} built-in, {customs} custom",
            registry.rows().len()
        ),
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
    Check::findings(
        Repair::StaleRecords.check_name(),
        details,
        Status::Warn,
        true,
        "No stale records",
        |count| format!("{count} stale record(s) found"),
    )
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
fn duplicates(registry: &mulch::Registry, domains: &[DomainLines]) -> Check {
    let mut reference_details = Vec::new();
    let mut divergent_details = Vec::new();
    for domain in domains {
        let records: Vec<&Value> = records_of(domain).map(|(_, record)| record).collect();
        for (index, record) in records.iter().enumerate().skip(1) {
            let Some(matched) =
                mulch::find_duplicate(registry, records[..index].iter().copied(), record)
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

/// Orphaned domains (reference `checkOrphanedDomains`): both
/// directions — expertise files whose domain is missing from the
/// config, and config domains without an expertise file. Warns with
/// one detail per issue (file findings first, then config order);
/// `--fix` adopts the files into the config and creates missing
/// files.
fn orphaned_domains(store: &mulch::StoreFiles) -> Check {
    let config_domains = store.domains();
    let mut details = Vec::new();
    for domain in domains_on_disk(store) {
        if !config_domains.contains(&domain) {
            details.push(format!(
                "File \"{domain}.jsonl\" exists but domain \"{domain}\" is not in config"
            ));
        }
    }
    for domain in &config_domains {
        if !store.has_domain_file(domain) {
            details.push(format!(
                "Domain \"{domain}\" in config but no JSONL file exists"
            ));
        }
    }
    Check::findings(
        Repair::OrphanedDomains.check_name(),
        details,
        Status::Warn,
        true,
        "No orphaned domains",
        |count| format!("{count} orphaned domain issue(s)"),
    )
}

/// The domain names of the expertise `.jsonl` files on disk, in the
/// directory's own iteration order (the reference's `readdir` — both
/// sides walk the same unsorted stream, so twin stores list
/// identically). Read errors read as an empty listing: the
/// reference's `readdir` try/catch is total — a missing expertise dir
/// on a fresh store and an unreadable one both mean "no orphans from
/// disk".
fn domains_on_disk(store: &mulch::StoreFiles) -> Vec<String> {
    std::fs::read_dir(store.root().join("expertise"))
        .into_iter()
        .flatten()
        .filter_map(std::result::Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            name.strip_suffix(".jsonl").map(String::from)
        })
        .collect()
}

/// File anchors (reference `checkFileAnchors`): every record's
/// `files[]`, `dir_anchors[]`, and `evidence.file` entries must
/// resolve to existing paths under the project root. Warns with one
/// detail per broken anchor in record order; `--fix` strips the
/// broken entries.
fn file_anchors(domains: &[DomainLines], project_root: &std::path::Path) -> Check {
    let mut details = Vec::new();
    for domain in domains {
        for (_, record) in records_of(domain) {
            let id = id_bracket(record.get("id"));
            for broken in broken_anchors(record, project_root) {
                details.push(format!("{}{id}: {broken}", domain.domain));
            }
        }
    }
    Check::findings(
        Repair::FileAnchors.check_name(),
        details,
        Status::Warn,
        true,
        "All file anchors resolve to existing paths",
        |count| format!("{count} broken file anchor(s) found"),
    )
}

/// The reference's id bracket (`record.id ? `[${record.id}]` : ""`):
/// JS-truthy ids interpolate through the template-literal coercion
/// ([`mulch::value_text`]). Truthiness is the jsval seam's general law
/// — arrays/objects are truthy in JS, and a hand-corrupted store that
/// carries one as an id renders its coercion (probe-pinned to the
/// reference's own template literal, mulch-ec71).
fn id_bracket(id: Option<&Value>) -> String {
    match id.filter(|id| mulch::truthy(id)) {
        Some(value) => format!("[{}]", mulch::value_text(value)),
        None => String::new(),
    }
}

/// A record's broken anchor detail bodies in the reference's report
/// order: `files[]` entries, `dir_anchors[]` entries, then
/// `evidence.file` — the lib anchors module owns the walk and the
/// resolution (mulch-5a3d); doctor only renders. Non-string entries
/// never participate (the reference crashes on them — README
/// DEVIATIONS: doctor reports instead).
fn broken_anchors(record: &Value, project_root: &std::path::Path) -> Vec<String> {
    mulch::anchors::record_anchors(record)
        .into_iter()
        .filter(|(_, path)| !mulch::anchors::exists(project_root, path))
        .map(|(kind, path)| match kind {
            "file" => format!("files[] path not found: {path}"),
            "dir" => format!("dir_anchors[] path not found: {path}"),
            _ => format!("evidence.file not found: {path}"),
        })
        .collect()
}

/// Strips a record's broken anchors in place (the `file-anchors`
/// repair): missing `files[]`/`dir_anchors[]` entries drop — the key
/// itself drops when nothing survives — and a missing `evidence.file`
/// key drops from its evidence object. Returns how many anchors were
/// removed.
fn strip_broken_anchors(record: &mut Value, project_root: &std::path::Path) -> usize {
    let Some(object) = record.as_object_mut() else {
        return 0;
    };
    let mut removed = 0;
    for field in ["files", "dir_anchors"] {
        let Some(Value::Array(entries)) = object.get_mut(field) else {
            continue;
        };
        let before = entries.len();
        entries.retain(|entry| {
            entry
                .as_str()
                .is_none_or(|path| mulch::anchors::exists(project_root, path))
        });
        let dropped = before - entries.len();
        if dropped > 0 {
            removed += dropped;
            if entries.is_empty() {
                // shift_remove keeps the remaining keys in place (the
                // reference's spread-assign drops the key only)
                object.shift_remove(field);
            }
        }
    }
    if let Some(evidence) = record.get("evidence")
        && let Some(file) = evidence.get("file").and_then(Value::as_str)
        && !file.is_empty()
        && !mulch::anchors::exists(project_root, file)
    {
        if let Some(evidence) = record.get_mut("evidence").and_then(Value::as_object_mut) {
            evidence.shift_remove("file");
        }
        removed += 1;
    }
    removed
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
        // The ladder verdict is the lib's (ADR-0005); the check
        // statuses and detail wordings stay here.
        let (tier, threshold, wording) = match governance.tier(count) {
            mulch::Tier::Hard => (Status::Fail, governance.hard_limit, "over hard limit of"),
            mulch::Tier::Warn => (
                Status::Warn,
                governance.warn_entries,
                "over warn threshold of",
            ),
            mulch::Tier::Approaching => {
                (Status::Warn, governance.max_entries, "approaching limit of")
            }
            mulch::Tier::Ok => continue,
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
    store: &mut mulch::StoreFiles,
    rule: &StaleRule,
    domains: &[DomainLines],
    checks: &[Check],
    project_root: &std::path::Path,
) -> Result<Vec<String>, Failure> {
    let now = Timestamp::now();
    let mut fixes = Vec::new();
    // The reference iterates its CHECK array, running one repair case
    // per failed fixable check. Gated by VALUE through [`Repair`], so
    // the pass list and the check names cannot drift apart
    // (mulch-d343).
    for repair in Repair::ALL {
        if !repair.opens(checks) {
            continue;
        }
        match repair {
            Repair::JsonlIntegrity => {
                for domain in domains {
                    run_raw_pass(
                        store,
                        &domain.domain,
                        &LineAction::Drop,
                        |_| LineAction::Keep,
                        &mut fixes,
                        |count, domain| {
                            format!("Removed {count} invalid JSON line(s) from {domain}")
                        },
                    )?;
                }
            }
            Repair::LegacyOutcome => {
                for domain in domains {
                    run_raw_pass(
                        store,
                        &domain.domain,
                        &LineAction::Keep,
                        |value| {
                            migrate_legacy_line(value).map_or(LineAction::Keep, LineAction::Replace)
                        },
                        &mut fixes,
                        |count, domain| {
                            format!(
                                "Migrated {count} legacy \"outcome\" field(s) to \"outcomes[]\" in {domain}"
                            )
                        },
                    )?;
                }
            }
            Repair::SchemaValidation => {
                let keep =
                    |record: &Value| mulch::schema::survives_repair(store.registry(), record);
                for domain in domains {
                    run_value_pass(store, &domain.domain, keep, &mut fixes, |count, domain| {
                        format!("Removed {count} invalid record(s) from {domain}")
                    })?;
                }
            }
            Repair::StaleRecords => {
                // The verdict owns the extraction; an unparsable or missing
                // recorded_at never prunes, and a missing/unknown
                // classification never decays (mulch-f9b9).
                let keep = |record: &Value| {
                    !matches!(rule.verdict(record, now), StaleVerdict::Stale { .. })
                };
                for domain in domains {
                    run_value_pass(store, &domain.domain, keep, &mut fixes, |count, domain| {
                        format!("Pruned {count} stale record(s) from {domain}")
                    })?;
                }
            }
            Repair::OrphanedDomains => {
                // Adopts orphan files into the config (unvalidated — whatever
                // `.jsonl` names exist), creates missing domain files, and
                // rewrites the config once when anything was fixed (the
                // reference checks its whole `fixed` array, so an earlier
                // pass's lines also trigger the rewrite — byte-identical
                // either way).
                let config_domains = store.domains();
                for domain in domains_on_disk(store) {
                    if !config_domains.contains(&domain) {
                        store.adopt_domain(&domain);
                        fixes.push(format!("Added orphaned domain \"{domain}\" to config"));
                    }
                }
                for domain in store.domains() {
                    if store
                        .ensure_domain_file(&domain)
                        .map_err(|source| store_failure(&source))?
                    {
                        fixes.push(format!(
                            "Created missing JSONL file for domain \"{domain}\""
                        ));
                    }
                }
                if !fixes.is_empty() {
                    store
                        .write_config()
                        .map_err(|source| store_failure(&source))?;
                }
            }
            Repair::FileAnchors => {
                // Fresh config order: the orphan pass above may have adopted
                // domains whose records carry broken anchors too.
                for domain in store.domains() {
                    if let Some(count) = anchors_pass(store, &domain, project_root)? {
                        fixes.push(format!(
                            "Removed {count} broken file anchor(s) from {domain}"
                        ));
                    }
                }
            }
        }
    }
    Ok(fixes)
}

/// The raw-line repair shape (jsonl-integrity, legacy-outcome): run
/// the pass over the domain's text and, when it changed, write the
/// body back and report one fix line. The CALLER owns the IO; the
/// pass owns only the line laws (one write owner, mulch-d343).
fn run_raw_pass(
    store: &mulch::StoreFiles,
    domain: &str,
    unparsable: &LineAction,
    action: impl Fn(&Value) -> LineAction,
    fixes: &mut Vec<String>,
    message: impl Fn(usize, &str) -> String,
) -> Result<(), Failure> {
    let Some(text) = read_domain_text(store, domain)? else {
        return Ok(());
    };
    let Some((body, count)) = mulch::raw_line_pass(&text, unparsable, action) else {
        return Ok(());
    };
    write_domain_text(store, domain, &body)?;
    fixes.push(message(count, domain));
    Ok(())
}

/// The canonical-rewrite repair shape (schema-validation,
/// stale-records): run the pure value pass over the domain's text
/// and, when records dropped, rewrite the domain through the store
/// seam and report one fix line. Again the caller owns both the read
/// and the write; the pass returns what to keep.
fn run_value_pass(
    store: &mulch::StoreFiles,
    domain: &str,
    keep: impl Fn(&Value) -> bool,
    fixes: &mut Vec<String>,
    message: impl Fn(usize, &str) -> String,
) -> Result<(), Failure> {
    let Some(text) = read_domain_text(store, domain)? else {
        return Ok(());
    };
    let Some((kept, removed)) = mulch::value_line_pass(&text, store.registry(), keep) else {
        return Ok(());
    };
    store
        .rewrite_domain(domain, &kept)
        .map_err(|source| store_failure(&source))?;
    fixes.push(message(removed, domain));
    Ok(())
}

/// Maps a lib store error onto the doctor failure shape.
fn store_failure(source: &mulch::Error) -> Failure {
    Failure::handled("doctor", crate::output::chain_message(source))
}

/// The repairable checks, keyed by VALUE (mulch-d343): each variant
/// names the check it repairs, and the check BUILDERS mint their
/// report names from [`Repair::check_name`] — one spelling for the
/// mint and the gate. A renamed check whose repair was not carried
/// along used to silently disable the repair (no compile error, no
/// test); now the name exists exactly here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Repair {
    JsonlIntegrity,
    LegacyOutcome,
    SchemaValidation,
    StaleRecords,
    OrphanedDomains,
    FileAnchors,
}

impl Repair {
    /// Every repair pass, in the reference's fix-case order (jsonl,
    /// legacy, schema, stale, orphaned, anchors — report lines follow
    /// this order, domain order within each pass).
    const ALL: [Repair; 6] = [
        Repair::JsonlIntegrity,
        Repair::LegacyOutcome,
        Repair::SchemaValidation,
        Repair::StaleRecords,
        Repair::OrphanedDomains,
        Repair::FileAnchors,
    ];

    /// The check's report name — parity-pinned report bytes, spelled
    /// exactly once as the single source for the builder AND the gate.
    fn check_name(self) -> &'static str {
        match self {
            Repair::JsonlIntegrity => "jsonl-integrity",
            Repair::LegacyOutcome => "legacy-outcome",
            Repair::SchemaValidation => "schema-validation",
            Repair::StaleRecords => "stale-records",
            Repair::OrphanedDomains => "orphaned-domains",
            Repair::FileAnchors => "file-anchors",
        }
    }

    /// The reference's gate law: a pass repairs only when its check
    /// is fixable and did not pass (a passing check's data is clean
    /// by construction).
    fn opens(self, checks: &[Check]) -> bool {
        checks.iter().any(|check| {
            check.name == self.check_name()
                && check.fixable
                && !matches!(check.status, Status::Pass)
        })
    }
}

/// The file-anchors repair on one domain: every line parses, each
/// record sheds its broken anchors ([`strip_broken_anchors`]), and
/// the domain rewrites through `rewrite_domain` (canonical) when
/// anything was removed. Returns the number of removed anchors,
/// `None` when the file is absent or nothing changed (no write).
fn anchors_pass(
    store: &mulch::StoreFiles,
    domain: &str,
    project_root: &std::path::Path,
) -> Result<Option<usize>, Failure> {
    let Some(text) = read_domain_text(store, domain)? else {
        return Ok(None);
    };
    let mut records = parsed_records(&text, store.registry());
    let mut removed = 0usize;
    for record in &mut records {
        removed += strip_broken_anchors(record, project_root);
    }
    if removed == 0 {
        return Ok(None);
    }
    crate::commands::store::persist("doctor", store, domain, &records)?;
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

/// The store's domain-text read, mapped onto doctor's handled shape
/// (`None` when the file is absent — the reference's repair passes
/// skip silently).
fn read_domain_text(store: &mulch::StoreFiles, domain: &str) -> Result<Option<String>, Failure> {
    store
        .read_domain_text(domain)
        .map_err(|source| store_failure(&source))
}

/// The store's atomic domain-text rewrite for the two line-preserving
/// passes (jsonl-integrity, legacy-outcome).
fn write_domain_text(store: &mulch::StoreFiles, domain: &str, body: &str) -> Result<(), Failure> {
    store
        .rewrite_domain_text(domain, body)
        .map_err(|source| store_failure(&source))
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
        // The one-check report reuses the shared check shape (the
        // seed's item 4: the hand-built Map duplicated check_json).
        let check = Check {
            name:    "config",
            status:  Status::Fail,
            message: "No .mulch/ directory found".into(),
            fixable: false,
            details: Vec::new(),
        };
        let mut summary = Map::new();
        summary.insert("pass".into(), Value::from(0));
        summary.insert("warn".into(), Value::from(0));
        summary.insert("fail".into(), Value::from(1));
        let mut fields = Map::new();
        fields.insert("checks".into(), Value::Array(vec![check_json(&check)]));
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
            // Its own parity byte law, NOT the run report's shape: no
            // "warning(s)" suffix and no per-check lines — the two
            // look-alikes stay separate on purpose (seed item 4's
            // other half).
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

    /// A fixable check with the given name and status — the gate
    /// tests' report input.
    fn check(name: &'static str, status: Status) -> Check {
        Check {
            name,
            status,
            message: String::new(),
            fixable: true,
            details: Vec::new(),
        }
    }

    fn record(line: &str) -> Value {
        serde_json::from_str(line).expect("record")
    }

    /// A throwaway store with one live domain `d` for the pass tests.
    fn temp_store(tag: &str) -> (std::path::PathBuf, mulch::StoreFiles) {
        crate::commands::testscratch::store_with(tag, "version: '1'\ndomains:\n  d: {}\n", true)
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
    fn governance_limits_tier_wording_and_worst_status_win() {
        let domain = |name: &str, records: usize| DomainLines {
            domain:    name.into(),
            lines:     (1..=records)
                .map(|line| mulch::LenientLine::Record {
                    line,
                    record: serde_json::json!({"type": "pattern"}),
                })
                .collect(),
            raw_lines: Vec::new(),
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
            format:                None,
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
            format:                None,
        };
        let failure = no_store_report(&opts);
        assert_eq!(failure.code, 1);
        assert!(failure.rendered);
        assert_eq!(failure.message, "");
    }

    #[test]
    fn repair_opens_only_for_failed_fixable_checks() {
        let mut unfixable = check("stale-records", Status::Fail);
        unfixable.fixable = false;
        let checks = vec![
            check("jsonl-integrity", Status::Pass),
            check("legacy-outcome", Status::Warn),
            check("schema-validation", Status::Fail),
            unfixable,
        ];
        // Gate by value: only failed fixable checks open their pass.
        // Catches a gate that ignores `fixable` (the unfixable stale
        // check must stay closed) or matches the NAME only loosely.
        assert!(!Repair::JsonlIntegrity.opens(&checks));
        assert!(Repair::LegacyOutcome.opens(&checks));
        assert!(Repair::SchemaValidation.opens(&checks));
        // failed but not fixable: the pass stays closed
        assert!(!Repair::StaleRecords.opens(&checks));
        // the sprint-24 checks gate like the rest
        assert!(!Repair::OrphanedDomains.opens(&checks));
        assert!(!Repair::FileAnchors.opens(&checks));
        assert!(Repair::OrphanedDomains.opens(&[check("orphaned-domains", Status::Warn)]));
        assert!(!Repair::FileAnchors.opens(&[check("orphaned-domains", Status::Warn)]));
        // The name source: ALL SIX builders mint from Repair, so the
        // mint and the gate cannot drift apart (the silent-disable
        // class, mulch-d343). Catches ANY builder reverting to a
        // literal that differs from the gate's key — two of six were
        // not enough (review finding: the other four could drift
        // again with no test failing).
        let (dir, store) = temp_store("name-source");
        let rule = StaleRule::from_shelf_life(&store.config().effective_shelf_life());
        let root = std::path::Path::new("/tmp");
        assert_eq!(
            jsonl_integrity(&[]).name,
            Repair::JsonlIntegrity.check_name()
        );
        assert_eq!(legacy_outcome(&[]).name, Repair::LegacyOutcome.check_name());
        assert_eq!(
            schema_validation(store.registry(), &[]).name,
            Repair::SchemaValidation.check_name()
        );
        assert_eq!(
            stale_records(&rule, &[]).name,
            Repair::StaleRecords.check_name()
        );
        assert_eq!(
            orphaned_domains(&store).name,
            Repair::OrphanedDomains.check_name()
        );
        assert_eq!(
            file_anchors(&[], root).name,
            Repair::FileAnchors.check_name()
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A file-anchors detail fixture: two broken files, a broken dir
    /// anchor, and a broken evidence file on an id-carrying record,
    /// plus the id-less and empty-string edges.
    fn anchors_store(tag: &str) -> std::path::PathBuf {
        let (dir, store) = temp_store(tag);
        std::fs::write(dir.join("present.txt"), "x").expect("anchor target");
        std::fs::write(
            store.domain_path("d"),
            concat!(
                r#"{"type":"pattern","name":"a","description":"d","id":"mx-abc123","files":["present.txt","gone.txt"],"#,
                r#""dir_anchors":["gone-dir"],"evidence":{"commit":"c","file":"gone.md"}}"#,
                '\n',
                r#"{"type":"pattern","name":"b","description":"d","files":["","/abs/gone"]}"#,
                '\n',
            ),
        )
        .expect("domain file");
        dir
    }

    #[test]
    fn file_anchors_details_follow_reference_order_and_id_rendering() {
        let dir = anchors_store("anchors-check");
        let store = match mulch::StoreFiles::locate(&dir).expect("store opens") {
            mulch::StoreLocation::Open(store) => store,
            other => panic!("store must open, got {other:?}"),
        };
        let domains = read_domains(&store);
        let check = file_anchors(&domains, &dir);
        assert_eq!(check.name, "file-anchors");
        assert_eq!(check.status.as_str(), "warn");
        assert_eq!(check.message, "4 broken file anchor(s) found");
        assert!(check.fixable);
        assert_eq!(check.details, [
            "d[mx-abc123]: files[] path not found: gone.txt",
            "d[mx-abc123]: dir_anchors[] path not found: gone-dir",
            "d[mx-abc123]: evidence.file not found: gone.md",
            // id-less records carry no brackets; the empty string
            // resolves to the project root (exists, unbroken);
            // absolute anchors stay absolute
            "d: files[] path not found: /abs/gone",
        ]);
        // removing an anchor target breaks it retroactively
        std::fs::remove_file(dir.join("present.txt")).expect("cleanup");
        let domains = read_domains(&store);
        let check = file_anchors(&domains, &dir);
        assert_eq!(
            check.details[0],
            "d[mx-abc123]: files[] path not found: present.txt"
        );
    }

    #[test]
    fn strip_broken_anchors_filters_drops_keys_and_keeps_order() {
        let (dir, _store) = temp_store("anchors-strip");
        std::fs::write(dir.join("present.txt"), "x").expect("anchor target");
        let mut carried = record(
            r#"{"type":"pattern","name":"a","description":"d","files":["gone.txt","present.txt"],"dir_anchors":["gone"],"evidence":{"file":"gone.md","commit":"c"}}"#,
        );
        assert_eq!(strip_broken_anchors(&mut carried, &dir), 3);
        assert_eq!(
            serde_json::to_string(&carried).expect("serializes"),
            r#"{"type":"pattern","name":"a","description":"d","files":["present.txt"],"evidence":{"commit":"c"}}"#
        );
        // an all-broken array drops its key entirely; an empty array
        // stays untouched (nothing was removed)
        let mut emptied = record(
            r#"{"type":"pattern","name":"a","description":"d","files":["gone.txt"],"dir_anchors":[]}"#,
        );
        assert_eq!(strip_broken_anchors(&mut emptied, &dir), 1);
        assert_eq!(
            serde_json::to_string(&emptied).expect("serializes"),
            r#"{"type":"pattern","name":"a","description":"d","dir_anchors":[]}"#
        );
    }

    #[test]
    fn anchors_pass_rewrites_and_reports_the_count() {
        let (dir, store) = temp_store("anchors-pass");
        std::fs::write(dir.join("present.txt"), "x").expect("anchor target");
        std::fs::write(
            store.domain_path("d"),
            concat!(
                r#"{"type":"pattern","name":"a","description":"d","id":"mx-1","files":["gone.txt","present.txt"],"#,
                r#""evidence":{"file":"gone.md","commit":"c"}}"#,
                '\n',
                r#"{"type":"pattern","name":"b","description":"d"}"#,
                '\n',
            ),
        )
        .expect("domain file");
        assert_eq!(
            anchors_pass(&store, "d", &dir).expect("pass runs"),
            Some(2),
            "one files entry plus the evidence file"
        );
        // keys keep their positions (only broken anchors move); the
        // id-less survivor gets write_records' deterministic id
        // appended (byte-pinned differentially in the battery)
        let rewritten = read_domain(&store, "d");
        let lines: Vec<&str> = rewritten
            .split('\n')
            .filter(|line| !line.is_empty())
            .collect();
        assert_eq!(lines.len(), 2, "both records survive");
        assert_eq!(
            lines[0],
            r#"{"type":"pattern","name":"a","description":"d","id":"mx-1","files":["present.txt"],"evidence":{"commit":"c"}}"#
        );
        assert!(
            lines[1].starts_with(r#"{"type":"pattern","name":"b","description":"d","id":"mx-"#),
            "id backfilled: {}",
            lines[1]
        );
    }

    #[test]
    fn id_bracket_follows_js_truthiness_and_coercion() {
        assert_eq!(id_bracket(None), "");
        assert_eq!(id_bracket(record(r#"{"v":null}"#).get("v")), "");
        assert_eq!(id_bracket(record(r#"{"v":""}"#).get("v")), "");
        assert_eq!(id_bracket(record(r#"{"v":0}"#).get("v")), "");
        assert_eq!(id_bracket(record(r#"{"v":false}"#).get("v")), "");
        assert_eq!(id_bracket(record(r#"{"v":5}"#).get("v")), "[5]");
        assert_eq!(id_bracket(record(r#"{"v":1.5}"#).get("v")), "[1.5]");
        assert_eq!(id_bracket(record(r#"{"v":true}"#).get("v")), "[true]");
        assert_eq!(
            id_bracket(record(r#"{"v":"mx-1"}"#).get("v")),
            "[mx-1]",
            "the schema's id shape renders as-is"
        );
        // the jsval seam's general truthiness: arrays and objects ARE
        // truthy — a hand-corrupted id renders its coercion like the
        // reference's template literal (mulch-ec71; the inline match
        // used to suppress these unreachable-by-schema shapes)
        assert_eq!(id_bracket(record(r#"{"v":[1]}"#).get("v")), "[1]");
        assert_eq!(
            id_bracket(record(r#"{"v":{}}"#).get("v")),
            "[[object Object]]"
        );
    }

    #[test]
    fn orphaned_domains_flags_both_directions_in_reference_order() {
        let (dir, store) = temp_store("orphaned-check");
        std::fs::write(store.domain_path("d"), "").expect("live file");
        let clean = orphaned_domains(&store);
        assert_eq!(clean.status.as_str(), "pass");
        assert_eq!(clean.message, "No orphaned domains");
        assert!(clean.fixable);
        std::fs::write(dir.join(".mulch/expertise/orphan.jsonl"), "").expect("orphan file");
        std::fs::remove_file(store.domain_path("d")).expect("live file gone");
        let check = orphaned_domains(&store);
        assert_eq!(check.status.as_str(), "warn");
        assert_eq!(check.message, "2 orphaned domain issue(s)");
        assert_eq!(check.details, [
            "File \"orphan.jsonl\" exists but domain \"orphan\" is not in config",
            "Domain \"d\" in config but no JSONL file exists",
        ]);
    }

    #[test]
    fn orphaned_fix_adopts_unvalidated_creates_files_and_rewrites_once() {
        let (dir, mut store) = temp_store("orphaned-fix");
        // d is in the config but has no file; "we ird" is an orphan
        // file whose name register_domain would reject
        std::fs::write(dir.join(".mulch/expertise/we ird.jsonl"), "").expect("orphan file");
        let rule = StaleRule::from_shelf_life(&store.config().effective_shelf_life());
        let checks = vec![check("orphaned-domains", Status::Warn)];
        let fixes = apply_fixes(&mut store, &rule, &[], &checks, &dir).expect("fix pass runs");
        assert_eq!(fixes, [
            "Added orphaned domain \"we ird\" to config",
            "Created missing JSONL file for domain \"d\"",
        ]);
        assert_eq!(
            std::fs::read_to_string(dir.join(".mulch/mulch.config.yaml")).expect("config"),
            "version: '1'\ndomains:\n  d: {}\n  we ird: {}\n\
governance:\n  max_entries: 100\n  warn_entries: 150\n  hard_limit: 200\n\
classification_defaults:\n  shelf_life:\n    tactical: 14\n    observational: 30\n",
            "adopted domains append in config order, unvalidated; the \
canonical rewrite backfills the default blocks (pinned parity)"
        );
        assert!(store.domain_path("d").is_file());
        assert!(store.domain_path("we ird").is_file());
    }
}
