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
    #[must_use]
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
        let (days, shelf_class) = match classification {
            "tactical" => (self.tactical_days, "tactical"),
            "observational" => (self.observational_days, "observational"),
            // foundational and anything unknown never decay
            _ => return StaleVerdict::Fresh,
        };
        // The reference's `new Date(record.recorded_at)` is the full
        // JS Date constructor coercion ([`crate::date_ms`], mulch-c5c4):
        // date-only stamps, the space separator, zone-less civil time,
        // and the null/boolean/number coercions (null is the epoch —
        // ancient, therefore stale) all parse where a strict ISO read
        // returned None; `None` here is the JS NaN (missing field,
        // garbage strings) — the never-stale arm below. This one
        // extraction owner also aligns doctor's stale check and the
        // future prune: every isRecordStale/isStale consumer reads the
        // reference's `new Date(...)` everywhere.
        let Some(recorded_ms) = crate::date_ms(record) else {
            return StaleVerdict::Unparsable;
        };
        // Reference boundary: Math.floor(age_in_days) > shelf — a
        // record goes stale at the FIRST instant of the (shelf+1)-th
        // day, not after shelf days exactly. The quotient runs in
        // f64 milliseconds and floors exactly like the reference's
        // division (both operands are TimeClip-bounded, so the span
        // is finite), and an absurd day count saturates the shelf
        // past every finite age — a gigantic shelf is never stale.
        let age_days = ((crate::timestamp_ms(now) - recorded_ms) / 86_400_000.0).floor();
        if age_days > days as f64 {
            StaleVerdict::Stale {
                classification: shelf_class,
            }
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
    /// Past its classification's shelf life; carries the expired
    /// classification. Only the decaying classifications go stale, so
    /// the payload is always `"tactical"` or `"observational"` — a
    /// stale verdict never has to re-extract the record's field.
    Stale {
        /// The expired classification.
        classification: &'static str,
    },
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
        assert_eq!(rule.verdict(&at(15), now), StaleVerdict::Stale {
            classification: "tactical",
        });
        assert_eq!(rule.verdict(&at(40), now), StaleVerdict::Stale {
            classification: "tactical",
        });
        // observational: fresh at 30, stale at 31
        let obs = |days: i64| json!({"type": "pattern", "classification": "observational", "recorded_at": ago(days)});
        assert_eq!(rule.verdict(&obs(30), now), StaleVerdict::Fresh);
        assert_eq!(rule.verdict(&obs(31), now), StaleVerdict::Stale {
            classification: "observational",
        });
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

    #[test]
    fn absurd_day_counts_never_overflow_the_verdict() {
        // a config saturating the shelf past i64 never inverts the
        // verdict (no panic in debug, no wrap in release): the record
        // stays fresh
        let rule = StaleRule {
            tactical_days:      i64::MAX,
            observational_days: i64::MAX,
        };
        let mut record = serde_json::json!({
            "classification": "tactical",
            "recorded_at": "2020-01-01T00:00:00.000Z",
        });
        assert_eq!(
            rule.verdict(&record, jiff::Timestamp::now()),
            StaleVerdict::Fresh
        );
        record["recorded_at"] = serde_json::json!("not a timestamp");
        assert_eq!(
            rule.verdict(&record, jiff::Timestamp::now()),
            StaleVerdict::Unparsable
        );
    }
}
