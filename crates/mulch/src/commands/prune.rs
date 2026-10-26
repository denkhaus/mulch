//! `mulch prune` — shelf-life decay plus the supersession-demotion
//! ladder (mulch-7c55): soft-archive stale records (or hard-delete
//! them), demote superseded and anchor-decayed records one tier per
//! pass — or straight to the archive with `--aggressive`.
//!
//! `pre-prune` hooks (mulch-7f49) run between the preview and the
//! write phases, block-or-allow: skipped in `--dry-run` and when no
//! domain produced candidates; the payload carries the raw candidate
//! records per bucket, a non-zero exit (or timeout) aborts before
//! any write, and the hook's stdout is IGNORED (no mutation even
//! though it is a `pre-*` event).

use std::collections::{HashMap, HashSet};
use std::path::Path;

use jiff::Timestamp;
use mulch::anchors::AnchorReport;
use mulch::stale::{StaleRule, StaleVerdict};
use mulch::{js_number, timestamp_ms, value_text};
use serde_json::{Map, Value};
use serde_yaml::Value as Yaml;

use crate::cli::{GlobalOpts, PruneArgs};
use crate::commands::hooks_surface::{HookPhase, print_hook_warnings};
use crate::output::{Failure, print_json, print_line, success_envelope};

/// Reference default for `decay.anchor_validity.threshold`
/// (schemas/config.ts).
const DEFAULT_ANCHOR_THRESHOLD: f64 = 0.5;

/// Reference default for `decay.anchor_validity.grace_days`.
const DEFAULT_ANCHOR_GRACE_DAYS: f64 = 7.0;

/// Milliseconds per day (the reference's day arithmetic).
const MS_PER_DAY: f64 = 86_400_000.0;

/// The demotion reasons, in the reference's stamp order.
const SUPERSEDED: &str = "superseded";
const ANCHOR_DECAY: &str = "anchor_decay";

/// Runs `prune`.
pub(super) fn run(opts: &GlobalOpts, args: &PruneArgs) -> Result<(), Failure> {
    let command: &'static str = "prune";
    // The reference opens with readConfig (its no-store wording) and
    // re-reads each candidate domain under its file lock for the
    // write phase; our seam holds the store writer lock for the whole
    // cycle, so the preview read IS the locked re-read.
    let store = crate::commands::store::open_store_mut(command, true)?;

    // The anchor-knob validation fires before any read (prune.ts:247):
    // a misconfigured block fails the command even without
    // --check-anchors.
    let anchor_block = anchor_validity_block(store.config());
    let errors = validate_anchor_validity_config(&anchor_block);
    if !errors.is_empty() {
        let message = format!(
            "Invalid decay.anchor_validity config: {}. Edit .mulch/mulch.config.yaml.",
            errors.join("; ")
        );
        return Err(Failure::error_line(command, message));
    }
    let anchors = if args.check_anchors {
        Some(resolve_anchor_params(&anchor_block))
    } else {
        None
    };

    let shelf_life = store.config().effective_shelf_life();
    let rule = StaleRule::from_shelf_life(&shelf_life);
    let now = Timestamp::now();
    let now_iso = crate::commands::failure::now_iso();
    let project_root = crate::commands::store::cwd(command)?;

    // Phase 1 — preview: every domain strict, one pass (the writer
    // lock makes this read the reference's locked phase-3 re-read).
    let mut live: Vec<(String, Vec<Value>)> = Vec::new();
    for domain in store.domains() {
        let records = crate::commands::store::read_domain_records(
            command,
            &store,
            &domain,
            opts.allow_unknown_types,
        )?
        .into_iter()
        .map(|line| line.record)
        .collect();
        live.push((domain, records));
    }

    // The cross-domain supersession set (content-relational, not
    // domain-bound) and the cycle members it must exclude.
    let (superseded_ids, cycle_ids) =
        collect_superseded_ids(live.iter().map(|(_, records)| records));
    if !cycle_ids.is_empty() && !opts.json && !opts.quiet {
        print_warning(&format!(
            "Warning: supersession cycle detected for {} record(s); cycle members will not be \
             demoted. Run `mulch doctor` for details.",
            cycle_ids.len()
        ));
    }

    // The classification context every later phase reads: phase 2
    // (the hook payload's buckets) and phase 3 (the writes) share it.
    let walk = PruneWalk {
        rule: &rule,
        now,
        now_ms: timestamp_ms(now),
        superseded_ids: &superseded_ids,
        anchors: anchors.as_ref(),
        project_root: &project_root,
    };

    // Phase 2 — pre-prune hooks (prune.ts:330): between the preview
    // and the write phases; skipped in --dry-run (preview-only runs
    // must not fire side-effecting hooks) and when no domain has
    // candidates. LOCK NOTE: the reference takes its per-domain file
    // locks only in the write phase, so its hook runs lock-free; this
    // port opened the store under the writer lock up front — the hook
    // observes the same config either way, only a concurrent writer
    // would notice the held lock (acceptable seam deviation).
    if !args.dry_run {
        let candidate_rows = hook_candidate_rows(&live, &walk);
        if !candidate_rows.is_empty() {
            let phase = HookPhase::run(
                mulch::hooks::HookEvent::PrePrune,
                serde_json::json!({"candidates": candidate_rows}),
                &store,
                command,
            )?;
            if let Some(reason) = phase.blocked_reason("pre-prune hook blocked") {
                return Err(Failure::error_line(command, reason));
            }
            // Warnings plain-only (no warning source exists for
            // pre-prune today — blocking and non-mutable — but the
            // law is wired like the reference's loop).
            if !opts.json {
                print_hook_warnings(phase.warnings());
            }
            // Block-or-allow: the hook's stdout is ignored (probe
            // PR-F — a hook printing candidate-reshaping JSON changes
            // nothing); this port never reads hooks.payload here.
        }
    }

    // Phase 3 — classify and write. Staleness wins over supersession
    // on a record hitting both; a record both superseded AND
    // anchor-decayed demotes one tier with both reasons stamped.
    let mut outcomes: Vec<DomainOutcome> = Vec::new();
    for (domain, records) in live {
        if let Some(outcome) = prune_domain(&domain, records, &walk, args.aggressive, &now_iso) {
            outcomes.push(outcome);
        }
    }
    let mut totals = Totals::default();
    for outcome in &outcomes {
        // `--hard` deletes: the live rewrite alone, no archive write.
        // Dry-run writes nothing at all.
        if !args.dry_run && args.hard {
            crate::commands::store::persist(command, &store, &outcome.domain, &outcome.kept)?;
        }
        // The default shelf reason covers the staleness path;
        // bottom-out records pre-stamp their own reason and the
        // archive seam preserves it. Both halves of the soft path
        // share prune's handled shape, so the transaction error
        // collapses to one mapping.
        if !args.dry_run && !args.hard {
            store
                .move_to_archive(
                    &outcome.domain,
                    &outcome.kept,
                    &outcome.archived,
                    &now_iso,
                    "stale",
                )
                .map_err(|source| {
                    Failure::handled(command, crate::output::chain_message(source.as_error()))
                })?;
        }
        totals.pruned += outcome.pruned;
        totals.demoted += outcome.demoted;
        totals.anchor_demoted += outcome.anchor_demoted;
        totals.supersession_demoted += outcome.supersession_demoted;
    }

    if opts.json {
        let mut fields = Map::new();
        fields.insert("dryRun".into(), Value::Bool(args.dry_run));
        fields.insert("hard".into(), Value::Bool(args.hard));
        fields.insert("aggressive".into(), Value::Bool(args.aggressive));
        fields.insert("checkAnchors".into(), Value::Bool(args.check_anchors));
        fields.insert("totalPruned".into(), Value::from(totals.pruned));
        fields.insert("totalDemoted".into(), Value::from(totals.demoted));
        fields.insert(
            "totalAnchorDemoted".into(),
            Value::from(totals.anchor_demoted),
        );
        fields.insert(
            "totalSupersessionDemoted".into(),
            Value::from(totals.supersession_demoted),
        );
        fields.insert(
            "results".into(),
            Value::Array(outcomes.iter().map(DomainOutcome::to_json).collect()),
        );
        // The legacy shape: demotion-only, gated on --explain. A
        // truthy empty array still ships (`explanations: []` when
        // every action was a stale archive).
        if args.explain {
            let explanations: Vec<Value> = outcomes
                .iter()
                .flat_map(|outcome| outcome.actions.iter())
                .filter(|action| action.reasons.iter().any(|reason| *reason != "stale"))
                .map(RecordAction::to_json)
                .collect();
            fields.insert("explanations".into(), Value::Array(explanations));
        }
        print_json(&success_envelope(command, fields), false);
        return Ok(());
    }

    if totals.pruned == 0 && totals.demoted == 0 {
        if !opts.quiet {
            print_line(
                false,
                "No stale or superseded records found. All records are current.",
            );
        }
        return Ok(());
    }

    render_plain(opts, args, &outcomes, &totals);
    Ok(())
}

/// The per-domain write plan the plain and JSON renderers read.
struct DomainOutcome {
    domain:               String,
    before:               usize,
    after:                usize,
    pruned:               usize,
    demoted:              usize,
    anchor_demoted:       usize,
    supersession_demoted: usize,
    /// Survivors (demotions applied), the rewrite set.
    kept:                 Vec<Value>,
    /// Records leaving the live file; bottom-outs pre-stamp their
    /// `archive_reason`.
    archived:             Vec<Value>,
    actions:              Vec<RecordAction>,
}

impl DomainOutcome {
    /// The `results` row (field order reference-pinned).
    fn to_json(&self) -> Value {
        let mut row = Map::new();
        row.insert("domain".into(), Value::String(self.domain.clone()));
        row.insert("before".into(), Value::from(self.before));
        row.insert("pruned".into(), Value::from(self.pruned));
        row.insert("demoted".into(), Value::from(self.demoted));
        row.insert("anchor_demoted".into(), Value::from(self.anchor_demoted));
        row.insert(
            "supersession_demoted".into(),
            Value::from(self.supersession_demoted),
        );
        row.insert("after".into(), Value::from(self.after));
        Value::Object(row)
    }
}

/// The command's totals.
#[derive(Default)]
struct Totals {
    pruned:               usize,
    demoted:              usize,
    anchor_demoted:       usize,
    supersession_demoted: usize,
}

/// One per-record action row: the plain per-record lines and the
/// `--explain` entries render from it.
struct RecordAction {
    domain:      String,
    id:          Option<String>,
    record_type: Option<String>,
    from:        String,
    to:          String,
    reasons:     Vec<&'static str>,
    anchors:     Option<AnchorReport>,
}

impl RecordAction {
    /// The `explanations` entry: `id`/`type` drop when the record
    /// carried none (the reference's undefined-valued keys vanish
    /// under JSON.stringify); `anchors` rides every anchor-decay
    /// action.
    fn to_json(&self) -> Value {
        let mut row = Map::new();
        row.insert("domain".into(), Value::String(self.domain.clone()));
        if let Some(id) = &self.id {
            row.insert("id".into(), Value::String(id.clone()));
        }
        if let Some(record_type) = &self.record_type {
            row.insert("type".into(), Value::String(record_type.clone()));
        }
        row.insert("from".into(), Value::String(self.from.clone()));
        row.insert("to".into(), Value::String(self.to.clone()));
        row.insert(
            "reasons".into(),
            Value::Array(
                self.reasons
                    .iter()
                    .map(|reason| Value::String((*reason).into()))
                    .collect(),
            ),
        );
        if let Some(report) = &self.anchors {
            row.insert("anchors".into(), report.to_json());
        }
        Value::Object(row)
    }
}

/// The per-run classification context: one owner for every law the
/// per-record walk reads.
struct PruneWalk<'a> {
    rule:           &'a StaleRule,
    now:            Timestamp,
    now_ms:         f64,
    superseded_ids: &'a HashSet<String>,
    /// The resolved knobs; `None` when `--check-anchors` is off.
    anchors:        Option<&'a AnchorParams>,
    project_root:   &'a Path,
}

/// The resolved `decay.anchor_validity` knobs (defaults backfilled).
struct AnchorParams {
    threshold:  f64,
    grace_days: f64,
}

impl PruneWalk<'_> {
    /// The shelf-life verdict (our [`mulch::stale`] seam IS the
    /// reference `isStale`).
    fn stale(&self, record: &Value) -> bool {
        matches!(
            self.rule.verdict(record, self.now),
            StaleVerdict::Stale { .. }
        )
    }

    /// `isAnchorDecayed`: `None` = no hit. A NaN age (unparsable
    /// `recorded_at`) fails the grace comparison like the reference's
    /// NaN comparison; zero-anchor records are exempt (a 0/0 record
    /// would otherwise compute a NaN fraction).
    fn anchor_decayed(&self, record: &Value) -> Option<AnchorReport> {
        let params = self.anchors?;
        let recorded_ms = mulch::date_ms(record)?;
        let age_days = (self.now_ms - recorded_ms) / MS_PER_DAY;
        // The reference's `ageDays > graceDays` is false for NaN
        // (unparsable `recorded_at`), so an incomparable pair exits.
        if !matches!(
            age_days.partial_cmp(&params.grace_days),
            Some(std::cmp::Ordering::Greater)
        ) {
            return None;
        }
        let report = mulch::anchors::validity(record, self.project_root);
        // Zero anchors exempt (a 0/0 record would otherwise compute a
        // NaN fraction and falsely trip the threshold).
        if report.total == 0 || report.valid_fraction >= params.threshold {
            return None;
        }
        Some(report)
    }

    /// The truthy-string id read (the reference's `!!record.id` /
    /// `!r.id` gates: empty and non-string ids never match).
    fn record_id(record: &Value) -> Option<&str> {
        record
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
    }
}

/// Classifies one domain's records and builds its write plan.
/// `None` when nothing would change (not a candidate domain: no
/// result row, no writes).
fn prune_domain(
    domain: &str,
    records: Vec<Value>,
    walk: &PruneWalk,
    aggressive: bool,
    now_iso: &str,
) -> Option<DomainOutcome> {
    let mut kept = Vec::new();
    let mut archived = Vec::new();
    let mut actions = Vec::new();
    let mut pruned = 0;
    let mut demoted = 0;
    let mut anchor_demoted = 0;
    let mut supersession_demoted = 0;
    let before = records.len();

    for record in records {
        // Staleness wins: no point demoting something being archived.
        if walk.stale(&record) {
            pruned += 1;
            actions.push(RecordAction {
                domain:      domain.to_string(),
                id:          action_id(&record),
                record_type: string_field(&record, "type"),
                from:        display_text(record.get("classification")),
                to:          "archived".into(),
                reasons:     vec!["stale"],
                anchors:     None,
            });
            archived.push(record);
            continue;
        }
        let superseded_hit =
            PruneWalk::record_id(&record).is_some_and(|id| walk.superseded_ids.contains(id));
        let anchor_hit = walk.anchor_decayed(&record);
        if !superseded_hit && anchor_hit.is_none() {
            kept.push(record);
            continue;
        }
        let mut reasons: Vec<&'static str> = Vec::new();
        if superseded_hit {
            reasons.push(SUPERSEDED);
        }
        if anchor_hit.is_some() {
            reasons.push(ANCHOR_DECAY);
        }
        let from = display_text(record.get("classification"));
        let target = if aggressive {
            None
        } else {
            next_demotion_tier(record.get("classification").and_then(Value::as_str))
        };
        let Some(target) = target else {
            // Bottom-out: pre-stamp the per-record reason so the
            // domain-wide archive write ("stale") preserves it.
            pruned += 1;
            let bottom_reason = match (superseded_hit, anchor_hit.is_some()) {
                (true, true) => "superseded+anchor_decay",
                (true, false) => SUPERSEDED,
                (false, _) => ANCHOR_DECAY,
            };
            let mut bottom = record.clone();
            if let Some(object) = bottom.as_object_mut() {
                object.insert("archive_reason".into(), Value::String(bottom_reason.into()));
            }
            actions.push(RecordAction {
                domain: domain.to_string(),
                id: action_id(&record),
                record_type: string_field(&record, "type"),
                from,
                to: "archived".into(),
                reasons,
                anchors: anchor_hit,
            });
            archived.push(bottom);
            continue;
        };
        let mut demoted_record = record.clone();
        if let Some(object) = demoted_record.as_object_mut() {
            // Present keys replace in place, absent keys append (the
            // archive stamp law; serde's ordered map does both).
            object.insert("classification".into(), Value::String(target.into()));
            if superseded_hit {
                object.insert(
                    "supersession_demoted_at".into(),
                    Value::String(now_iso.to_string()),
                );
                supersession_demoted += 1;
            }
            if anchor_hit.is_some() {
                object.insert(
                    "anchor_decay_demoted_at".into(),
                    Value::String(now_iso.to_string()),
                );
                anchor_demoted += 1;
            }
        }
        demoted += 1;
        actions.push(RecordAction {
            domain: domain.to_string(),
            id: action_id(&record),
            record_type: string_field(&record, "type"),
            from,
            to: target.into(),
            reasons,
            anchors: anchor_hit,
        });
        kept.push(demoted_record);
    }

    if pruned == 0 && demoted == 0 {
        return None;
    }
    let after = kept.len();
    Some(DomainOutcome {
        domain: domain.to_string(),
        before,
        after,
        pruned,
        demoted,
        anchor_demoted,
        supersession_demoted,
        kept,
        archived,
        actions,
    })
}

/// The id for an action row (`None` when absent — the JSON row drops
/// it, the plain line renders `(no id)`).
fn action_id(record: &Value) -> Option<String> {
    PruneWalk::record_id(record).map(str::to_string)
}

/// A record's string field, cloned for an action row.
fn string_field(record: &Value, field: &str) -> Option<String> {
    record
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// The JS template coercion of a display field (`undefined` when the
/// record carried none).
fn display_text(value: Option<&Value>) -> String {
    value.map_or_else(|| "undefined".into(), value_text)
}

/// Next tier on the demotion ladder: `None` = bottomed out, archive.
fn next_demotion_tier(classification: Option<&str>) -> Option<&'static str> {
    match classification {
        Some("foundational") => Some("tactical"),
        Some("tactical") => Some("observational"),
        _ => None,
    }
}

// ---- plain rendering ----

/// The plain report: per-domain lines and per-record actions (both
/// `--quiet`-gated), the totals line (always — seed mulch-5ce3), and
/// the archive hint.
fn render_plain(opts: &GlobalOpts, args: &PruneArgs, outcomes: &[DomainOutcome], totals: &Totals) {
    let action_label = match (args.dry_run, args.hard) {
        (true, true) => "Would delete",
        (true, false) => "Would archive",
        (false, true) => "Deleted",
        (false, false) => "Archived",
    };
    let demote_label = if args.dry_run {
        "Would demote"
    } else {
        "Demoted"
    };
    let prefix = if args.dry_run { "[DRY RUN] " } else { "" };

    for outcome in outcomes {
        if opts.quiet {
            continue;
        }
        let body = if outcome.pruned > 0 && outcome.demoted > 0 {
            format!(
                "{action_label} {}, demoted {}",
                outcome.pruned, outcome.demoted
            )
        } else if outcome.pruned > 0 {
            format!("{action_label} {}", outcome.pruned)
        } else {
            format!("{demote_label} {}", outcome.demoted)
        };
        print_line(
            false,
            &format!(
                "{prefix}{}: {} of {} records ({} remaining)",
                outcome.domain, body, outcome.before, outcome.after
            ),
        );
        for action in &outcome.actions {
            let id_part = action.id.as_deref().unwrap_or("(no id)");
            print_line(
                false,
                &format!(
                    "  {} [{}]: {} → {} ({})",
                    id_part,
                    action.record_type.as_deref().unwrap_or("undefined"),
                    action.from,
                    action.to,
                    action.reasons.join(" + ")
                ),
            );
            if args.explain
                && let Some(report) = &action.anchors
            {
                print_line(
                    false,
                    &format!(
                        "      anchors: {}/{} valid ({}%)",
                        report.valid,
                        report.total,
                        js_to_fixed_0(report.valid_fraction)
                    ),
                );
                for (kind, path) in &report.broken {
                    print_line(false, &format!("        ✗ {kind}: {path}"));
                }
            }
        }
    }

    let mut parts: Vec<String> = Vec::new();
    if totals.pruned > 0 {
        let noun = if totals.pruned == 1 {
            "record"
        } else {
            "records"
        };
        parts.push(format!(
            "{} {} stale {noun}",
            action_label.to_lowercase(),
            totals.pruned
        ));
    }
    if totals.demoted > 0 {
        let noun = if totals.demoted == 1 {
            "record"
        } else {
            "records"
        };
        let mut breakdown: Vec<String> = Vec::new();
        if totals.supersession_demoted > 0 {
            breakdown.push(format!("{} superseded", totals.supersession_demoted));
        }
        if totals.anchor_demoted > 0 {
            breakdown.push(format!("{} anchor-decayed", totals.anchor_demoted));
        }
        let suffix = if breakdown.len() > 1 {
            format!(" ({})", breakdown.join(", "))
        } else {
            String::new()
        };
        let tag = if breakdown.len() == 1 && totals.supersession_demoted > 0 {
            "superseded "
        } else if breakdown.len() == 1 && totals.anchor_demoted > 0 {
            "anchor-decayed "
        } else {
            ""
        };
        parts.push(format!(
            "{} {} {tag}{noun}{suffix}",
            demote_label.to_lowercase(),
            totals.demoted
        ));
    }
    // Totals print even under --quiet; the separator blank line is
    // what quiet suppresses.
    let separator = if opts.quiet { "" } else { "\n" };
    print_line(
        false,
        &format!("{separator}{prefix}Total: {}.", parts.join("; ")),
    );
    if !opts.quiet && !args.hard && !args.dry_run && totals.pruned > 0 {
        print_line(
            false,
            "Records moved to .mulch/archive/. Restore with `mulch restore <id>` or use `--hard` \
             next time to permanently delete.",
        );
    }
}

/// The pre-prune payload rows (prune.ts:302): per candidate domain
/// — any bucket non-empty — the raw record objects per bucket in
/// file order. Staleness wins (a stale record rides `stale` only);
/// supersession and anchor decay can hit the SAME record, which then
/// rides both buckets (probe PR-A: `mx-both001` in `demote` AND
/// `anchor_decay`).
fn hook_candidate_rows(live: &[(String, Vec<Value>)], walk: &PruneWalk) -> Vec<Value> {
    let mut rows = Vec::new();
    for (domain, records) in live {
        let mut stale: Vec<Value> = Vec::new();
        let mut demote: Vec<Value> = Vec::new();
        let mut anchor_decay: Vec<Value> = Vec::new();
        for record in records {
            if walk.stale(record) {
                stale.push(record.clone());
                continue;
            }
            let superseded_hit =
                PruneWalk::record_id(record).is_some_and(|id| walk.superseded_ids.contains(id));
            if superseded_hit {
                demote.push(record.clone());
            }
            if walk.anchor_decayed(record).is_some() {
                anchor_decay.push(record.clone());
            }
        }
        if !(stale.is_empty() && demote.is_empty() && anchor_decay.is_empty()) {
            rows.push(serde_json::json!({
                "domain": domain,
                "stale": stale,
                "demote": demote,
                "anchor_decay": anchor_decay,
            }));
        }
    }
    rows
}

/// The cycle warning rides stderr (plain, non-quiet modes only).
#[allow(clippy::print_stderr, reason = "the warning channel is stderr")]
fn print_warning(text: &str) {
    eprintln!("{text}");
}

/// The raw `decay.anchor_validity` block: any shape, missing keys
/// included (validation and resolution read it per the reference's
/// `config.decay?.anchor_validity ?? {}`).
fn anchor_validity_block(config: &mulch::Config) -> Yaml {
    config
        .extra("decay")
        .and_then(|decay| decay.get("anchor_validity"))
        .cloned()
        .unwrap_or_default()
}

/// The knob validation law (validateAnchorValidityConfig): range and
/// finiteness errors, reference wording, joined by the caller.
fn validate_anchor_validity_config(block: &Yaml) -> Vec<String> {
    let mut errors = Vec::new();
    let read = |field: &str| -> Option<&Yaml> { block.get(field).filter(|value| !value.is_null()) };
    if let Some(threshold) = read("threshold") {
        match yaml_f64(threshold) {
            Some(value) if (0.0..=1.0).contains(&value) => {}
            Some(value) => errors.push(format!("threshold must be between 0 and 1 (got {value})")),
            None => errors.push(format!(
                "threshold must be a finite number (got {})",
                js_stringify(threshold)
            )),
        }
    }
    if let Some(grace) = read("grace_days") {
        match yaml_f64(grace) {
            Some(value) if value >= 0.0 => {}
            Some(value) => errors.push(format!("grace_days must be >= 0 (got {value})")),
            None => errors.push(format!(
                "grace_days must be a finite number (got {})",
                js_stringify(grace)
            )),
        }
    }
    errors
}

/// The knobs with the reference defaults backfilled. A non-mapping
/// block behaves as empty (the reference's property reads on a
/// non-object yield undefined).
fn resolve_anchor_params(block: &Yaml) -> AnchorParams {
    let threshold = block
        .get("threshold")
        .and_then(yaml_f64)
        .unwrap_or(DEFAULT_ANCHOR_THRESHOLD);
    let grace_days = block
        .get("grace_days")
        .and_then(yaml_f64)
        .unwrap_or(DEFAULT_ANCHOR_GRACE_DAYS);
    AnchorParams {
        threshold,
        grace_days,
    }
}

/// A YAML value as a finite JS number (booleans and strings are not
/// numbers; NaN and infinities are not finite).
fn yaml_f64(value: &Yaml) -> Option<f64> {
    match value {
        Yaml::Number(number) => number.as_f64().filter(|f| f.is_finite()),
        _ => None,
    }
}

/// `JSON.stringify` of a scalar YAML value for the validation
/// message's `(got …)` slot: strings quote, non-finite numbers render
/// `null`, numbers print like a JS number.
fn js_stringify(value: &Yaml) -> String {
    match value {
        Yaml::Null => "null".into(),
        Yaml::Bool(flag) => flag.to_string(),
        Yaml::Number(number) => match number.as_f64() {
            Some(float) if float.is_finite() => js_number(float).to_string(),
            _ => "null".into(),
        },
        Yaml::String(text) => serde_json::Value::String(text.clone()).to_string(),
        // Mappings/sequences stringify like their JSON shapes.
        other => serde_json::to_value(other)
            .unwrap_or_else(|_| serde_json::Value::Null)
            .to_string(),
    }
}

// ---- supersession graph ----

/// The cross-domain supersession census: every live record's
/// `supersedes` targets (chains included — the second pass takes ALL
/// edge targets), minus cycle members. Self-references never register.
fn collect_superseded_ids<'a>(
    domains: impl Iterator<Item = &'a Vec<Value>>,
) -> (HashSet<String>, HashSet<String>) {
    let mut graph = SupersessionGraph::default();
    let mut all_edges: Vec<String> = Vec::new();
    for records in domains {
        for record in records {
            let Some(id) = PruneWalk::record_id(record) else {
                continue;
            };
            let supersedes = supersedes_entries(record);
            if supersedes.is_empty() {
                continue;
            }
            let source = graph.ensure_node(id);
            for target in supersedes {
                if target == id {
                    continue;
                }
                let target_index = graph.ensure_node(&target);
                if !graph.edges[source].contains(&target_index) {
                    graph.edges[source].push(target_index);
                }
                all_edges.push(target);
            }
        }
    }
    let cycle_ids = graph.find_cycle_ids();
    let superseded_ids = all_edges
        .into_iter()
        .filter(|target| !cycle_ids.contains(target))
        .collect();
    (superseded_ids, cycle_ids)
}

/// The `supersedes` field's targets: array string entries, or the
/// characters of a bare string (the reference's `for...of`). Truthy
/// non-string non-array values crash the reference's iterator; this
/// port treats them as absent (README DEVIATIONS).
fn supersedes_entries(record: &Value) -> Vec<String> {
    match record.get("supersedes") {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| item.as_str().map(str::to_string))
            .collect(),
        Some(Value::String(text)) if !text.is_empty() => {
            text.chars().map(|c| c.to_string()).collect()
        }
        _ => Vec::new(),
    }
}

/// The id graph: insertion-ordered nodes with deduped adjacency (the
/// reference's `Map<string, Set<string>>`).
#[derive(Default)]
struct SupersessionGraph {
    nodes:    Vec<String>,
    edges:    Vec<Vec<usize>>,
    index_of: HashMap<String, usize>,
}

/// Tarjan's working state, index-addressed.
struct TarjanState {
    indices:    Vec<usize>,
    lowlinks:   Vec<usize>,
    on_stack:   Vec<bool>,
    stack:      Vec<usize>,
    next_index: usize,
}

/// One iterative-recursion frame.
struct Frame {
    node:          usize,
    next_edge:     usize,
    pending_child: Option<usize>,
}

impl TarjanState {
    /// Opens a node: index, lowlink, stack push.
    fn open(&mut self, node: usize, call_stack: &mut Vec<Frame>) {
        self.indices[node] = self.next_index;
        self.lowlinks[node] = self.next_index;
        self.next_index += 1;
        self.stack.push(node);
        self.on_stack[node] = true;
        call_stack.push(Frame {
            node,
            next_edge: 0,
            pending_child: None,
        });
    }
}

impl SupersessionGraph {
    fn ensure_node(&mut self, id: &str) -> usize {
        if let Some(index) = self.index_of.get(id) {
            return *index;
        }
        let index = self.nodes.len();
        self.nodes.push(id.to_string());
        self.edges.push(Vec::new());
        self.index_of.insert(id.to_string(), index);
        index
    }

    /// Ids in any supersession cycle (SCC of size > 1), iterative
    /// Tarjan so long chains cannot blow the stack. Self-loops are
    /// filtered at edge-collection time and never register.
    fn find_cycle_ids(&self) -> HashSet<String> {
        let node_count = self.nodes.len();
        let mut state = TarjanState {
            indices:    vec![usize::MAX; node_count],
            lowlinks:   vec![0; node_count],
            on_stack:   vec![false; node_count],
            stack:      Vec::new(),
            next_index: 0,
        };
        let mut call_stack: Vec<Frame> = Vec::new();
        let mut cycles = HashSet::new();
        for start in 0..node_count {
            if state.indices[start] != usize::MAX {
                continue;
            }
            state.open(start, &mut call_stack);
            while !call_stack.is_empty() {
                let top = call_stack.len() - 1;
                // pending-child lowlink propagation (the recursion's
                // return step).
                if let Some(child) = call_stack[top].pending_child.take() {
                    let node = call_stack[top].node;
                    let child_low = state.lowlinks[child];
                    if child_low < state.lowlinks[node] {
                        state.lowlinks[node] = child_low;
                    }
                }
                let node = call_stack[top].node;
                if call_stack[top].next_edge >= self.edges[node].len() {
                    // Node exhausted: pop; a root (index == lowlink)
                    // closes its strongly connected component.
                    call_stack.pop();
                    if state.indices[node] == state.lowlinks[node] {
                        let mut component = Vec::new();
                        while let Some(popped) = state.stack.pop() {
                            state.on_stack[popped] = false;
                            component.push(popped);
                            if popped == node {
                                break;
                            }
                        }
                        if component.len() > 1 {
                            for member in component {
                                cycles.insert(self.nodes[member].clone());
                            }
                        }
                    }
                    continue;
                }
                let target = self.edges[node][call_stack[top].next_edge];
                call_stack[top].next_edge += 1;
                if state.indices[target] == usize::MAX {
                    call_stack[top].pending_child = Some(target);
                    state.open(target, &mut call_stack);
                } else if state.on_stack[target] && state.indices[target] < state.lowlinks[node] {
                    state.lowlinks[node] = state.indices[target];
                }
            }
        }
        cycles
    }
}

// ---- JS display helpers ----

/// `(value * 100).toFixed(0)` for the explain line's non-negative
/// fractions: round-half-up on the scaled double (Rust's `{:.0}`
/// rounds half-to-even and diverges on exact .5 products).
fn js_to_fixed_0(value: f64) -> String {
    let scaled = value * 100.0;
    let rounded = if scaled.is_finite() {
        (scaled + 0.5).floor()
    } else {
        0.0
    };
    if rounded < 0.0 {
        "0".into()
    } else {
        format!("{rounded:.0}")
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// The demotion ladder: foundational → tactical → observational
    /// → bottom-out; anything else bottoms out immediately.
    #[test]
    fn demotion_ladder_steps_down_then_bottoms_out() {
        assert_eq!(next_demotion_tier(Some("foundational")), Some("tactical"));
        assert_eq!(next_demotion_tier(Some("tactical")), Some("observational"));
        assert_eq!(next_demotion_tier(Some("observational")), None);
        assert_eq!(next_demotion_tier(Some("mystic")), None);
        assert_eq!(next_demotion_tier(None), None);
    }

    fn census(records: &[Value]) -> (HashSet<String>, HashSet<String>) {
        let owned: Vec<Vec<Value>> = vec![records.to_vec()];
        collect_superseded_ids(owned.iter())
    }

    fn set(items: &[&str]) -> HashSet<String> {
        items.iter().map(|item| (*item).to_string()).collect()
    }

    /// Direct targets supersede, chains supersede transitively through
    /// the second pass, and self-references never register.
    #[test]
    fn supersession_census_takes_all_edge_targets() {
        let records = vec![
            json!({"id": "a", "supersedes": ["b"]}),
            json!({"id": "b", "supersedes": ["c"]}),
            json!({"id": "c"}),
            json!({"id": "self", "supersedes": ["self"]}),
        ];
        let (superseded, cycles) = census(&records);
        assert_eq!(superseded, set(&["b", "c"]));
        assert!(cycles.is_empty());
    }

    /// Cycle members are excluded even when targeted from outside the
    /// cycle; the SCC is size-gated (a 2-ring, a 3-ring, both flagged
    /// whole).
    #[test]
    fn cycle_members_are_excluded_whole() {
        let records = vec![
            json!({"id": "a", "supersedes": ["b"]}),
            json!({"id": "b", "supersedes": ["a"]}),
            json!({"id": "outside", "supersedes": ["a"]}),
            json!({"id": "x", "supersedes": ["y"]}),
            json!({"id": "y", "supersedes": ["z"]}),
            json!({"id": "z", "supersedes": ["x"]}),
            json!({"id": "chain", "supersedes": ["x"]}),
        ];
        let (superseded, cycles) = census(&records);
        assert!(superseded.is_empty(), "every target is a cycle member");
        assert_eq!(cycles, set(&["a", "b", "x", "y", "z"]));
    }

    /// Long chains cannot blow the iterative Tarjan stack.
    #[test]
    fn long_chain_is_not_a_cycle() {
        let mut records = Vec::new();
        for i in 0..20_000 {
            let next = format!("n{}", i + 1);
            records.push(json!({"id": format!("n{i}"), "supersedes": [next]}));
        }
        let (superseded, cycles) = census(&records);
        assert!(cycles.is_empty());
        assert_eq!(superseded.len(), 20_000);
    }

    /// Records without an id never source edges; a bare-string
    /// `supersedes` iterates its characters (the reference's for...of).
    #[test]
    fn id_less_records_and_string_supersedes_follow_the_reference_gates() {
        let records = vec![
            json!({"supersedes": ["a"]}),
            json!({"id": "", "supersedes": ["a"]}),
            json!({"id": "s", "supersedes": "xy"}),
            json!({"id": "n", "supersedes": [1, 2, "n"]}),
        ];
        let (superseded, cycles) = census(&records);
        assert_eq!(superseded, set(&["x", "y"]));
        assert!(cycles.is_empty());
    }

    /// The validation law's wording: ranges, finiteness, and the
    /// JSON.stringify `(got …)` slot.
    #[test]
    fn anchor_validation_renders_the_reference_errors() {
        let empty = serde_yaml::from_str("{}").expect("empty mapping");
        assert_eq!(
            validate_anchor_validity_config(&empty),
            Vec::<String>::new()
        );
        let both = serde_yaml::from_str("threshold: 1.5\ngrace_days: -2").expect("yaml");
        assert_eq!(validate_anchor_validity_config(&both), vec![
            "threshold must be between 0 and 1 (got 1.5)".to_string(),
            "grace_days must be >= 0 (got -2)".to_string(),
        ]);
        let string = serde_yaml::from_str("threshold: \"0.5\"").expect("yaml");
        assert_eq!(validate_anchor_validity_config(&string), vec![
            "threshold must be a finite number (got \"0.5\")".to_string()
        ]);
        let bools = serde_yaml::from_str("grace_days: true").expect("yaml");
        assert_eq!(validate_anchor_validity_config(&bools), vec![
            "grace_days must be a finite number (got true)".to_string()
        ]);
        let null = serde_yaml::from_str("threshold: null").expect("yaml");
        assert_eq!(validate_anchor_validity_config(&null), Vec::<String>::new());
        let nan = serde_yaml::from_str("threshold: .nan").expect("yaml");
        assert_eq!(validate_anchor_validity_config(&nan), vec![
            "threshold must be a finite number (got null)".to_string()
        ]);
        let negative = serde_yaml::from_str("threshold: -0.1").expect("yaml");
        assert_eq!(validate_anchor_validity_config(&negative), vec![
            "threshold must be between 0 and 1 (got -0.1)".to_string()
        ]);
    }

    /// Defaults backfill per knob; a non-mapping block reads empty.
    #[test]
    fn anchor_params_backfill_the_reference_defaults() {
        let block = serde_yaml::from_str("threshold: 0.9").expect("yaml");
        let params = resolve_anchor_params(&block);
        assert!((params.threshold - 0.9).abs() < f64::EPSILON);
        assert!((params.grace_days - 7.0).abs() < f64::EPSILON);
        let scalar = serde_yaml::from_str("just a string").expect("yaml");
        let params = resolve_anchor_params(&scalar);
        assert!((params.threshold - DEFAULT_ANCHOR_THRESHOLD).abs() < f64::EPSILON);
        assert!((params.grace_days - DEFAULT_ANCHOR_GRACE_DAYS).abs() < f64::EPSILON);
    }

    /// The prune-side fold over the census (the hit site): grace gate
    /// first, then the zero-anchor exemption, then the threshold. The
    /// census edge table itself lives in `mulch::anchors`.
    #[test]
    fn anchor_decay_folds_the_census_at_the_hit_site() {
        let params = AnchorParams {
            threshold:  0.9,
            grace_days: 7.0,
        };
        let root = std::env::temp_dir();
        let good = root.join("mulch-anchor-fold-good.txt");
        std::fs::write(&good, b"").expect("write anchor");
        let good = good.to_str().expect("utf-8 anchor path");
        let walk = PruneWalk {
            rule:           &StaleRule::default(),
            now:            "2026-10-24T00:00:00.000Z".parse().expect("now parses"),
            now_ms:         1_792_214_400_000.0,
            superseded_ids: &set(&[]),
            anchors:        Some(&params),
            project_root:   &root,
        };
        let old = "2026-10-01T00:00:00.000Z";
        // Below threshold after the grace window: the report rides out.
        let broken = json!({"recorded_at": old, "files": ["missing-a.txt", "missing-b.txt"]});
        let hit = walk.anchor_decayed(&broken).expect("demotion hit");
        assert_eq!(hit.total, 2);
        assert_eq!(hit.valid, 0);
        // Zero anchors: exempt (a 0/0 fraction must not trip).
        let bare = json!({"recorded_at": old});
        assert!(walk.anchor_decayed(&bare).is_none());
        // At/above threshold: no demotion.
        let valid = json!({"recorded_at": old, "files": [good]});
        assert!(walk.anchor_decayed(&valid).is_none());
        // Inside the grace window: the census never runs.
        let young = json!({"recorded_at": "2026-10-23T00:00:00.000Z", "files": ["missing-c.txt"]});
        assert!(walk.anchor_decayed(&young).is_none());
    }

    /// The explain percent rounds half-up on the scaled double like
    /// JS `toFixed(0)` (Rust's `{:.0}` would print 12 for 12.5).
    #[test]
    fn explain_percent_rounds_like_js_to_fixed() {
        assert_eq!(js_to_fixed_0(0.0), "0");
        assert_eq!(js_to_fixed_0(0.5), "50");
        assert_eq!(js_to_fixed_0(2.0 / 3.0), "67");
        assert_eq!(js_to_fixed_0(0.125), "13");
        assert_eq!(js_to_fixed_0(0.9999), "100");
    }

    /// The stamp law: present keys replace in place, absent keys
    /// append — classification keeps its slot, both demotion stamps
    /// append in reason order.
    #[test]
    fn demotion_stamps_follow_the_key_position_law() {
        let record = json!({
            "type": "pattern",
            "classification": "foundational",
            "recorded_at": "2026-10-01T00:00:00.000Z",
            "id": "mx-x1",
            "supersedes": ["mx-x0"]
        });
        let walk = PruneWalk {
            rule:           &StaleRule::default(),
            now:            "2026-10-24T00:00:00.000Z".parse().expect("now parses"),
            now_ms:         1_792_214_400_000.0,
            superseded_ids: &set(&["mx-x1"]),
            anchors:        None,
            project_root:   Path::new("/nonexistent"),
        };
        let outcome = prune_domain(
            "dev",
            vec![record],
            &walk,
            false,
            "2026-10-24T01:02:03.004Z",
        )
        .expect("demotion outcome");
        assert_eq!(outcome.demoted, 1);
        assert_eq!(outcome.supersession_demoted, 1);
        let kept = &outcome.kept[0];
        assert_eq!(
            kept.as_object().expect("object").keys().collect::<Vec<_>>(),
            vec![
                "type",
                "classification",
                "recorded_at",
                "id",
                "supersedes",
                "supersession_demoted_at"
            ]
        );
        assert_eq!(kept["classification"], json!("tactical"));
        assert_eq!(
            kept["supersession_demoted_at"],
            json!("2026-10-24T01:02:03.004Z")
        );
    }

    /// Staleness wins over supersession; aggressive bottoms out with
    /// the pre-stamped archive reason the archive seam preserves.
    #[test]
    fn staleness_wins_and_aggressive_bottoms_out() {
        let stale_and_superseded = json!({
            "type": "pattern",
            "classification": "tactical",
            "recorded_at": "2026-09-01T00:00:00.000Z",
            "id": "mx-s1"
        });
        let walk = PruneWalk {
            rule:           &StaleRule::default(),
            now:            "2026-10-24T00:00:00.000Z".parse().expect("now parses"),
            now_ms:         1_792_214_400_000.0,
            superseded_ids: &set(&["mx-s1"]),
            anchors:        None,
            project_root:   Path::new("/nonexistent"),
        };
        let outcome = prune_domain(
            "dev",
            vec![stale_and_superseded],
            &walk,
            false,
            "2026-10-24T01:02:03.004Z",
        )
        .expect("stale outcome");
        assert_eq!(outcome.pruned, 1);
        assert_eq!(outcome.demoted, 0);
        assert_eq!(outcome.actions[0].reasons, vec!["stale"]);
        assert!(outcome.archived[0].get("archive_reason").is_none());

        let superseded_foundational = json!({
            "type": "pattern",
            "classification": "foundational",
            "recorded_at": "2026-10-22T00:00:00.000Z",
            "id": "mx-s2"
        });
        let walk_s2 = PruneWalk {
            rule:           &StaleRule::default(),
            now:            "2026-10-24T00:00:00.000Z".parse().expect("now parses"),
            now_ms:         1_792_214_400_000.0,
            superseded_ids: &set(&["mx-s2"]),
            anchors:        None,
            project_root:   Path::new("/nonexistent"),
        };
        let outcome = prune_domain(
            "dev",
            vec![superseded_foundational],
            &walk_s2,
            true,
            "2026-10-24T01:02:03.004Z",
        )
        .expect("aggressive outcome");
        assert_eq!(outcome.pruned, 1);
        assert_eq!(outcome.archived[0]["archive_reason"], json!("superseded"));
    }

    /// An untouched domain yields no outcome (no result row, no
    /// writes); a bottomed-out observational with both reasons keeps
    /// both stamps in one demotion when a tier remains.
    #[test]
    fn untouched_domain_yields_no_outcome() {
        let fresh = json!({
            "type": "pattern",
            "classification": "foundational",
            "recorded_at": "2026-10-22T00:00:00.000Z",
            "id": "mx-f1"
        });
        let walk = PruneWalk {
            rule:           &StaleRule::default(),
            now:            "2026-10-24T00:00:00.000Z".parse().expect("now parses"),
            now_ms:         1_792_214_400_000.0,
            superseded_ids: &set(&["mx-f1"]),
            anchors:        None,
            project_root:   Path::new("/nonexistent"),
        };
        let outcome = prune_domain("dev", vec![fresh], &walk, false, "now");
        let demoted = outcome.expect("demotion outcome");
        assert_eq!(demoted.demoted, 1);
        assert!(prune_domain("dev", vec![], &walk, false, "now").is_none());
    }
}
