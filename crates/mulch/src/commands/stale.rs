//! The shelf-life staleness rule, shared by `status` and `doctor`.
//!
//! Tactical records expire after 14 days, observational after 30
//! (reference defaults); foundational records do not decay. Config
//! `classification_defaults.shelf_life` overrides the day counts.

use jiff::Timestamp;
use mulch::ShelfLife;

/// The staleness rule derived from a store's shelf-life config.
#[derive(Clone, Copy)]
pub(crate) struct StaleRule {
    tactical_days:      i64,
    observational_days: i64,
}

impl StaleRule {
    /// From config values, falling back to the reference defaults.
    pub(crate) fn from_config(shelf_life: Option<&ShelfLife>) -> Self {
        match shelf_life {
            Some(shelf) => Self {
                tactical_days:      i64::try_from(shelf.tactical).unwrap_or(14),
                observational_days: i64::try_from(shelf.observational).unwrap_or(30),
            },
            None => Self::default(),
        }
    }

    /// Whether a record of `classification` recorded at `recorded` is
    /// stale at `now`.
    pub(crate) fn is_stale(
        &self,
        classification: &str,
        recorded: Timestamp,
        now: Timestamp,
    ) -> bool {
        let days = match classification {
            "tactical" => self.tactical_days,
            "observational" => self.observational_days,
            _ => return false,
        };
        now >= recorded + jiff::Span::new().hours(days * 24)
    }
}

impl Default for StaleRule {
    fn default() -> Self {
        Self {
            tactical_days:      14,
            observational_days: 30,
        }
    }
}
