//! `mulch audit` — corpus health audit (reference commands/audit.ts,
//! ml 0.10.7). Channel laws: no store / unknown domain → plain red
//! stderr line, or the JSON error envelope on stderr when `--json`
//! OR `--ci` (the reference's `outputJsonError`), exit 1; `--ci`
//! implies the JSON envelope on STDOUT with `success` mirroring the
//! failures and exit 1 only when failures exist; plain `--json`
//! always reports `success: true`; human mode prints the report
//! (plus the suggest block after a blank line and the bold header
//! when `--suggest`).
//!
//! Colors: the reference chalk-colors PASS/WARN/FAIL tags, the
//! brand header and the suggest lines — chalk disables itself on
//! non-tty streams, and this port ships no ANSI at all (probed: the
//! reference piped emits plain bytes; our established convention,
//! see doctor/onboard).

use mulch::StoreLocation;
use mulch::audit::{AuditReport, SuggestAction, SuggestPayload, floor_pct};
use serde_json::{Map, Value};

use crate::cli::GlobalOpts;
use crate::commands::failure::render_core_error;
use crate::commands::store::locate_error;
use crate::output::{Failure, failure_envelope, print_json, success_envelope};

/// The no-store sentence (audit.ts `existsSync` gate).
const NO_STORE: &str = "No .mulch/ directory found. Run `mulch init` first.";

/// Runs `audit` from the current directory.
pub(super) fn run(
    opts: &GlobalOpts,
    ci: bool,
    suggest: bool,
    domain: Option<&str>,
    ignore_domains: Option<&str>,
) -> Result<(), Failure> {
    let cwd = crate::commands::store::cwd("audit")?;
    let store = match mulch::StoreFiles::locate(&cwd) {
        Ok(StoreLocation::Open(store)) => store,
        // The reference gates on the .mulch/ DIRECTORY existing;
        // a directory without a config crashes its config reader —
        // both fold onto the pinned sentence here (clean error).
        Ok(StoreLocation::Missing | StoreLocation::NoConfig) => {
            return early_error(opts, ci, NO_STORE);
        }
        Err(source) => return Err(locate_error("audit", &source)),
    };

    let cli_ignored: Vec<String> = ignore_domains.map_or_else(Vec::new, |list| {
        list.split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_string)
            .collect()
    });

    if let Some(name) = domain
        && !store.config().domains().contains(&name)
    {
        return early_error(opts, ci, &format!("Domain \"{name}\" not found in config."));
    }

    let now = jiff::Timestamp::now();
    let report = mulch::audit::compute_audit(&store, &cwd, domain, &cli_ignored, now)
        .map_err(|source| Failure::handled_on_stderr("audit", render_core_error(&source)))?;
    let suggest_payload = if suggest {
        Some(
            mulch::audit::build_suggestions(&store, domain, &cli_ignored, now).map_err(
                |source| Failure::handled_on_stderr("audit", render_core_error(&source)),
            )?,
        )
    } else {
        None
    };

    // --ci implies JSON regardless of the global --json flag, and
    // sets the exit code.
    if ci {
        print_json(&ci_envelope(&report, suggest_payload.as_ref()), false);
        if report.failures.is_empty() {
            return Ok(());
        }
        return Err(Failure::rendered_exit("audit"));
    }

    if opts.json {
        print_json(&json_envelope(&report, suggest_payload.as_ref()), false);
        return Ok(());
    }

    #[allow(clippy::print_stdout, reason = "the report renders here")]
    {
        println!("{}", format_human(&report));
        if let Some(payload) = &suggest_payload {
            println!();
            println!("=== suggested remediations ===");
            println!("{}", format_suggest(payload));
        }
    }
    Ok(())
}

/// The no-store / unknown-domain surface: plain red stderr line, or
/// the JSON error envelope on STDERR when `--json` or `--ci`
/// (outputJsonError); exit 1.
fn early_error(opts: &GlobalOpts, ci: bool, message: &str) -> Result<(), Failure> {
    if opts.json {
        return Err(Failure::handled_on_stderr("audit", message));
    }
    if ci {
        let mut fields = Map::new();
        fields.insert("error".into(), Value::String(message.into()));
        print_json(&failure_envelope("audit", fields), true);
        return Err(Failure::rendered_exit("audit"));
    }
    Err(Failure::handled_on_stderr("audit", message))
}

/// The `--ci` envelope: `success` mirrors the failures; field order
/// success, command, report, suggestions (the reference literal).
fn ci_envelope(report: &AuditReport, payload: Option<&SuggestPayload>) -> Value {
    let mut fields = Map::new();
    fields.insert(
        "report".into(),
        serde_json::to_value(report).expect("report serializes"),
    );
    if let Some(payload) = payload {
        fields.insert(
            "suggestions".into(),
            serde_json::to_value(payload).expect("suggestions serialize"),
        );
    }
    if report.failures.is_empty() {
        success_envelope("audit", fields)
    } else {
        failure_envelope("audit", fields)
    }
}

/// The plain `--json` envelope: always `success: true` (the
/// reference's json branch reports the report, not the verdicts).
fn json_envelope(report: &AuditReport, payload: Option<&SuggestPayload>) -> Value {
    let mut fields = Map::new();
    fields.insert(
        "report".into(),
        serde_json::to_value(report).expect("report serializes"),
    );
    if let Some(payload) = payload {
        fields.insert(
            "suggestions".into(),
            serde_json::to_value(payload).expect("suggestions serialize"),
        );
    }
    success_envelope("audit", fields)
}

/// The signal-percent law: `Math.round(n * 100)` (the coverage
/// percents floor instead — see [`coverage_pct`]).
#[allow(
    clippy::cast_possible_truncation,
    reason = "the round() lands the value inside i64 for metric ranges"
)]
fn pct(n: f64) -> String {
    format!("{}%", (n * 100.0).round() as i64)
}

/// The coverage-percent law: `Math.floor((100 * n) / total)`.
fn coverage_pct(n: u64, total: u64) -> String {
    format!("{}%", floor_pct(n, total))
}

/// The human report (formatHuman, byte-pinned against the piped
/// reference).
fn format_human(report: &AuditReport) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("=== mulch-audit: {} ===", report.repo));
    lines.push(String::new());
    lines.push(format!("total records: {}", report.total_records));
    if !report.ignored_domains.is_empty() {
        lines.push(format!(
            "(ignoring domains: {})",
            report.ignored_domains.join(", ")
        ));
    }
    if report.total_records == 0 {
        lines.push(String::new());
        lines.push("no records — skipping metric computation".into());
        return lines.join("\n");
    }

    lines.push(String::new());
    lines.push("type mix:".into());
    for row in &report.type_mix {
        lines.push(format!("  {}: {} ({}%)", row.r#type, row.count, row.pct));
    }

    lines.push(String::new());
    lines.push("evidence coverage:".into());
    let ev = &report.evidence;
    let total = report.total_records;
    lines.push(format!(
        "  with any tracker: {} ({})",
        ev.with_any_tracker,
        coverage_pct(ev.with_any_tracker, total)
    ));
    for (tracker, n) in [
        ("seeds", ev.with_tracker.seeds),
        ("gh", ev.with_tracker.gh),
        ("linear", ev.with_tracker.linear),
        ("bead", ev.with_tracker.bead),
    ] {
        if n > 0 {
            lines.push(format!("  - {tracker}: {n} ({})", coverage_pct(n, total)));
        }
    }
    lines.push(format!(
        "  with commit-ev: {} ({})",
        ev.with_commit,
        coverage_pct(ev.with_commit, total)
    ));
    lines.push(format!("  with relates_to: {}", ev.with_relates));
    lines.push(format!(
        "  FLOATERS (no tracker/relates/commit): {}",
        ev.floaters
    ));

    let refs = &report.tracker_citations;
    if refs.unique > 0 {
        lines.push(String::new());
        lines.push(format!(
            "tracker-citation (unique ids cited: {}):",
            refs.unique
        ));
        // status counts, count-descending (stable — insertion order
        // for ties, serde preserve_order's iteration)
        let mut statuses: Vec<(String, u64)> = refs
            .status_counts
            .iter()
            .map(|(status, count)| (status.clone(), count.as_u64().unwrap_or(0)))
            .collect();
        statuses.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
        for (status, count) in statuses {
            lines.push(format!("  {status}: {count}"));
        }
        if refs.missing_in_index > 0 {
            lines.push(format!(
                "  not-found-in-seeds.jsonl: {}",
                refs.missing_in_index
            ));
        }
        let closed = refs
            .status_counts
            .get("closed")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        lines.push(format!(
            "  closed-citation rate: {}%",
            floor_pct(closed, refs.unique)
        ));
    }

    if let Some(cq) = &report.convention_quality {
        let rate = floor_pct(cq.with_rule_signal, cq.total);
        lines.push(String::new());
        lines.push("convention quality:".into());
        lines.push(format!("  total conventions: {}", cq.total));
        lines.push(format!(
            "  with rule-signal word: {} ({}%)",
            cq.with_rule_signal, rate
        ));
        lines.push(format!(
            "  likely code-restatement: {} ({}%)",
            cq.likely_code_restatement,
            100 - rate
        ));
    }

    lines.push(String::new());
    lines.push("per-domain mix (sorted by record count):".into());
    lines.push(format!(
        "  {:<22} {:>5} {:>4} {:>4} {:>4} {:>4}  high-value%",
        "domain", "total", "fail", "dec", "pat", "conv"
    ));
    for d in &report.by_domain {
        lines.push(format!(
            "  {:<22} {:>5} {:>4} {:>4} {:>4} {:>4}  {:>3}%",
            d.domain,
            d.total,
            d.type_counts
                .get("failure")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            d.type_counts
                .get("decision")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            d.type_counts
                .get("pattern")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            d.type_counts
                .get("convention")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            d.high_value_pct
        ));
    }

    if !report.tracker_citations.top_cited.is_empty() {
        lines.push(String::new());
        lines.push("top-cited tracker ids:".into());
        for t in &report.tracker_citations.top_cited {
            lines.push(format!(
                "  {} cited={} status={}  {}",
                t.id, t.count, t.status, t.title
            ));
        }
    }

    let dated: Vec<&mulch::audit::DomainMix> = report
        .by_domain
        .iter()
        .filter(|d| d.first_recorded_at.is_some())
        .collect();
    if !dated.is_empty() {
        lines.push(String::new());
        lines.push("domain age (first → last recorded_at):".into());
        // DEVIATION (README): the reference sorts with
        // localeCompare, we by bytes — identical for same-case
        // domain names (the diff_cmd precedent).
        let mut sorted = dated.clone();
        sorted.sort_by(|a, b| a.domain.cmp(&b.domain));
        for d in sorted {
            lines.push(format!(
                "  {:<22} {} → {}  ({} records)",
                d.domain,
                d.first_recorded_at
                    .as_deref()
                    .unwrap_or("")
                    .get(..10)
                    .unwrap_or(""),
                d.last_recorded_at
                    .as_deref()
                    .unwrap_or("")
                    .get(..10)
                    .unwrap_or(""),
                d.total
            ));
        }
    }

    lines.push(String::new());
    lines.push("--- summary signals ---".into());
    let s = &report.signals;
    lines.push(format!(
        "evidence coverage: {}  {} (target ≥{})",
        pct(s.evidence_coverage.value),
        s.evidence_coverage.verdict.as_str(),
        pct(s.evidence_coverage.threshold)
    ));
    if let Some(rule_density) = &s.rule_density {
        lines.push(format!(
            "convention rule-density: {}  {} (target ≥{})",
            pct(rule_density.value),
            rule_density.verdict.as_str(),
            pct(rule_density.threshold)
        ));
    }
    lines.push(format!(
        "floater rate: {}  {} (target ≤{})",
        pct(s.floater_rate.value),
        s.floater_rate.verdict.as_str(),
        pct(s.floater_rate.threshold)
    ));
    let warn_records: Vec<&mulch::audit::DomainRow> = s
        .per_domain_max_records
        .iter()
        .filter(|row| row.verdict != mulch::audit::Verdict::Pass)
        .collect();
    let warn_stale: Vec<&mulch::audit::DomainRow> = s
        .per_domain_max_stale
        .iter()
        .filter(|row| row.verdict != mulch::audit::Verdict::Pass)
        .collect();
    if !warn_records.is_empty() {
        let joined: Vec<String> = warn_records
            .iter()
            .map(|row| format!("{}={}", row.domain, row.count))
            .collect();
        lines.push(format!(
            "max_records_per_domain: {} over ({})  WARN",
            warn_records.len(),
            joined.join(", ")
        ));
    }
    if !warn_stale.is_empty() {
        let joined: Vec<String> = warn_stale
            .iter()
            .map(|row| format!("{}={}", row.domain, row.count))
            .collect();
        lines.push(format!(
            "max_stale: {} over ({})  WARN",
            warn_stale.len(),
            joined.join(", ")
        ));
    }
    let weak = serde_json::to_string(&Value::Array(
        report
            .weak_domains
            .iter()
            .map(|d| Value::String(d.clone()))
            .collect(),
    ))
    .expect("weak domains serialize");
    lines.push(format!(
        "weak-domains (≥5 records, high-value <20%): {}  -> {weak}",
        report.weak_domains.len()
    ));

    if report.failures.is_empty() && report.warnings.is_empty() {
        lines.push(String::new());
        lines.push("✓ all signals PASS".into());
    } else {
        lines.push(String::new());
        if !report.failures.is_empty() {
            lines.push(format!("✗ FAIL: {}", report.failures.join(", ")));
        }
        if !report.warnings.is_empty() {
            lines.push(format!("! WARN: {}", report.warnings.join(", ")));
        }
    }
    lines.join("\n")
}

/// The suggest block (formatSuggest): groups with header, muted
/// rationale, record-id preview (12 shown, ` … (+N more)`), and
/// copy-paste commands; the empty payload carries the one clean
/// line.
fn format_suggest(payload: &SuggestPayload) -> String {
    if payload.groups.is_empty() {
        return "✓ no remediation suggestions — corpus looks clean".into();
    }
    let mut lines: Vec<String> = Vec::new();
    for g in &payload.groups {
        lines.push(String::new());
        lines.push(format!("[{}] {}", action_label(g.action), g.headline));
        lines.push(g.rationale.clone());
        lines.push(String::new());
        lines.push("  record ids:".into());
        let preview = &g.record_ids[..g.record_ids.len().min(12)];
        let tail = if g.record_ids.len() > preview.len() {
            format!(" … (+{} more)", g.record_ids.len() - preview.len())
        } else {
            String::new()
        };
        lines.push(format!("    {}{}", preview.join(", "), tail));
        lines.push(String::new());
        lines.push("  copy-paste commands:".into());
        for cmd in &g.commands {
            lines.push(format!("    {cmd}"));
        }
    }
    lines.join("\n")
}

/// The group header form: `action.toUpperCase()`.
fn action_label(action: SuggestAction) -> &'static str {
    match action {
        SuggestAction::Archive => "ARCHIVE",
        SuggestAction::Revise => "REVISE",
        SuggestAction::Attribute => "ATTRIBUTE",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two percent laws: signals ROUND (37.5 → 38), coverage
    /// percents FLOOR (0.375 of a 8-record corpus prints 37).
    #[test]
    fn percent_laws_round_and_floor() {
        assert_eq!(pct(0.375), "38%");
        assert_eq!(pct(0.5), "50%");
        assert_eq!(pct(0.205), "21%", "0.205*100 rounds half-away");
        assert_eq!(coverage_pct(3, 8), "37%");
        assert_eq!(coverage_pct(0, 8), "0%");
        assert_eq!(coverage_pct(5, 0), "0%");
    }

    /// The suggest preview slice: exactly 12 shown, the tail counts
    /// the rest with the leading-space ellipsis.
    #[test]
    fn suggest_preview_shows_twelve_then_ellipsized_tail() {
        let ids: Vec<String> = (0..14).map(|i| format!("mx-{i}")).collect();
        let payload = SuggestPayload {
            groups: vec![mulch::audit::SuggestionGroup {
                action:     SuggestAction::Attribute,
                headline:   "h".into(),
                rationale:  "r".into(),
                record_ids: ids,
                commands:   vec!["c".into()],
            }],
        };
        let text = format_suggest(&payload);
        assert!(text.contains(
            "mx-0, mx-1, mx-2, mx-3, mx-4, mx-5, mx-6, mx-7, mx-8, mx-9, \
             mx-10, mx-11 … (+2 more)"
        ));
        assert!(!text.contains("mx-12,"));
        // exactly 12 ids stay visible
        let ids_line = text
            .lines()
            .find(|l| l.starts_with("    mx-"))
            .expect("ids line");
        assert_eq!(ids_line.matches("mx-").count(), 12);
    }

    /// The empty payload renders the single clean line.
    #[test]
    fn suggest_empty_payload_is_the_clean_line() {
        assert_eq!(
            format_suggest(&SuggestPayload::default()),
            "✓ no remediation suggestions — corpus looks clean"
        );
    }
}
