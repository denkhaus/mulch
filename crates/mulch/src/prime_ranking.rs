//! Trust-tier ranking and the "why surfaced" annotations for
//! `prime`'s full mode — the reference's
//! `utils/prime-ranking.ts`, byte-relevant law.
//!
//! JS semantics pinned here:
//! - Trust score: `stars * star_weight + tier_weight` where an unknown
//!   classification contributes `0` (the `?? 0` arm).
//! - `sortByTrust` is score-descending STABLE (ties keep corpus order — the
//!   reference decorates with the index to make it explicit).
//! - `whySurfaced` checks the LITERAL `files` field (not the registry's
//!   per-type files field), then `dir_anchors`, then trackers in `TRACKERS`
//!   order (`seeds, gh, linear, bead`), then stars, then a 7-day recency
//!   window, else universal. An unparsable `recorded_at` skips the recency arm
//!   entirely.
//! - Stars print as `★N` when integral, `★N.toFixed(1)` otherwise (the score is
//!   `successes + partials * 0.5`, so halves are the only fractional shape —
//!   exactly representable, no rounding ambiguity).

use serde_json::Value;

use crate::scoring::confirmation_score;

/// The tracker names, in the reference's `TRACKERS` order.
pub const TRACKERS: [&str; 4] = ["seeds", "gh", "linear", "bead"];

/// Recency window for the "recent authorship" reason (days).
pub const RECENT_AUTHORSHIP_DAYS: i64 = 7;

/// The resolved tier weights (defaults merged with the config
/// override). Field types follow the config block (u64 weights).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TierWeights {
    pub star:          u64,
    pub foundational:  u64,
    pub tactical:      u64,
    pub observational: u64,
}

/// The reference's `DEFAULT_PRIME_TIER_WEIGHTS`.
pub const DEFAULT_TIER_WEIGHTS: TierWeights = TierWeights {
    star:          100,
    foundational:  50,
    tactical:      20,
    observational: 10,
};

/// The git/tracker context `prime --context` gathers; the pure law
/// below only reads it.
#[derive(Clone, Debug, Default)]
pub struct ActiveContext {
    /// Changed files (the active-file law of `utils/git.ts`).
    pub changed_files: Vec<String>,
    /// Active tracker ids in `TRACKERS` order (empty = none).
    pub trackers:      Vec<(&'static str, String)>,
}

/// `resolveTierWeights`: defaults with the override's set fields.
pub fn resolve_tier_weights(override_: Option<&crate::config::TierWeights>) -> TierWeights {
    let Some(override_) = override_ else {
        return DEFAULT_TIER_WEIGHTS;
    };
    TierWeights {
        star:          override_.star,
        foundational:  override_.foundational,
        tactical:      override_.tactical,
        observational: override_.observational,
    }
}

/// `computeTrustScore`: stars times the star weight plus the
/// classification's tier weight (unknown classification: 0).
pub fn compute_trust_score(record: &Value, weights: &TierWeights) -> f64 {
    let stars = confirmation_score(record);
    let tier = match record.get("classification").and_then(Value::as_str) {
        Some("foundational") => weights.foundational,
        Some("tactical") => weights.tactical,
        Some("observational") => weights.observational,
        _ => 0,
    };
    stars * weights.star as f64 + tier as f64
}

/// `sortByTrust`: stable score-descending order.
pub fn sort_by_trust(records: &[Value], weights: &TierWeights) -> Vec<Value> {
    let mut decorated: Vec<(usize, &Value)> = records.iter().enumerate().collect::<Vec<_>>();
    decorated.sort_by(|a, b| {
        let sa = compute_trust_score(a.1, weights);
        let sb = compute_trust_score(b.1, weights);
        sb.partial_cmp(&sa)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });
    decorated
        .into_iter()
        .map(|(_, record)| record.clone())
        .collect()
}

/// `whySurfaced`'s reason, in the reference's priority order.
#[derive(Clone, Debug, PartialEq)]
pub enum SurfaceReason {
    FileMatch { anchor: String },
    TrackerMatch { tracker: String, id: String },
    Stars { count: f64 },
    Recent { days_ago: i64 },
    Universal,
}

/// The literal `files` array of a record (NOT the registry's
/// per-type files field — the reference reads `r.files` directly).
fn literal_files(record: &Value) -> Option<&Vec<Value>> {
    record.get("files")?.as_array()
}

/// `whySurfaced`: the strongest signal wins, in reference order.
pub fn why_surfaced(record: &Value, ctx: Option<&ActiveContext>) -> SurfaceReason {
    if let Some(ctx) = ctx {
        if let Some(files) = literal_files(record)
            && !files.is_empty()
        {
            for file in files.iter().filter_map(Value::as_str) {
                if crate::git_context::file_matches_any(file, &ctx.changed_files) {
                    return SurfaceReason::FileMatch {
                        anchor: file.to_string(),
                    };
                }
            }
        }
        if let Some(anchors) = record.get("dir_anchors").and_then(Value::as_array)
            && !anchors.is_empty()
        {
            for anchor in anchors.iter().filter_map(Value::as_str) {
                for changed in &ctx.changed_files {
                    if crate::git_context::file_lives_under_dir(changed, anchor) {
                        return SurfaceReason::FileMatch {
                            anchor: anchor.to_string(),
                        };
                    }
                }
            }
        }
        for (tracker, active) in &ctx.trackers {
            let evidence = record
                .get("evidence")
                .and_then(|e| e.get(*tracker))
                .and_then(Value::as_str);
            if !active.is_empty() && evidence.is_some_and(|ev| ev == *active) {
                return SurfaceReason::TrackerMatch {
                    tracker: tracker.to_string(),
                    id:      active.clone(),
                };
            }
        }
    }
    let stars = confirmation_score(record);
    if stars > 0.0 {
        return SurfaceReason::Stars { count: stars };
    }
    if let Some(days) = days_ago(record)
        && days < RECENT_AUTHORSHIP_DAYS
    {
        return SurfaceReason::Recent {
            days_ago: days.max(0),
        };
    }
    SurfaceReason::Universal
}

/// Whole days since `recorded_at`; `None` when unparsable (the JS
/// `NaN` path — skips the recency arm).
fn days_ago(record: &Value) -> Option<i64> {
    let recorded = record
        .get("recorded_at")
        .and_then(Value::as_str)?
        .parse::<jiff::Timestamp>()
        .ok()?;
    let now = jiff::Timestamp::now();
    Some((now.as_millisecond() - recorded.as_millisecond()) / 86_400_000)
}

/// `formatSurfaceReason`: the agent-facing suffix line.
pub fn format_surface_reason(reason: &SurfaceReason) -> String {
    match reason {
        SurfaceReason::FileMatch { anchor } => format!("why: file match ({anchor})"),
        SurfaceReason::TrackerMatch { tracker, id } => {
            format!("why: in-progress {tracker}:{id}")
        }
        SurfaceReason::Stars { count } => {
            let stars = if count.fract() == 0.0 {
                format!("{count:.0}")
            } else {
                // toFixed(1): the only fractional shape is .5,
                // exactly representable — no rounding ambiguity.
                format!("{count:.1}")
            };
            format!("why: \u{2605}{stars} confirmations")
        }
        SurfaceReason::Recent { days_ago: 0 } => "why: recorded today".to_string(),
        SurfaceReason::Recent { days_ago } => format!("why: recorded {days_ago}d ago"),
        SurfaceReason::Universal => "why: applies broadly (no anchors)".to_string(),
    }
}

/// `buildSurfaceAnnotations`: per-record-id reason lines, in corpus
/// order; records without an id are skipped.
pub fn build_surface_annotations(
    records: &[Value],
    ctx: Option<&ActiveContext>,
) -> Vec<(String, String)> {
    records
        .iter()
        .filter_map(|record| {
            let id = record.get("id").and_then(Value::as_str)?;
            let reason = why_surfaced(record, ctx);
            Some((id.to_string(), format_surface_reason(&reason)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn weights() -> TierWeights {
        DEFAULT_TIER_WEIGHTS
    }

    /// Stars times 100 plus the tier; unknown classification adds
    /// nothing.
    #[test]
    fn trust_score_follows_the_weights() {
        let stars = json!({
            "type": "pattern", "classification": "tactical",
            "outcomes": [{"status": "success"}, {"status": "partial"}]
        });
        // 1 + 0.5 = 1.5 stars * 100 + 20
        assert_eq!(compute_trust_score(&stars, &weights()), 170.0);

        let unknown = json!({"type": "pattern", "classification": "mystic"});
        assert_eq!(compute_trust_score(&unknown, &weights()), 0.0);
    }

    /// Stable descending order with explicit tie handling.
    #[test]
    fn trust_sort_is_stable() {
        let tactical = json!({"type": "pattern", "classification": "tactical"});
        let foundational = json!({"type": "pattern", "classification": "foundational"});
        let tactical2 = json!({"type": "guide", "classification": "tactical"});
        let sorted = sort_by_trust(
            &[tactical.clone(), foundational.clone(), tactical2.clone()],
            &weights(),
        );
        assert_eq!(sorted[0], foundational);
        // Ties (20 == 20) keep corpus order.
        assert_eq!(sorted[1], tactical);
        assert_eq!(sorted[2], tactical2);
    }

    /// The reason priority: file match, then tracker, stars,
    /// recent, universal — and the exact wording.
    #[test]
    fn why_surfaced_priority_and_wording() {
        let plain = json!({"type": "convention", "recorded_at": "2020-01-01T00:00:00.000Z"});
        assert_eq!(
            format_surface_reason(&why_surfaced(&plain, None)),
            "why: applies broadly (no anchors)"
        );

        let ctx = ActiveContext {
            changed_files: vec!["src/a.rs".into()],
            trackers:      vec![("seeds", "mulch-1".into())],
        };
        let file_record = json!({"type": "pattern", "files": ["src/a.rs"]});
        assert_eq!(
            format_surface_reason(&why_surfaced(&file_record, Some(&ctx))),
            "why: file match (src/a.rs)"
        );
        // Suffix matching both directions (fileMatchesAny).
        let suffix = json!({"type": "pattern", "files": ["a.rs"]});
        assert!(matches!(
            why_surfaced(&suffix, Some(&ctx)),
            SurfaceReason::FileMatch { .. }
        ));

        let tracker_record = json!({
            "type": "decision",
            "evidence": {"seeds": "mulch-1"}
        });
        assert_eq!(
            format_surface_reason(&why_surfaced(&tracker_record, Some(&ctx))),
            "why: in-progress seeds:mulch-1"
        );

        let starred = json!({
            "type": "decision",
            "outcomes": [{"status": "success"}, {"status": "partial"}]
        });
        assert_eq!(
            format_surface_reason(&why_surfaced(&starred, None)),
            "why: \u{2605}1.5 confirmations"
        );

        let fresh = json!({
            "type": "decision",
            "recorded_at": (jiff::Timestamp::now() - jiff::Span::new().hours(24)).to_string()
        });
        assert_eq!(
            format_surface_reason(&why_surfaced(&fresh, None)),
            "why: recorded 1d ago"
        );
    }

    /// Annotations skip id-less records and keep corpus order.
    #[test]
    fn annotations_key_on_ids() {
        let records = vec![
            json!({"id": "a1", "type": "convention"}),
            json!({"type": "convention"}),
            json!({"id": "a2", "type": "convention"}),
        ];
        let annotations = build_surface_annotations(&records, None);
        assert_eq!(annotations.len(), 2);
        assert_eq!(annotations[0].0, "a1");
        assert_eq!(annotations[1].0, "a2");
    }
}
