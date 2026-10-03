//! mulch — native Rust implementation of the mulch structured-expertise
//! format.
//!
//! Format-compatibility contract (README): read+write drop-in compatible
//! with `@os-eco/mulch-cli` 0.10.7 (`.mulch/` directory:
//! `mulch.config.yaml`, `expertise/<domain>.jsonl`, `archive/`).
//! Unknown record fields are preserved on every write; additive fields
//! are the only sanctioned extension mechanism.
//!
//! The format core lives here; CLI parity and the self-hosting cutover
//! are tracked in this repo's Seeds tracker (seed ids `mulch-…`).

mod config;
mod error;
mod ids;
mod record;
mod records;
mod store;
mod store_files;

pub use config::{Config, Governance, Prime, SUPPORTED_VERSION, Search, ShelfLife, TierWeights};
pub use error::{Error, Result};
pub use ids::{PAYLOAD_TYPES, id_key_field, payload_fields, record_id};
pub use record::{Record, RecordId};
pub use records::{
    LineRecord, ResolveError, assign_missing_id, read_strict, record_summary, resolve_record_id,
    write_records,
};
pub use store::Store;
pub use store_files::{Located, ReadPolicy, StoreFiles};

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
