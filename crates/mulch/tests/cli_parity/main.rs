//! The behavior battery (ADR-0008): OUR binary, OUR byte pins. The
//! differential regime against the reference `ml` 0.10.7 is history
//! (mulch-da8b started it; the sweep of mulch-ed39 froze every
//! surviving expectation into these modules); the format contract
//! lives in `tests/format_contract.rs`. ONE test binary via the
//! module layout (mulch-3e7c): per-surface modules plus the shared
//! `support` harness.

mod aliases;
mod archive;
mod audit;
mod compact;
mod completions;
mod config;
mod custom_types;
mod delete;
mod diff_cmd;
mod doctor;
mod edit;
mod init;
mod learn;
mod move_cmd;
mod onboard;
mod outcome;
mod prime;
mod prune;
mod query;
mod rank;
mod ready;
mod record;
mod restore;
mod search;
mod setup;
mod status;
mod support;
mod sync;
mod update;
mod upgrade;
mod validate;
