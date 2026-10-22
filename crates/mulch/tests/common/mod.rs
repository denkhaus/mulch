//! Shared integration-test scaffolding. Each test binary includes
//! this module (`mod common;` from `tests/*.rs`, or
//! `#[path = "../common/mod.rs"]` from inside `tests/cli_parity/`)
//! — the helper is byte-identical everywhere, the pid in the path
//! keeps parallel binaries from colliding.

#![allow(dead_code, reason = "each test binary uses the subset it needs")]

use std::path::PathBuf;

pub(crate) struct TempDir(pub(crate) PathBuf);

impl TempDir {
    pub(crate) fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("mulch-test-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("temp dir");
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}
