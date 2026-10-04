//! Round-trip battery against the reference `ml` CLI (0.10.7).
//!
//! The ADR-0023 acceptance gate, exercised end to end: our writer
//! produces a corpus, the reference CLI reads and mutates it, our
//! reader re-reads the result — and every additive/unknown field we
//! wrote survives the reference's rewrites.
//!
//! Works entirely on the one store owner ([`mulch::StoreFiles`]) and
//! the one record shape (`serde_json::Value`); the sprint-7
//! store-dual-owner collapse (mulch-bf22) deleted the parallel
//! `Store`/`Record` API this test used to exercise.
//!
//! Skipped (with a note) when no `ml` binary is on PATH, so the unit
//! suite stays runnable in bare toolchain images.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use mulch::{Config, StoreFiles, StoreLocation};
use serde_json::{Value, json};

fn reference_ml() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join("ml"))
        .find(|ml| ml.is_file())
}

fn ml(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("ml")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("ml spawns");
    let text = String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "ml {} failed in {}:\n{text}",
        args.join(" "),
        dir.display()
    );
    text
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("mulch-roundtrip-{tag}-{}", std::process::id()));
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

const NOW: &str = "2026-10-03T10:12:14.018Z";

/// Opens the store through the one seam the CLI uses.
fn open_store(dir: &Path) -> StoreFiles {
    match StoreFiles::locate(dir).expect("store locates") {
        StoreLocation::Open(store) => store,
        StoreLocation::Missing | StoreLocation::NoConfig => {
            panic!("store under {} must exist", dir.display())
        }
    }
}

/// The corpus' convention record as a compact JSONL line — field order
/// pinned from the old `Record` builder (canonical slots, additive
/// fields at the end).
fn convention_line(with_additive: bool) -> String {
    let mut record = json!({
        "type": "convention",
        "classification": "foundational",
        "recorded_at": NOW,
        "evidence": { "commit": "abc123" },
        "tags": ["a", "b"],
        "dir_anchors": ["crates/x"],
        "content": "body text",
        "id": "mx-e4f59f",
    });
    if with_additive {
        // Additive extension field (the sanctioned ADR-0023 mechanism).
        record["fabricated_by"] = json!("mulch-rs");
    }
    serde_json::to_string(&record).expect("compact line")
}

fn failure_line() -> String {
    serde_json::to_string(&json!({
        "type": "failure",
        "classification": "tactical",
        "recorded_at": NOW,
        "description": "fail desc",
        "resolution": "fixed it",
        "id": "mx-7b33dd",
    }))
    .expect("compact line")
}

fn write_corpus(dir: &Path, with_additive: bool) {
    std::fs::create_dir_all(dir.join(".mulch/expertise")).expect("expertise dir");
    let mut config = Config::default();
    config.add_domain("rust");
    std::fs::write(dir.join(".mulch/mulch.config.yaml"), config.to_yaml()).expect("config written");

    let store = open_store(dir);
    store
        .append_domain_line("rust", &convention_line(with_additive))
        .expect("convention written");
    store
        .append_domain_line("rust", &failure_line())
        .expect("failure written");
}

#[test]
fn reference_accepts_our_corpus_and_additive_fields_survive() {
    let Some(_ml) = reference_ml() else {
        let _ = writeln!(std::io::stderr(), "skipping: no reference `ml` on PATH");
        return;
    };
    let dir = TempDir::new("accept");

    // Our writer builds the whole store from scratch — schema-clean, so
    // the reference validator accepts it without errors.
    write_corpus(&dir.0, false);
    ml(&dir.0, &["validate"]);

    // Now add the ADR-0023 additive extension field with our writer.
    // The reference's schema validator flags additional properties even
    // on its own records, but the mutating commands accept and preserve
    // them — that is the acceptance gate.
    {
        let store = open_store(&dir.0);
        let mut lines: Vec<Value> = store
            .read_records("rust", false)
            .expect("rust domain")
            .into_iter()
            .map(|line| line.record)
            .collect();
        for record in &mut lines {
            record["fabricated_by"] = json!("mulch-rs");
        }
        let text = lines
            .iter()
            .map(|record| serde_json::to_string(record).expect("compact line"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(
            dir.0.join(".mulch/expertise/rust.jsonl"),
            format!("{text}\n"),
        )
        .expect("rewrite with additive fields");
    }

    // The reference mutates our corpus: outcome on our record id, a new
    // record, archive + restore of our failure record.
    ml(&dir.0, &[
        "outcome",
        "rust",
        "mx-e4f59f",
        "--status",
        "success",
        "--duration",
        "1200",
    ]);
    ml(&dir.0, &[
        "record",
        "rust",
        "--type",
        "pattern",
        "--name",
        "pat",
        "--description",
        "pat desc",
    ]);
    ml(&dir.0, &[
        "archive",
        "rust",
        "mx-7b33dd",
        "--reason",
        "roundtrip probe",
    ]);
    let archive_text =
        std::fs::read_to_string(dir.0.join(".mulch/archive/rust.jsonl")).expect("archive file");
    assert!(
        archive_text.starts_with("# ARCHIVED — not for active use."),
        "reference archive header expected"
    );
    ml(&dir.0, &["restore", "mx-7b33dd"]);

    // Our reader re-reads the mutated store.
    let store = open_store(&dir.0);
    assert_eq!(store.domains(), vec!["rust"]);
    let records: Vec<Value> = store
        .read_records("rust", false)
        .expect("rust domain")
        .into_iter()
        .map(|line| line.record)
        .collect();
    let convention = records
        .iter()
        .find(|record| record.get("id").and_then(Value::as_str) == Some("mx-e4f59f"))
        .expect("convention record survives");
    // ADR-0023 acceptance gate: the additive field outlived the
    // reference's rewrite of the file.
    assert_eq!(
        convention.get("fabricated_by").and_then(Value::as_str),
        Some("mulch-rs")
    );
    let outcomes = convention
        .get("outcomes")
        .expect("reference outcome visible");
    assert_eq!(
        outcomes.pointer("/0/status").and_then(Value::as_str),
        Some("success")
    );
    let restored = records
        .iter()
        .find(|record| record.get("id").and_then(Value::as_str) == Some("mx-7b33dd"))
        .expect("restored failure is live again");
    assert_eq!(
        restored.get("fabricated_by").and_then(Value::as_str),
        Some("mulch-rs")
    );
    assert!(restored.get("status").is_none(), "archive fields stripped");
    assert!(
        records
            .iter()
            .any(|record| record.get("name").and_then(Value::as_str) == Some("pat")),
        "reference-written pattern record is readable"
    );
}

#[test]
fn our_reader_rewrites_reference_corpus_byte_identically() {
    let Some(_ml) = reference_ml() else {
        let _ = writeln!(std::io::stderr(), "skipping: no reference `ml` on PATH");
        return;
    };
    let dir = TempDir::new("stable");
    write_corpus(&dir.0, true);
    // Reference rewrites the live file through its own serializer.
    ml(&dir.0, &[
        "record",
        "rust",
        "--type",
        "pattern",
        "--name",
        "p2",
        "--description",
        "d2",
    ]);
    let live_path = dir.0.join(".mulch/expertise/rust.jsonl");
    let after_reference = std::fs::read_to_string(&live_path).expect("live file");

    let store = open_store(&dir.0);
    let payload: Vec<Value> = store
        .read_records("rust", false)
        .expect("rust domain")
        .into_iter()
        .map(|line| line.record)
        .collect();
    store.rewrite_domain("rust", &payload).expect("rewrite");

    let after_us = std::fs::read_to_string(&live_path).expect("rewritten file");
    assert_eq!(after_reference, after_us, "our rewrite is byte-identical");
}
