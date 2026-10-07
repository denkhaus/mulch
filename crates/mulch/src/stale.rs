//! The staleness law of the format: the shelf-life decay rule.
//!
//! Tactical records expire after 14 days, observational after 30
//! (reference defaults); foundational records do not decay. Config
//! `classification_defaults.shelf_life` overrides the day counts. The
//! CLI's `status` and `doctor` render from this rule; embedders
//! consume it directly.

use jiff::Timestamp;
use serde_json::Value;

use crate::config::ShelfLife;

/// The staleness rule derived from a store's shelf-life config.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StaleRule {
    tactical_days:      i64,
    observational_days: i64,
}

impl StaleRule {
    /// From the config's effective shelf life (the config module owns
    /// the reference defaults — mulch-53cb; absurd day counts beyond
    /// i64 saturate: a gigantic shelf is never stale).
    pub fn from_shelf_life(shelf_life: &ShelfLife) -> Self {
        Self {
            tactical_days:      i64::try_from(shelf_life.tactical).unwrap_or(i64::MAX),
            observational_days: i64::try_from(shelf_life.observational).unwrap_or(i64::MAX),
        }
    }

    /// The record's verdict — the ONE owner of the classification and
    /// recorded_at extraction (status, doctor's check and doctor --fix
    /// all render from this; reference `isStale`/`isRecordStale` are
    /// identical twins over prune.ts and utils/expertise.ts).
    pub fn verdict(&self, record: &Value, now: Timestamp) -> StaleVerdict {
        let Some(classification) = record.get("classification").and_then(Value::as_str) else {
            return StaleVerdict::Fresh;
        };
        let days = match classification {
            "tactical" => self.tactical_days,
            "observational" => self.observational_days,
            // foundational and anything unknown never decay
            _ => return StaleVerdict::Fresh,
        };
        let Some(recorded) = record
            .get("recorded_at")
            .and_then(Value::as_str)
            .and_then(|raw| raw.parse::<Timestamp>().ok())
        else {
            return StaleVerdict::Unparsable;
        };
        // Reference boundary: Math.floor(age_in_days) > shelf — a
        // record goes stale at the FIRST instant of the (shelf+1)-th
        // day, not after shelf days exactly.
        if now >= recorded + jiff::Span::new().hours((days + 1) * 24) {
            StaleVerdict::Stale
        } else {
            StaleVerdict::Fresh
        }
    }
}

/// One record's staleness verdict.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StaleVerdict {
    /// Never decays: foundational, unknown/missing/non-string
    /// classification, or within shelf life.
    Fresh,
    /// Past its classification's shelf life.
    Stale,
    /// recorded_at missing or unparsable — the age is unknown, so the
    /// record never decays (the reference's NaN comparisons are
    /// false); schema validation owns reporting it.
    Unparsable,
}

impl Default for StaleRule {
    fn default() -> Self {
        Self::from_shelf_life(&ShelfLife::reference_default())
    }
}

#[cfg(test)]
mod tests {
    use jiff::Timestamp;
    use serde_json::json;

    use super::{StaleRule, StaleVerdict};

    fn rule() -> StaleRule {
        StaleRule::default()
    }

    fn now() -> Timestamp {
        "2026-10-06T12:00:00.000Z".parse().expect("now parses")
    }

    fn ago(days: i64) -> String {
        let span = jiff::Span::new().hours(days * 24);
        (now() - span).to_string()
    }

    #[test]
    fn unknown_or_missing_classification_never_decays() {
        let rule = rule();
        let now = now();
        // missing classification, 100 days old
        let missing = json!({"type": "pattern", "recorded_at": ago(100)});
        assert_eq!(rule.verdict(&missing, now), StaleVerdict::Fresh);
        // unknown classification
        let unknown =
            json!({"type": "pattern", "classification": "mystic", "recorded_at": ago(100)});
        assert_eq!(rule.verdict(&unknown, now), StaleVerdict::Fresh);
        // foundational with a garbage timestamp stays fresh
        let foundational =
            json!({"type": "pattern", "classification": "foundational", "recorded_at": "nope"});
        assert_eq!(rule.verdict(&foundational, now), StaleVerdict::Fresh);
    }

    #[test]
    fn stale_starts_at_the_shelf_plus_one_day_boundary() {
        let rule = rule();
        let now = now();
        let at = |days: i64| json!({"type": "pattern", "classification": "tactical", "recorded_at": ago(days)});
        // reference Math.floor(age) > shelf: 14.0 and 14.9 days are
        // fresh, the 15th day is stale
        assert_eq!(rule.verdict(&at(14), now), StaleVerdict::Fresh);
        let mid = {
            let recorded = now - jiff::Span::new().hours(14 * 24 + 12);
            json!({"type": "pattern", "classification": "tactical", "recorded_at": recorded.to_string()})
        };
        assert_eq!(rule.verdict(&mid, now), StaleVerdict::Fresh);
        assert_eq!(rule.verdict(&at(15), now), StaleVerdict::Stale);
        assert_eq!(rule.verdict(&at(40), now), StaleVerdict::Stale);
        // observational: fresh at 30, stale at 31
        let obs = |days: i64| json!({"type": "pattern", "classification": "observational", "recorded_at": ago(days)});
        assert_eq!(rule.verdict(&obs(30), now), StaleVerdict::Fresh);
        assert_eq!(rule.verdict(&obs(31), now), StaleVerdict::Stale);
    }

    #[test]
    fn unparsable_recorded_at_is_never_stale() {
        let rule = rule();
        let now = now();
        let garbage =
            json!({"type": "pattern", "classification": "tactical", "recorded_at": "nope"});
        assert_eq!(rule.verdict(&garbage, now), StaleVerdict::Unparsable);
        let absent = json!({"type": "pattern", "classification": "tactical"});
        assert_eq!(rule.verdict(&absent, now), StaleVerdict::Unparsable);
    }

    #[test]
    fn non_string_classification_and_future_records_stay_fresh() {
        let rule = rule();
        let now = now();
        // a non-string classification never decays
        let numeric = json!({"type": "pattern", "classification": 42, "recorded_at": ago(100)});
        assert_eq!(rule.verdict(&numeric, now), StaleVerdict::Fresh);
        // a future recorded_at has a negative age — floor() makes it
        // never greater than the shelf
        let future =
            json!({"type": "pattern", "classification": "tactical", "recorded_at": ago(-5)});
        assert_eq!(rule.verdict(&future, now), StaleVerdict::Fresh);
    }
}
