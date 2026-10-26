//! The audit engine (reference utils/audit.ts, ml 0.10.7): corpus
//! health metrics — evidence coverage, convention rule density,
//! floater rate, tracker citations, per-domain mix — plus the
//! remediation suggestions. Laws, not wordings: the channel
//! rendering lives in the CLI's `commands/audit`.
//!
//! Number laws pinned from the reference: signal percentages round
//! (`Math.round`), coverage percentages floor (`Math.floor`), and
//! report numbers serialize as JS numbers (integral doubles print
//! bare). The deprecated back-compat shapes (`evidence.with_seeds`,
//! `seed_citations`) are part of the pinned JSON and ported
//! faithfully.

use std::collections::HashMap;
use std::path::Path;

use jiff::Timestamp;
use serde_json::{Map, Value};

use crate::config::{AuditConfig, ResolvedAuditThresholds, resolve_audit_thresholds};
use crate::prime_ranking::TRACKERS;
use crate::records::LineRecord;
use crate::stale::{StaleRule, StaleVerdict};
use crate::store_files::StoreFiles;
use crate::{Config, Result, truthy};

/// Words that indicate a convention encodes a rule or rationale
/// rather than restating code structure (mirrors the Python
/// prototype's RULE_SIGNALS so audit baselines transfer 1:1). Two
/// entries carry a TRAILING space ("always ", "never ").
pub const RULE_SIGNAL_WORDS: [&str; 13] = [
    "because",
    "must not",
    "do not",
    "don't",
    "avoid",
    "always ",
    "never ",
    "prefer",
    "required for",
    "reason:",
    "rationale",
    "so that",
    "otherwise",
];

/// Whether a convention's content carries one of the rule-signal
/// words (case-insensitive substring, JS `toLowerCase` semantics for
/// ASCII words — the reference lowercases the whole text first).
#[must_use]
pub fn has_rule_signal(text: Option<&str>) -> bool {
    let Some(text) = text else {
        return false;
    };
    let lowered = text.to_lowercase();
    RULE_SIGNAL_WORDS.iter().any(|word| lowered.contains(word))
}

/// One metric's band verdict.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// At or over the PASS band.
    Pass,
    /// In the WARN band.
    Warn,
    /// Below (or over) every acceptable band.
    Fail,
}

impl Verdict {
    /// The reference's verdict string.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Pass => "PASS",
            Verdict::Warn => "WARN",
            Verdict::Fail => "FAIL",
        }
    }
}

impl serde::Serialize for Verdict {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// A metric's verdict with its value and thresholds (the reference
/// `MetricVerdict`; the floater rate carries no warn threshold).
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct MetricVerdict {
    /// The band verdict.
    pub verdict:        Verdict,
    /// The metric's value.
    #[serde(serialize_with = "js_f64_field")]
    pub value:          f64,
    /// The PASS-band threshold.
    #[serde(serialize_with = "js_f64_field")]
    pub threshold:      f64,
    /// The WARN-band threshold, when the metric has one.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_f64_option"
    )]
    pub warn_threshold: Option<f64>,
}

/// The per-domain record mix (reference `DomainMix`).
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct DomainMix {
    /// Domain name.
    pub domain:            String,
    /// Record count.
    pub total:             u64,
    /// Type → count, first-seen insertion order.
    pub type_counts:       Map<String, Value>,
    /// `(failure + decision) / total` floored to a whole percent.
    pub high_value_pct:    u64,
    /// Earliest `recorded_at` (string compare), when any record
    /// carries one.
    pub first_recorded_at: Option<String>,
    /// Latest `recorded_at` (string compare).
    pub last_recorded_at:  Option<String>,
}

/// One top-cited tracker id (tracker_citations block).
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct TrackerCitation {
    /// The display id (`seeds` bare, `gh#42`, `linear:X`, `bead:Y`).
    pub id:      String,
    /// The tracker name.
    pub tracker: String,
    /// Citation count.
    pub count:   u64,
    /// Index status, `"?"`, `"MISSING"`, or `"unresolved"`.
    pub status:  String,
    /// Index title, truncated to 55 units, `""` when unindexed.
    pub title:   String,
}

/// One top-cited seeds id (the deprecated seed_citations block —
/// no tracker field, `MISSING` title fallback).
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct SeedCitation {
    /// The bare seeds id.
    pub id:     String,
    /// Citation count.
    pub count:  u64,
    /// Index status or `"MISSING"`.
    pub status: String,
    /// Index title or `"MISSING"`, truncated to 55 units.
    pub title:  String,
}

/// The convention-quality block, present only with ≥1 convention.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct ConventionQuality {
    /// Convention count.
    pub total:                   u64,
    /// Conventions carrying a rule-signal word.
    pub with_rule_signal:        u64,
    /// The rest.
    pub likely_code_restatement: u64,
}

/// The audit report (reference `AuditReport`); field order matches
/// the reference JSON exactly, deprecated shapes included.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct AuditReport {
    /// Store directory's basename.
    pub repo:                  String,
    /// Records read across the audited domains.
    pub total_records:         u64,
    /// The audited domains, config order.
    pub domains:               Vec<String>,
    /// Config-ignored ⊕ CLI-ignored, first occurrence kept.
    pub ignored_domains:       Vec<String>,
    /// Type → count/pct, count-descending (stable).
    pub type_mix:              Vec<TypeMixRow>,
    /// Evidence roll-ups (includes the deprecated `with_seeds`).
    pub evidence:              Evidence,
    /// Convention rule-signal quality, `None` without conventions.
    pub convention_quality:    Option<ConventionQuality>,
    /// The deprecated seeds-only citation shape.
    pub seed_citations:        SeedCitations,
    /// The generalized tracker citation shape.
    pub tracker_citations:     TrackerCitations,
    /// Per-domain mixes, total-descending (stable).
    pub by_domain:             Vec<DomainMix>,
    /// Domains with ≥5 records and <20% high-value records.
    pub weak_domains:          Vec<String>,
    /// The merged global thresholds.
    pub thresholds:            ResolvedAuditThresholds,
    /// The merged thresholds per audited domain.
    pub per_domain_thresholds: Map<String, Value>,
    /// The summary signals.
    pub signals:               Signals,
    /// Metric labels in the FAIL band.
    pub failures:              Vec<String>,
    /// Metric labels in the WARN band.
    pub warnings:              Vec<String>,
}

/// One type-mix row.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct TypeMixRow {
    /// Record type.
    pub r#type: String,
    /// Count.
    pub count:  u64,
    /// `floor(100 * count / total)` percent.
    pub pct:    u64,
}

/// Evidence roll-ups (reference `AuditReport["evidence"]`).
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct Evidence {
    /// Deprecated since v0.10.1 — populated from
    /// `with_tracker.seeds`; readable until v0.11.
    pub with_seeds:       u64,
    /// Per-tracker record counts (all four keys, fixed order).
    pub with_tracker:     WithTracker,
    /// Records with any tracker OR a commit.
    pub with_any_tracker: u64,
    /// Records with a commit evidence.
    pub with_commit:      u64,
    /// Records with a non-empty relates_to.
    pub with_relates:     u64,
    /// Records with no tracker, commit, or relates_to.
    pub floaters:         u64,
}

/// The four per-tracker counts, fixed key order.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct WithTracker {
    /// seeds-evidenced records.
    pub seeds:  u64,
    /// gh-evidenced records.
    pub gh:     u64,
    /// linear-evidenced records.
    pub linear: u64,
    /// bead-evidenced records.
    pub bead:   u64,
}

/// The deprecated seeds-only citation block.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct SeedCitations {
    /// Unique seeds ids cited.
    pub unique:           u64,
    /// Status → unique-id count, insertion order.
    pub status_counts:    Map<String, Value>,
    /// Citations missing from `.seeds/issues.jsonl`.
    pub missing_in_seeds: u64,
    /// Top 8 by count (stable).
    pub top_cited:        Vec<SeedCitation>,
}

/// The generalized tracker citation block.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct TrackerCitations {
    /// Unique tracker ids cited.
    pub unique:           u64,
    /// Status → unique-id count, insertion order (unresolved
    /// included for the index-less trackers).
    pub status_counts:    Map<String, Value>,
    /// SEEDS citations missing from `.seeds/issues.jsonl`.
    pub missing_in_index: u64,
    /// Top 8 by count (stable).
    pub top_cited:        Vec<TrackerCitation>,
}

/// The report's summary signals.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Signals {
    /// Evidence coverage (any tracker or commit / total).
    pub evidence_coverage:      MetricVerdict,
    /// Rule density, `None` without conventions.
    pub rule_density:           Option<MetricVerdict>,
    /// Floater rate.
    pub floater_rate:           MetricVerdict,
    /// Per-domain record-count rows, by_domain order.
    pub per_domain_max_records: Vec<DomainRow>,
    /// Per-domain stale-count rows, by_domain order.
    pub per_domain_max_stale:   Vec<DomainRow>,
}

/// One per-domain max row.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct DomainRow {
    /// Domain name.
    pub domain:  String,
    /// The domain's record (or stale) count.
    pub count:   u64,
    /// `PASS` within the threshold, `WARN` over.
    pub verdict: Verdict,
}

/// The remediation action of a suggestion group.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SuggestAction {
    /// Stale records past shelf life.
    Archive,
    /// Conventions without rule-signal language.
    Revise,
    /// Floater records without evidence or relates_to.
    Attribute,
}

impl SuggestAction {
    /// The group header form (uppercased by the renderer).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            SuggestAction::Archive => "archive",
            SuggestAction::Revise => "revise",
            SuggestAction::Attribute => "attribute",
        }
    }
}

/// One remediation group (reference `SuggestionGroup`).
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct SuggestionGroup {
    /// The action.
    pub action:     SuggestAction,
    /// One-line summary.
    pub headline:   String,
    /// Why the group fires.
    pub rationale:  String,
    /// Matching record ids, read order.
    pub record_ids: Vec<String>,
    /// Copy-paste commands (≤2 for archive, ≤10 otherwise).
    pub commands:   Vec<String>,
}

/// The suggest payload (reference `SuggestPayload`).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct SuggestPayload {
    /// archive, revise, attribute — present groups only, that order.
    pub groups: Vec<SuggestionGroup>,
}

/// One tracker reference extracted from a record's evidence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackerRef {
    /// Tracker name.
    pub tracker: &'static str,
    /// Raw evidence value (the id).
    pub id:      String,
}

/// The evidence object of a record, JS-truthy only (the reference's
/// `if (!ev)` treats `""`/`0`/`null` evidence as absent).
fn evidence_of(record: &Value) -> Option<&Value> {
    record.get("evidence").filter(|ev| truthy(ev))
}

/// A record's tracker refs: the four tracker fields that hold a
/// NON-EMPTY STRING (non-string values never cite).
#[must_use]
pub fn extract_tracker_refs(record: &Value) -> Vec<TrackerRef> {
    let Some(ev) = evidence_of(record) else {
        return Vec::new();
    };
    TRACKERS
        .iter()
        .filter_map(|tracker| {
            ev.get(*tracker)
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .map(|id| TrackerRef {
                    tracker,
                    id: id.to_string(),
                })
        })
        .collect()
}

/// Whether any of the four tracker fields is JS-truthy (unlike
/// [`extract_tracker_refs`], a numeric value counts here — the
/// reference's `Boolean(ev.seeds || …)`).
fn record_has_tracker(record: &Value) -> bool {
    evidence_of(record).is_some_and(|ev| TRACKERS.iter().any(|t| ev.get(*t).is_some_and(truthy)))
}

/// Whether a record's `relates_to` is non-empty (JS `.length > 0`:
/// arrays and strings carry a length; other shapes never do).
fn relates_nonempty(record: &Value) -> bool {
    match record.get("relates_to") {
        Some(Value::Array(items)) => !items.is_empty(),
        Some(Value::String(text)) => !text.is_empty(),
        _ => false,
    }
}

/// The floater law: no tracker, no commit, no relates_to (a falsy
/// evidence object collapses to the no-evidence arm).
fn record_is_floater(record: &Value) -> bool {
    let commit = evidence_of(record)
        .and_then(|ev| ev.get("commit"))
        .is_some_and(truthy);
    if record_has_tracker(record) || commit {
        return false;
    }
    !relates_nonempty(record)
}

/// Render a tracker ref for output: seeds keeps the bare id
/// (back-compat with the pre-mulch-1334 shape); gh uses `gh#<id>`
/// with a leading `#` stripped; linear and bead prefix
/// `<tracker>:`.
#[must_use]
pub fn format_tracker_id(tracker: &str, id: &str) -> String {
    match tracker {
        "seeds" => id.to_string(),
        "gh" => format!("gh#{}", id.strip_prefix('#').unwrap_or(id)),
        other => format!("{other}:{id}"),
    }
}

/// One `.seeds/issues.jsonl` row that resolved an id.
#[derive(Clone, Debug)]
struct SeedIndexEntry {
    status: Option<String>,
    title:  Option<String>,
}

/// The seeds index: `.seeds/issues.jsonl` under the project root,
/// parsed leniently per line — empty and `#` lines skipped,
/// malformed rows skipped (the audit is read-only and best-effort),
/// rows without a string id skipped, LATER rows overwrite earlier
/// ones for the same id (Map.set semantics). Only seeds resolves
/// locally; gh/linear/bead citations stay `unresolved`.
fn load_tracker_index(cwd: &Path) -> HashMap<String, SeedIndexEntry> {
    let mut index = HashMap::new();
    let Ok(text) = std::fs::read_to_string(cwd.join(".seeds").join("issues.jsonl")) else {
        return index;
    };
    for line in text.split('\n') {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Ok(row) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        if let Some(id) = row.get("id").and_then(Value::as_str) {
            index.insert(tracker_key("seeds", id), SeedIndexEntry {
                status: row
                    .get("status")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                title:  row.get("title").and_then(Value::as_str).map(str::to_string),
            });
        }
    }
    index
}

/// The index key of one tracker citation.
fn tracker_key(tracker: &str, id: &str) -> String {
    format!("{tracker}:{id}")
}

/// Evidence coverage bands: PASS ≥ threshold, WARN ≥ warn band,
/// else FAIL.
fn classify_evidence_coverage(value: f64, thresholds: &ResolvedAuditThresholds) -> MetricVerdict {
    let verdict = if value >= thresholds.evidence_coverage {
        Verdict::Pass
    } else if value >= thresholds.evidence_coverage_warn {
        Verdict::Warn
    } else {
        Verdict::Fail
    };
    MetricVerdict {
        verdict,
        value,
        threshold: thresholds.evidence_coverage,
        warn_threshold: Some(thresholds.evidence_coverage_warn),
    }
}

/// Rule-density bands: PASS ≥ min, WARN ≥ warn band, else FAIL.
fn classify_rule_density(value: f64, thresholds: &ResolvedAuditThresholds) -> MetricVerdict {
    let verdict = if value >= thresholds.rule_density_min {
        Verdict::Pass
    } else if value >= thresholds.rule_density_warn {
        Verdict::Warn
    } else {
        Verdict::Fail
    };
    MetricVerdict {
        verdict,
        value,
        threshold: thresholds.rule_density_min,
        warn_threshold: Some(thresholds.rule_density_warn),
    }
}

/// Floater-rate bands: PASS ≤ floater_max, WARN ≤ 2× floater_max,
/// else FAIL. The 2× FAIL band is asymmetric on purpose — V1_PLAN
/// expects floaters to drop after attribution gates land, so the
/// FAIL band fires only for flagrantly unattributed corpora.
fn classify_floater_rate(value: f64, thresholds: &ResolvedAuditThresholds) -> MetricVerdict {
    let verdict = if value <= thresholds.floater_max {
        Verdict::Pass
    } else if value <= thresholds.floater_max * 2.0 {
        Verdict::Warn
    } else {
        Verdict::Fail
    };
    MetricVerdict {
        verdict,
        value,
        threshold: thresholds.floater_max,
        warn_threshold: None,
    }
}

/// JS number serialization for report fields (see
/// `config::serialize_js_f64`): integral doubles print bare. The
/// reference-taking signatures are serde's `serialize_with` calling
/// convention.
#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's serialize_with passes field references"
)]
fn js_f64_field<S: serde::Serializer>(
    value: &f64,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    crate::config::serialize_js_f64(value, serializer)
}

/// The Option wrapper of [`js_f64_field`].
#[allow(
    clippy::ref_option,
    reason = "serde's serialize_with passes field references"
)]
fn js_f64_option<S: serde::Serializer>(
    value: &Option<f64>,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    match value {
        Some(value) => js_f64_field(value, serializer),
        None => serializer.serialize_none(),
    }
}

/// `floor((100 * n) / total)`, 0 when total is 0 — the reference's
/// per-branch guards folded into one law (every coverage percent in
/// the report and the human rendering reads it).
#[must_use]
pub fn floor_pct(n: u64, total: u64) -> u64 {
    if total == 0 {
        return 0;
    }
    (100 * n) / total
}

/// The record type key for the mix maps: the reference's `r.type`
/// with `undefined` for a missing/non-string type (a Map key that
/// stringifies as `"undefined"`).
fn type_key(record: &Value) -> &str {
    record
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("undefined")
}

/// Truncate to 55 units like the reference's `String.slice(0, 55)`
/// (UTF-16 code units there; chars here — astral titles differ only
/// at the cut surrogate, out of any battery's reach).
fn cut_55(text: &str) -> String {
    text.chars().take(55).collect()
}

/// Insertion-ordered counter increment: the first insert fixes the
/// key's position (serde preserve_order keeps it on later writes).
/// Values are `Value` numbers because serde_json's Map only
/// serializes `Map<String, Value>`.
fn bump(map: &mut Map<String, Value>, key: &str) {
    let count = count_of(map, key) + 1;
    map.insert(key.to_string(), Value::from(count));
}

/// Reads one ordered-counter entry.
fn count_of(map: &Map<String, Value>, key: &str) -> u64 {
    map.get(key).and_then(Value::as_u64).unwrap_or(0)
}

/// The ordered unique-ref table (the reference's insertion-ordered
/// Map keyed `tracker:id`).
struct RefCounts {
    rows:  Vec<(&'static str, String, u64)>,
    index: HashMap<String, usize>,
}

impl RefCounts {
    fn new() -> Self {
        Self {
            rows:  Vec::new(),
            index: HashMap::new(),
        }
    }

    fn record(&mut self, tracker: &'static str, id: &str) {
        let key = tracker_key(tracker, id);
        if let Some(slot) = self.index.get(&key) {
            if let Some(row) = self.rows.get_mut(*slot) {
                row.2 += 1;
            }
            return;
        }
        self.index.insert(key, self.rows.len());
        self.rows.push((tracker, id.to_string(), 1));
    }
}

/// Computes the audit report over the store's configured domains
/// (reference `computeAudit`). Unknown record types are ALLOWED (the
/// reference reads with `allowUnknownTypes: true`); a malformed line
/// is an error (the reference's reader throws).
///
/// # Errors
///
/// The strict read's [`crate::Error`] (malformed line, scalar line),
/// and the audit config block's parse error.
pub fn compute_audit(
    store: &StoreFiles,
    cwd: &Path,
    domain: Option<&str>,
    cli_ignored: &[String],
    now: Timestamp,
) -> Result<AuditReport> {
    let config = store.config();
    let audit_cfg = config.audit()?;
    let (domains, ignored) = select_domains(config, audit_cfg.as_ref(), domain, cli_ignored);

    // basename(resolve(cwd)) || cwd
    let repo = cwd.file_name().map_or_else(
        || cwd.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );

    let items = read_domain_records(store, &domains)?;
    let records: Vec<&Value> = items.iter().map(|(_, record)| record).collect();
    let total = records.len() as u64;

    let thresholds = resolve_audit_thresholds(audit_cfg.as_ref(), domain);
    let per_domain: Vec<(String, ResolvedAuditThresholds)> = domains
        .iter()
        .map(|d| {
            (
                d.clone(),
                resolve_audit_thresholds(audit_cfg.as_ref(), Some(d.as_str())),
            )
        })
        .collect();
    let per_domain_thresholds: Map<String, Value> = per_domain
        .iter()
        .map(|(d, t)| {
            (
                d.clone(),
                serde_json::to_value(t).expect("thresholds serialize"),
            )
        })
        .collect();

    // Type mix: first-seen insertion order, stable count-descending.
    let mut type_counts: Map<String, Value> = Map::new();
    for record in &records {
        bump(&mut type_counts, type_key(record));
    }
    let mut type_mix: Vec<TypeMixRow> = type_counts
        .iter()
        .map(|(r#type, count)| TypeMixRow {
            r#type: r#type.clone(),
            count:  count.as_u64().unwrap_or(0),
            pct:    floor_pct(count.as_u64().unwrap_or(0), total),
        })
        .collect();
    type_mix.sort_by_key(|row| std::cmp::Reverse(row.count));

    // Evidence coverage.
    let mut with_tracker = WithTracker::default();
    let mut with_commit = 0;
    let mut with_relates = 0;
    let mut with_any_tracker = 0;
    let mut floaters = 0;
    for record in &records {
        for reference in extract_tracker_refs(record) {
            match reference.tracker {
                "seeds" => with_tracker.seeds += 1,
                "gh" => with_tracker.gh += 1,
                "linear" => with_tracker.linear += 1,
                "bead" => with_tracker.bead += 1,
                other => unreachable!("TRACKERS is closed over {other:?}"),
            }
        }
        if evidence_of(record)
            .and_then(|ev| ev.get("commit"))
            .is_some_and(truthy)
        {
            with_commit += 1;
        }
        if relates_nonempty(record) {
            with_relates += 1;
        }
        if record_has_tracker(record)
            || evidence_of(record)
                .and_then(|ev| ev.get("commit"))
                .is_some_and(truthy)
        {
            with_any_tracker += 1;
        }
        if record_is_floater(record) {
            floaters += 1;
        }
    }

    // Tracker citations (generalized per mulch-1334).
    let tracker_index = load_tracker_index(cwd);
    let mut ref_counts = RefCounts::new();
    for record in &records {
        for reference in extract_tracker_refs(record) {
            ref_counts.record(reference.tracker, &reference.id);
        }
    }
    let mut tracker_status_counts: Map<String, Value> = Map::new();
    let mut tracker_missing = 0_u64;
    for (tracker, id, _) in &ref_counts.rows {
        let hit = tracker_index.get(&tracker_key(tracker, id));
        match hit {
            Some(entry) => bump(
                &mut tracker_status_counts,
                entry.status.as_deref().unwrap_or("?"),
            ),
            None if *tracker == "seeds" => {
                // Seeds is the one tracker with a local index — a
                // miss means the cited id is not in issues.jsonl.
                tracker_missing += 1;
            }
            None => {
                // gh/linear/bead have no local index — bucket into
                // "unresolved" rather than "missing".
                bump(&mut tracker_status_counts, "unresolved");
            }
        }
    }
    let mut tracker_sorted = ref_counts.rows.clone();
    tracker_sorted.sort_by_key(|row| std::cmp::Reverse(row.2));
    let tracker_top_cited: Vec<TrackerCitation> = tracker_sorted
        .iter()
        .take(8)
        .map(|(tracker, id, count)| {
            let hit = tracker_index.get(&tracker_key(tracker, id));
            let status = match hit {
                Some(entry) => entry.status.clone().unwrap_or_else(|| "?".into()),
                None if *tracker == "seeds" => "MISSING".into(),
                None => "unresolved".into(),
            };
            TrackerCitation {
                id: format_tracker_id(tracker, id),
                tracker: (*tracker).to_string(),
                count: *count,
                status,
                title: cut_55(hit.and_then(|e| e.title.as_deref()).unwrap_or("")),
            }
        })
        .collect();

    // Seeds-only back-compat shape (deprecated, populated from the
    // tracker refs so pre-mulch-1334 consumers see exactly the old
    // values).
    let seeds_rows: Vec<(&'static str, String, u64)> = ref_counts
        .rows
        .iter()
        .filter(|(tracker, _, _)| *tracker == "seeds")
        .cloned()
        .collect();
    let mut seeds_status_counts: Map<String, Value> = Map::new();
    let mut seeds_missing = 0_u64;
    for (tracker, id, _) in &seeds_rows {
        match tracker_index.get(&tracker_key(tracker, id)) {
            Some(entry) => bump(
                &mut seeds_status_counts,
                entry.status.as_deref().unwrap_or("?"),
            ),
            None => seeds_missing += 1,
        }
    }
    let mut seeds_sorted = seeds_rows.clone();
    seeds_sorted.sort_by_key(|row| std::cmp::Reverse(row.2));
    let seeds_top_cited: Vec<SeedCitation> = seeds_sorted
        .iter()
        .take(8)
        .map(|(tracker, id, count)| {
            let hit = tracker_index.get(&tracker_key(tracker, id));
            SeedCitation {
                id:     id.clone(),
                count:  *count,
                status: hit
                    .and_then(|e| e.status.clone())
                    .unwrap_or_else(|| "MISSING".into()),
                title:  cut_55(hit.and_then(|e| e.title.as_deref()).unwrap_or("MISSING")),
            }
        })
        .collect();

    // Convention quality.
    let conventions: Vec<&&Value> = records
        .iter()
        .filter(|record| type_key(record) == "convention")
        .collect();
    let conv_quality = (!conventions.is_empty()).then(|| {
        let with_signal = conventions
            .iter()
            .filter(|record| has_rule_signal(record.get("content").and_then(Value::as_str)))
            .count() as u64;
        ConventionQuality {
            total:                   conventions.len() as u64,
            with_rule_signal:        with_signal,
            likely_code_restatement: conventions.len() as u64 - with_signal,
        }
    });

    // Per-domain mix: first-seen type order per domain, string-
    // compared recorded_at extremes. Only domains WITH records
    // enter (the reference groups the read items — an empty
    // domain has none).
    let mut by_domain: Vec<DomainMix> = Vec::new();
    for domain_name in &domains {
        let group: Vec<&Value> = items
            .iter()
            .filter(|(d, _)| d == domain_name)
            .map(|(_, record)| record)
            .collect();
        if group.is_empty() {
            continue;
        }
        let mut counts: Map<String, Value> = Map::new();
        let mut first: Option<&str> = None;
        let mut last: Option<&str> = None;
        for record in &group {
            bump(&mut counts, type_key(record));
            if let Some(ts) = record.get("recorded_at").and_then(Value::as_str) {
                if first.is_none_or(|current| ts < current) {
                    first = Some(ts);
                }
                if last.is_none_or(|current| ts > current) {
                    last = Some(ts);
                }
            }
        }
        let tot = group.len() as u64;
        let hv = count_of(&counts, "failure") + count_of(&counts, "decision");
        by_domain.push(DomainMix {
            domain:            domain_name.clone(),
            total:             tot,
            type_counts:       counts,
            high_value_pct:    floor_pct(hv, tot),
            first_recorded_at: first.map(str::to_string),
            last_recorded_at:  last.map(str::to_string),
        });
    }
    by_domain.sort_by_key(|row| std::cmp::Reverse(row.total));

    let weak_domains: Vec<String> = by_domain
        .iter()
        .filter(|d| d.total >= 5 && d.high_value_pct < 20)
        .map(|d| d.domain.clone())
        .collect();

    // Verdicts.
    let evidence_coverage_rate = if total > 0 {
        (with_any_tracker as f64) / total as f64
    } else {
        0.0
    };
    let floater_rate = if total > 0 {
        (floaters as f64) / total as f64
    } else {
        0.0
    };
    let rule_density_rate = conv_quality
        .filter(|cq| cq.total > 0)
        .map_or(0.0, |cq| cq.with_rule_signal as f64 / cq.total as f64);

    let evidence_coverage = classify_evidence_coverage(evidence_coverage_rate, &thresholds);
    let floater_rate_verdict = classify_floater_rate(floater_rate, &thresholds);
    let rule_density = conv_quality.map(|_| classify_rule_density(rule_density_rate, &thresholds));

    let stale_rule = StaleRule::from_shelf_life(&config.effective_shelf_life());
    let per_domain_max_records: Vec<DomainRow> = by_domain
        .iter()
        .map(|d| {
            let t = per_domain
                .iter()
                .find(|(name, _)| name == &d.domain)
                .map_or(thresholds, |(_, t)| *t);
            DomainRow {
                domain:  d.domain.clone(),
                count:   d.total,
                verdict: if d.total as f64 <= t.max_records_per_domain {
                    Verdict::Pass
                } else {
                    Verdict::Warn
                },
            }
        })
        .collect();
    let per_domain_max_stale: Vec<DomainRow> = by_domain
        .iter()
        .map(|d| {
            let t = per_domain
                .iter()
                .find(|(name, _)| name == &d.domain)
                .map_or(thresholds, |(_, t)| *t);
            let group = items
                .iter()
                .filter(|(name, _)| name == &d.domain)
                .filter(|(_, record)| {
                    matches!(stale_rule.verdict(record, now), StaleVerdict::Stale { .. })
                })
                .count() as u64;
            DomainRow {
                domain:  d.domain.clone(),
                count:   group,
                verdict: if group as f64 <= t.max_stale {
                    Verdict::Pass
                } else {
                    Verdict::Warn
                },
            }
        })
        .collect();

    let mut failures: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut track = |label: &str, verdict: Option<MetricVerdict>| {
        if let Some(metric) = verdict {
            match metric.verdict {
                Verdict::Fail => failures.push(label.to_string()),
                Verdict::Warn => warnings.push(label.to_string()),
                Verdict::Pass => {}
            }
        }
    };
    track("evidence_coverage", Some(evidence_coverage));
    track("rule_density", rule_density);
    track("floater_rate", Some(floater_rate_verdict));
    for row in &per_domain_max_records {
        if row.verdict == Verdict::Warn {
            warnings.push(format!("max_records_per_domain[{}]", row.domain));
        }
    }
    for row in &per_domain_max_stale {
        if row.verdict == Verdict::Warn {
            warnings.push(format!("max_stale[{}]", row.domain));
        }
    }

    Ok(AuditReport {
        repo,
        total_records: total,
        domains,
        ignored_domains: ignored,
        type_mix,
        evidence: Evidence {
            with_seeds: with_tracker.seeds,
            with_tracker,
            with_any_tracker,
            with_commit,
            with_relates,
            floaters,
        },
        convention_quality: conv_quality,
        seed_citations: SeedCitations {
            unique:           seeds_rows.len() as u64,
            status_counts:    seeds_status_counts,
            missing_in_seeds: seeds_missing,
            top_cited:        seeds_top_cited,
        },
        tracker_citations: TrackerCitations {
            unique:           ref_counts.rows.len() as u64,
            status_counts:    tracker_status_counts,
            missing_in_index: tracker_missing,
            top_cited:        tracker_top_cited,
        },
        by_domain,
        weak_domains,
        thresholds,
        per_domain_thresholds,
        signals: Signals {
            evidence_coverage,
            rule_density,
            floater_rate: floater_rate_verdict,
            per_domain_max_records,
            per_domain_max_stale,
        },
        failures,
        warnings,
    })
}

/// Builds the remediation payload (reference `buildSuggestions`):
/// stale non-foundational records archive, conventions without
/// rule-signal language revise, floaters attribute.
///
/// # Errors
///
/// The strict read's [`crate::Error`], and the audit config block's
/// parse error.
pub fn build_suggestions(
    store: &StoreFiles,
    domain: Option<&str>,
    cli_ignored: &[String],
    now: Timestamp,
) -> Result<SuggestPayload> {
    let config = store.config();
    let audit_cfg = config.audit()?;
    let (domains, _) = select_domains(config, audit_cfg.as_ref(), domain, cli_ignored);
    let items = read_domain_records(store, &domains)?;
    let stale_rule = StaleRule::from_shelf_life(&config.effective_shelf_life());

    let mut archive: Vec<String> = Vec::new();
    let mut revise: Vec<String> = Vec::new();
    let mut attribute: Vec<String> = Vec::new();
    for (_, record) in &items {
        let Some(id) = record
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
        else {
            continue;
        };
        let classification = record.get("classification").and_then(Value::as_str);
        if classification != Some("foundational")
            && matches!(stale_rule.verdict(record, now), StaleVerdict::Stale { .. })
        {
            archive.push(id.to_string());
        }
        if type_key(record) == "convention"
            && !has_rule_signal(record.get("content").and_then(Value::as_str))
        {
            revise.push(id.to_string());
        }
        if record_is_floater(record) {
            attribute.push(id.to_string());
        }
    }

    let mut groups: Vec<SuggestionGroup> = Vec::new();
    if !archive.is_empty() {
        let ids = archive.join(",");
        groups.push(SuggestionGroup {
            action:     SuggestAction::Archive,
            headline:   format!("{} stale records past shelf life", archive.len()),
            rationale:  "These records are past their classification shelf life and are not \
                         foundational. Soft-archive them with `mulch prune` (or `mulch archive` \
                         once it lands in v0.10)."
                .into(),
            record_ids: archive.clone(),
            commands:   vec![
                format!("mulch prune --ids {ids} --dry-run"),
                format!("mulch prune --ids {ids}"),
            ],
        });
    }
    if !revise.is_empty() {
        groups.push(SuggestionGroup {
            action:     SuggestAction::Revise,
            headline:   format!("{} conventions lack rule-signal language", revise.len()),
            rationale:  "Conventions without rule-signal words (because, must not, avoid, \
                         always, never, …) often restate code rather than encode a rule. \
                         Revise the content or downgrade to a note."
                .into(),
            record_ids: revise.clone(),
            commands:   revise
                .iter()
                .take(10)
                .map(|id| format!("mulch edit {id} --content \"...\""))
                .collect(),
        });
    }
    if !attribute.is_empty() {
        groups.push(SuggestionGroup {
            action:     SuggestAction::Attribute,
            headline:   format!(
                "{} records have no evidence or relates_to link",
                attribute.len()
            ),
            rationale:  "Floater records lack any tracker (seeds/gh/linear/bead), commit, or \
                         relates_to link. Attribute them so future audits and pruning can \
                         follow the trail."
                .into(),
            record_ids: attribute.clone(),
            commands:   attribute
                .iter()
                .take(10)
                .map(|id| {
                    format!(
                        "mulch edit {id} --evidence-seeds <id>   # or --evidence-commit <sha> / \
                         --relates-to <mx-id>"
                    )
                })
                .collect(),
        });
    }
    Ok(SuggestPayload { groups })
}

/// The domain selection both walks share: `--domain` scopes to the
/// one domain; otherwise the config domains minus the ignored
/// (config-ignored ⊕ CLI-ignored, first occurrence kept — the
/// reference's Set union order).
fn select_domains(
    config: &Config,
    audit_cfg: Option<&AuditConfig>,
    domain: Option<&str>,
    cli_ignored: &[String],
) -> (Vec<String>, Vec<String>) {
    let all: Vec<String> = config.domains().into_iter().map(str::to_string).collect();
    let mut ignored: Vec<String> = Vec::new();
    for name in audit_cfg
        .iter()
        .flat_map(|cfg| cfg.ignore_domains.iter().flatten())
    {
        if !ignored.iter().any(|seen| seen == name) {
            ignored.push(name.clone());
        }
    }
    for name in cli_ignored {
        if !ignored.iter().any(|seen| seen == name) {
            ignored.push(name.clone());
        }
    }
    let domains = match domain {
        Some(domain) => all
            .iter()
            .filter(|d| d.as_str() == domain)
            .cloned()
            .collect(),
        None => all
            .iter()
            .filter(|d| !ignored.iter().any(|ignored| ignored.as_str() == d.as_str()))
            .cloned()
            .collect(),
    };
    (domains, ignored)
}

/// Reads every listed domain's records (unknown types allowed — the
/// reference's `readExpertiseFile` with `allowUnknownTypes: true`),
/// tagged with the domain name, config order.
fn read_domain_records(store: &StoreFiles, domains: &[String]) -> Result<Vec<(String, Value)>> {
    let mut out = Vec::new();
    for domain in domains {
        for line in store.read_records(domain, true)? {
            let LineRecord { record, .. } = line;
            out.push((domain.clone(), record));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn now() -> Timestamp {
        "2026-10-26T12:00:00.000Z".parse().expect("now parses")
    }

    // ---- verdict bands ----

    /// The three metric bands' decision tables, floater boundaries
    /// included at exactly max and 2×max.
    #[test]
    fn verdict_band_boundaries() {
        let thresholds = crate::config::DEFAULT_AUDIT_THRESHOLDS;
        // evidence coverage: PASS ≥ 0.5, WARN ≥ 0.3, FAIL below
        for (value, want) in [
            (0.5, Verdict::Pass),
            (0.49, Verdict::Warn),
            (0.3, Verdict::Warn),
            (0.2999, Verdict::Fail),
        ] {
            assert_eq!(
                classify_evidence_coverage(value, &thresholds).verdict,
                want,
                "evidence coverage {value}"
            );
        }
        // rule density: PASS ≥ 0.25, WARN ≥ 0.15, FAIL below
        for (value, want) in [
            (0.25, Verdict::Pass),
            (0.2499, Verdict::Warn),
            (0.15, Verdict::Warn),
            (0.1, Verdict::Fail),
        ] {
            assert_eq!(
                classify_rule_density(value, &thresholds).verdict,
                want,
                "rule density {value}"
            );
        }
        // floater: PASS ≤ 0.2, WARN ≤ 0.4 (2× max), FAIL above
        for (value, want) in [
            (0.2, Verdict::Pass),
            (0.2001, Verdict::Warn),
            (0.4, Verdict::Warn),
            (0.4001, Verdict::Fail),
            (1.0, Verdict::Fail),
        ] {
            assert_eq!(
                classify_floater_rate(value, &thresholds).verdict,
                want,
                "floater {value}"
            );
        }
        // the floater metric carries no warn threshold; the other
        // two carry theirs
        assert!(
            classify_floater_rate(0.1, &thresholds)
                .warn_threshold
                .is_none()
        );
        assert_eq!(
            classify_evidence_coverage(0.4, &thresholds).warn_threshold,
            Some(0.3)
        );
        assert_eq!(
            classify_rule_density(0.2, &thresholds).warn_threshold,
            Some(0.15)
        );
    }

    // ---- rule-signal words ----

    /// The signal-word table: every word matches (case-insensitive,
    /// trailing-space entries included) and near-misses do not.
    #[test]
    fn rule_signal_word_rows() {
        for word in [
            "because",
            "MUST NOT",
            "Do not",
            "don't",
            "avoid",
            "always ",
            "never ",
            "prefer",
            "required for",
            "reason:",
            "Rationale",
            "so that",
            "Otherwise",
        ] {
            assert!(has_rule_signal(Some(word)), "{word:?} must signal");
        }
        // "always" WITHOUT the trailing space does not match
        assert!(!has_rule_signal(Some("always-on green")));
        assert!(!has_rule_signal(Some("nevertheless")));
        assert!(!has_rule_signal(Some("")));
        assert!(!has_rule_signal(None));
    }

    // ---- tracker classification ----

    /// Floater/tracker classification rows: string refs cite,
    /// truthy non-strings count as tracker presence, empty strings
    /// do not, and relates_to rescues only as a non-empty
    /// array/string.
    #[test]
    fn tracker_and_floater_classification_rows() {
        let record = |evidence: Value| json!({"type": "pattern", "evidence": evidence});
        // seeds string → tracker + citation, not a floater
        let cited = record(json!({"seeds": "mulch-1"}));
        assert_eq!(extract_tracker_refs(&cited).len(), 1);
        assert!(record_has_tracker(&cited));
        assert!(!record_is_floater(&cited));
        // numeric seeds → tracker presence but NO citation
        let numeric = record(json!({"seeds": 5}));
        assert_eq!(extract_tracker_refs(&numeric).len(), 0);
        assert!(record_has_tracker(&numeric));
        assert!(!record_is_floater(&numeric));
        // empty-string seeds → neither
        let empty = record(json!({"seeds": ""}));
        assert_eq!(extract_tracker_refs(&empty).len(), 0);
        assert!(!record_has_tracker(&empty));
        assert!(record_is_floater(&empty));
        // commit alone → any-tracker coverage, not a floater
        let commit = record(json!({"commit": "abc"}));
        assert!(!record_is_floater(&commit));
        assert_eq!(extract_tracker_refs(&commit).len(), 0);
        // falsy evidence object collapses to absent
        let falsy = json!({"type": "pattern", "evidence": ""});
        assert!(evidence_of(&falsy).is_none());
        assert!(record_is_floater(&falsy));
        // relates_to rescues (array and string shapes)
        assert!(!record_is_floater(
            &json!({"type": "p", "relates_to": ["mx-1"]})
        ));
        assert!(!record_is_floater(
            &json!({"type": "p", "relates_to": "mx-1"})
        ));
        assert!(record_is_floater(&json!({"type": "p", "relates_to": []})));
        assert!(record_is_floater(&json!({"type": "p", "relates_to": {}})));
    }

    // ---- format_tracker_id arms ----

    /// The display-id arms: seeds bare, gh with `^#` stripped once,
    /// linear/bead prefixed.
    #[test]
    fn format_tracker_id_arms() {
        assert_eq!(format_tracker_id("seeds", "mulch-1"), "mulch-1");
        assert_eq!(format_tracker_id("gh", "42"), "gh#42");
        assert_eq!(format_tracker_id("gh", "#42"), "gh#42");
        assert_eq!(format_tracker_id("gh", "##42"), "gh##42");
        assert_eq!(format_tracker_id("linear", "ENG-1"), "linear:ENG-1");
        assert_eq!(format_tracker_id("bead", "b1"), "bead:b1");
    }

    // ---- compute over a scratch store ----

    /// A scratch store seeded with a config and per-domain lines.
    fn scratch(
        tag: &str,
        config: &str,
        domains: &[(&str, &str)],
    ) -> (std::path::PathBuf, StoreFiles) {
        let dir =
            std::env::temp_dir().join(format!("mulch-audit-scratch-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(dir.join(".mulch").join("expertise")).expect("dirs");
        std::fs::write(dir.join(".mulch").join("mulch.config.yaml"), config).expect("config");
        for (domain, body) in domains {
            std::fs::write(
                dir.join(".mulch")
                    .join("expertise")
                    .join(format!("{domain}.jsonl")),
                body,
            )
            .expect("domain file");
        }
        match StoreFiles::locate(&dir).expect("store opens") {
            crate::StoreLocation::Open(store) => (dir, store),
            other => panic!("expected an open store, got {other:?}"),
        }
    }

    const AUDIT_CONFIG: &str = "version: '1'\ndomains:\n  dev: {}\n  ops: {}\n";

    fn dev_lines() -> String {
        [
            r#"{"type":"pattern","classification":"tactical","recorded_at":"2026-10-20T10:00:00.000Z","name":"p1","description":"d","id":"mx-a1"}"#,
            r#"{"type":"convention","classification":"foundational","recorded_at":"2026-10-20T10:00:01.000Z","content":"always do X because Y","id":"mx-b2"}"#,
            r#"{"type":"decision","classification":"tactical","recorded_at":"2026-10-20T10:00:02.000Z","title":"t","rationale":"r","id":"mx-c3","evidence":{"seeds":"mulch-1"}}"#,
            r#"{"type":"failure","classification":"tactical","recorded_at":"2026-10-20T10:00:03.000Z","title":"f","resolution":"r","id":"mx-d4","evidence":{"gh":"42"}}"#,
            r#"{"type":"pattern","classification":"tactical","recorded_at":"2026-09-01T10:00:00.000Z","name":"stale","description":"d","id":"mx-e5"}"#,
        ]
        .join("\n")
            + "\n"
    }

    /// The report over a mixed store: counts, deprecated shapes,
    /// ordering laws, and the stale/max rows — the pinned reference
    /// probe values.
    #[test]
    fn compute_audit_over_a_mixed_store() {
        let (root, store) = scratch("mixed", AUDIT_CONFIG, &[("dev", &dev_lines()), ("ops", "")]);
        // a seeds index resolving one id, last-wins on duplicates
        std::fs::create_dir_all(root.join(".seeds")).expect("seeds dir");
        std::fs::write(
            root.join(".seeds").join("issues.jsonl"),
            "# banner\n{\"id\":\"mulch-1\",\"status\":\"closed\",\"title\":\"First seed title\"}\n\
             {\"id\":\"mulch-1\",\"title\":\"LATER WINS\"}\nnot json\n{\"noid\":1}\n",
        )
        .expect("index");
        let report = compute_audit(&store, &root, None, &[], now()).expect("computes");
        assert_eq!(report.repo, root.file_name().unwrap().to_string_lossy());
        assert_eq!(report.total_records, 5);
        assert_eq!(report.domains, vec!["dev".to_string(), "ops".to_string()]);
        // type mix: count-descending, ties keep first-seen order
        assert_eq!(report.type_mix[0].r#type, "pattern");
        assert_eq!(report.type_mix[0].count, 2);
        assert_eq!(report.type_mix[0].pct, 40);
        // evidence roll-ups incl. the deprecated with_seeds
        assert_eq!(report.evidence.with_seeds, 1);
        assert_eq!(report.evidence.with_tracker.gh, 1);
        assert_eq!(report.evidence.with_any_tracker, 2);
        assert_eq!(report.evidence.floaters, 3);
        // seeds index: last row wins (title overwritten), status lost
        let top = &report.tracker_citations.top_cited[0];
        assert_eq!(top.id, "mulch-1");
        assert_eq!(top.status, "?");
        assert_eq!(top.title, "LATER WINS");
        // the seeds back-compat top_cited shares the status law
        assert_eq!(report.seed_citations.unique, 1);
        assert_eq!(report.seed_citations.top_cited[0].title, "LATER WINS");
        // gh has no local index → unresolved
        assert_eq!(
            count_of(&report.tracker_citations.status_counts, "unresolved"),
            1
        );
        // stale tactical (Sep 1 vs Oct 26) → WARN row
        assert_eq!(
            report
                .signals
                .per_domain_max_stale
                .iter()
                .find(|row| row.domain == "dev")
                .map(|row| (row.count, row.verdict)),
            Some((1, Verdict::Warn))
        );
        assert_eq!(report.warnings, vec![
            "evidence_coverage".to_string(),
            "max_stale[dev]".to_string()
        ]);
        assert_eq!(report.failures, vec!["floater_rate".to_string()]);
        std::fs::remove_dir_all(&root).ok();
    }

    /// The seeds MISS laws: an id absent from the index counts
    /// `missing_in_index` and top-cites as MISSING (with the
    /// `MISSING` title fallback unique to the seeds shape).
    #[test]
    fn seeds_citation_miss_laws() {
        let (root, store) = scratch("missing", AUDIT_CONFIG, &[
            (
                "dev",
                concat!(
                    r#"{"type":"decision","classification":"foundational","recorded_at":"2026-10-20T10:00:00.000Z","title":"t","rationale":"r","id":"mx-m1","evidence":{"seeds":"mulch-77"}}"#,
                    "\n",
                    r#"{"type":"decision","classification":"foundational","recorded_at":"2026-10-20T10:00:01.000Z","title":"t2","rationale":"r","id":"mx-m2","evidence":{"linear":"LIN-1"}}"#,
                    "\n"
                ),
            ),
            ("ops", ""),
        ]);
        let report = compute_audit(&store, &root, None, &[], now()).expect("computes");
        assert_eq!(report.tracker_citations.unique, 2);
        assert_eq!(report.tracker_citations.missing_in_index, 1);
        assert_eq!(
            count_of(&report.tracker_citations.status_counts, "unresolved"),
            1
        );
        let seeds_top = &report.seed_citations.top_cited[0];
        assert_eq!(seeds_top.id, "mulch-77");
        assert_eq!(seeds_top.status, "MISSING");
        assert_eq!(seeds_top.title, "MISSING");
        // the tracker shape's miss title is "" (not "MISSING")
        assert_eq!(report.tracker_citations.top_cited[0].title, "");
        // long titles truncate at 55 units
        let (root2, store2) = scratch("cut", AUDIT_CONFIG, &[
            (
                "dev",
                r#"{"type":"decision","classification":"foundational","recorded_at":"2026-10-20T10:00:00.000Z","title":"t","rationale":"r","id":"mx-t1","evidence":{"seeds":"s1"}}"#,
            ),
            ("ops", ""),
        ]);
        std::fs::create_dir_all(root2.join(".seeds")).expect("seeds dir");
        std::fs::write(
            root2.join(".seeds").join("issues.jsonl"),
            format!(
                "{{\"id\":\"s1\",\"status\":\"open\",\"title\":\"{}\"}}\n",
                "T".repeat(80)
            ),
        )
        .expect("index");
        let report2 = compute_audit(&store2, &root2, None, &[], now()).expect("computes");
        assert_eq!(
            report2.tracker_citations.top_cited[0].title.chars().count(),
            55
        );
        std::fs::remove_dir_all(&root).ok();
        std::fs::remove_dir_all(&root2).ok();
    }

    /// The suggest groups: archive/revise/attribute fire in that
    /// order, ids keep read order, and the command slices cap at 10.
    #[test]
    fn suggest_groups_and_slices() {
        let mut lines = vec![
            r#"{"type":"pattern","classification":"tactical","recorded_at":"2026-09-01T10:00:00.000Z","name":"stale","description":"d","id":"mx-s1"}"#.to_string(),
            r#"{"type":"convention","classification":"foundational","recorded_at":"2026-10-20T10:00:00.000Z","content":"code restatement","id":"mx-r1"}"#.to_string(),
        ];
        for i in 0..12 {
            lines.push(
                r#"{"type":"pattern","classification":"foundational","recorded_at":"2026-10-20T10:00:00.000Z","name":"f"#.to_string() + &i.to_string() + r#"","description":"d","id":"mx-f"# + &i.to_string() + r#""}"#,
            );
        }
        let body = lines.join("\n") + "\n";
        let (root, store) = scratch("suggest", AUDIT_CONFIG, &[("dev", &body), ("ops", "")]);
        let payload = build_suggestions(&store, None, &[], now()).expect("suggests");
        let actions: Vec<&str> = payload
            .groups
            .iter()
            .map(|group| group.action.as_str())
            .collect();
        assert_eq!(actions, vec!["archive", "revise", "attribute"]);
        assert_eq!(payload.groups[0].record_ids, vec!["mx-s1"]);
        assert_eq!(payload.groups[1].record_ids, vec!["mx-r1"]);
        // 14 floaters: the stale record, the convention, and the 12
        // fresh patterns, in read order — stale archives mx-s1 AND
        // attribution still lists it (the reference probes both
        // independently)
        assert_eq!(
            payload.groups[2].record_ids.first().map(String::as_str),
            Some("mx-s1")
        );
        assert_eq!(payload.groups[2].record_ids.len(), 14);
        assert_eq!(payload.groups[2].commands.len(), 10);
        assert!(payload.groups[2].commands[0].starts_with("mulch edit mx-s1 "));
        std::fs::remove_dir_all(&root).ok();
    }

    /// The ignore law: config-ignored ⊕ CLI-ignored dedup in that
    /// order, and `--domain` bypasses the filter (scoping law).
    #[test]
    fn ignored_domains_merge_and_domain_scoping() {
        let config = "version: '1'\ndomains:\n  dev: {}\n  ops: {}\naudit:\n  ignore_domains:\n    - ops\n    - dev\n";
        let (root, store) = scratch("ignore", config, &[("dev", &dev_lines()), ("ops", "")]);
        // CLI dup of a config entry collapses; a new one appends
        let cli = vec!["dev".to_string(), "ghost".to_string()];
        let report = compute_audit(&store, &root, None, &cli, now()).expect("computes");
        assert_eq!(report.ignored_domains, vec!["ops", "dev", "ghost"]);
        assert_eq!(report.domains, Vec::<String>::new());
        assert_eq!(report.total_records, 0);
        // --domain scopes to the one domain regardless of ignore
        let scoped = compute_audit(&store, &root, Some("ops"), &cli, now()).expect("computes");
        assert_eq!(scoped.domains, vec!["ops".to_string()]);
        // per-domain thresholds carry the per-domain override
        let override_config = "version: '1'\ndomains:\n  dev: {}\naudit:\n  per_domain:\n    dev:\n      max_stale: 5\n";
        let (root2, store2) = scratch("per-domain", override_config, &[("dev", &dev_lines())]);
        let report2 = compute_audit(&store2, &root2, None, &[], now()).expect("computes");
        assert_eq!(report2.per_domain_thresholds["dev"]["max_stale"], json!(5));
        // the stale WARN flips to PASS under the override
        assert_eq!(
            report2
                .signals
                .per_domain_max_stale
                .first()
                .map(|row| row.verdict),
            Some(Verdict::Pass)
        );
        std::fs::remove_dir_all(&root).ok();
        std::fs::remove_dir_all(&root2).ok();
    }

    /// The report's serialized key order matches the reference JSON
    /// (pinned from the ml 0.10.7 probe), and integral doubles print
    /// bare.
    #[test]
    fn report_serializes_in_the_reference_key_order() {
        let (root, store) = scratch("order", AUDIT_CONFIG, &[("dev", &dev_lines()), ("ops", "")]);
        let report = compute_audit(&store, &root, None, &[], now()).expect("computes");
        let value = serde_json::to_value(&report).expect("serializes");
        let keys: Vec<&str> = value
            .as_object()
            .expect("report is an object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, vec![
            "repo",
            "total_records",
            "domains",
            "ignored_domains",
            "type_mix",
            "evidence",
            "convention_quality",
            "seed_citations",
            "tracker_citations",
            "by_domain",
            "weak_domains",
            "thresholds",
            "per_domain_thresholds",
            "signals",
            "failures",
            "warnings",
        ]);
        let evidence_keys: Vec<&str> = value["evidence"]
            .as_object()
            .expect("evidence object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(evidence_keys, vec![
            "with_seeds",
            "with_tracker",
            "with_any_tracker",
            "with_commit",
            "with_relates",
            "floaters",
        ]);
        // integral doubles print bare (0, not 0.0)
        assert_eq!(value["thresholds"]["max_stale"], json!(0));
        assert_eq!(value["signals"]["floater_rate"]["threshold"], json!(0.2));
        // empty domains stay in domains/per_domain_thresholds but
        // never in by_domain (the reference groups read items)
        assert_eq!(
            value["per_domain_thresholds"]
                .as_object()
                .expect("pdt object")
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            vec!["dev", "ops"]
        );
        assert_eq!(
            value["by_domain"]
                .as_array()
                .expect("by_domain array")
                .iter()
                .map(|d| d["domain"].as_str().expect("domain"))
                .collect::<Vec<_>>(),
            vec!["dev"]
        );
        // weak-domains renders as JSON.stringify would
        assert_eq!(
            serde_json::to_string(&report.weak_domains).expect("weak json"),
            "[]"
        );
        std::fs::remove_dir_all(&root).ok();
    }
}
