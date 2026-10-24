#![allow(clippy::print_stderr, reason = "skip notices print to stderr")]

//! Differential battery: our binary vs the reference `ml` 0.10.7
//! (mulch-da8b). Runs the same commands in twin temp stores and
//! compares exit codes, channel-split output, and on-disk state.
//! Skipped with a note when no `ml` is on PATH. ONE test binary via
//! the module layout (mulch-3e7c): per-surface modules plus the
//! shared `support` harness.

mod archive;
mod compact;
mod custom_types;
mod delete;
mod doctor;
mod edit;
mod init;
mod learn;
mod move_cmd;
mod outcome;
mod prime;
mod prune;
mod query;
mod rank;
mod ready;
mod record;
mod restore;
mod search;
mod status;
mod support;
mod validate;
