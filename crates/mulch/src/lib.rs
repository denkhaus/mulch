//! mulch — native Rust implementation of the mulch structured-expertise
//! format.
//!
//! Format-compatibility contract (README): read+write drop-in compatible
//! with `@os-eco/mulch-cli` 0.10.7 (`.mulch/` directory:
//! `mulch.config.yaml`, `expertise/<domain>.jsonl`, `archive/`).
//! Unknown record fields are preserved on every write; additive fields
//! are the only sanctioned extension mechanism.
//!
//! The format core lives here — store IO, the record-type registry,
//! and the record-domain laws (schema validity in [`schema`], anchor
//! validity in [`anchors`], staleness decay in [`stale`]) plus the
//! lifecycle hook engine ([`hooks`], ADR-0002); CLI parity and the
//! self-hosting cutover are tracked in this repo's Seeds
//! tracker (seed ids `mulch-…`).

pub mod anchors;
pub mod audit;
pub mod bm25;
pub mod budget;
pub mod compact;
mod config;
mod custom_types;
mod error;
pub mod git_context;
pub mod hooks;
mod ids;
pub mod json_error;
pub mod jsval;
pub mod lock;
pub mod prime_ranking;
pub mod query;
mod records;
mod repair;
pub mod schema;
pub mod scoring;
pub mod stale;
mod store_files;
mod time;
mod upsert;

pub use config::{
    AuditConfig, AuditThresholds, Config, DEFAULT_AUDIT_THRESHOLDS, Governance, Prime,
    ResolvedAuditThresholds, SUPPORTED_VERSION, Search, SessionClose, ShelfLife, Tier, TierWeights,
    resolve_audit_thresholds,
};
pub use error::{Error, Result};
pub use ids::{
    CompactStrategy, PAYLOAD_TYPES, REGISTRY, Registry, Row, SummaryLaw, SummaryPart, TypeSpec,
    effective_type, record_id,
};
// the JS-quirk facade: the command-consumed jsval items re-export at
// the root like the records/schema constants (one convention;
// `mulch::jsval` stays the deep path for the rest, e.g. `js_dedup_eq`)
#[doc(inline)]
pub use jsval::{
    days_u64, js_key_sorted, js_length, js_number, js_rel, js_round_u64, js_string, js_truncate,
    parse_duration, parse_non_negative_number, parse_strict_positive_int, truthy, truthy_str,
    value_text,
};
pub use query::{DomainSpec, RecordFilter, UnknownDomain, domain_set};
pub use records::{
    Counter, LenientLine, LineRecord, ResolveError, SummaryError, apply_aliases, apply_aliases_for,
    legacy_entry, legacy_outcome, read_lenient, read_strict, record_summary, record_summary_value,
    resolve_record_id,
};
pub use repair::{LineAction, parsed_records, raw_line_pass, value_line_pass};
pub use schema::CLASSIFICATIONS;
pub use scoring::{apply_confirmation_boost, confirmation_score};
pub use store_files::{StoreFiles, StoreLocation, TransactionError};
pub use time::{date_ms, js_date_ms, now_ms, parse_js_date, time_ago, time_ago_ts, timestamp_ms};
pub use upsert::{
    OutcomeEntry, UpsertPlan, assign_missing_id, ensure_write_id, find_duplicate, merge_outcomes,
    upsert_plan, write_records,
};

/// The reference implementation this crate is read+write compatible with
/// (README format-compatibility promise; ADR-0023 in denkhaus/fabro).
pub const COMPAT_TARGET: &str = "@os-eco/mulch-cli 0.10.7";

#[cfg(test)]
mod tests {
    use super::COMPAT_TARGET;

    #[test]
    fn compat_target_is_pinned() {
        assert_eq!(COMPAT_TARGET, "@os-eco/mulch-cli 0.10.7");
    }
}
