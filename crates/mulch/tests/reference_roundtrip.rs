//! Round-trip battery against the reference `ml` CLI (0.10.7).
//!
//! The ADR-0023 acceptance gate, exercised end to end: our writer
//! produces a corpus, the reference CLI reads and mutates it, our
//! reader re-reads the result — and every additive/unknown field we
//! wrote survives the reference's rewrites.
//!
//! Skipped (with a note) when no `ml` binary is on PATH, so the unit
//! suite stays runnable in bare toolchain images.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use mulch::{Config, Record, RecordId, Store};

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

fn additive_record() -> Record {
    let mut record = Record::new("convention", "foundational", NOW);
    record.set("evidence", serde_json::json!({ "commit": "abc123" }));
    record.set("tags", serde_json::json!(["a", "b"]));
    record.set("dir_anchors", serde_json::json!(["crates/x"]));
    record.set("content", serde_json::Value::String("body text".into()));
    record.set_id(RecordId::parse("mx-e4f59f").unwrap());
    // Additive extension field (the sanctioned ADR-0023 mechanism).
    record.append(
        "fabricated_by",
        serde_json::Value::String("mulch-rs".into()),
    );
    record
}

fn write_corpus(dir: &Path, with_additive: bool) {
    let mut config = Config::default();
    config.add_domain("rust");
    let mut store = Store::create(dir, config).expect("store created");
    let mut convention = additive_record();
    if !with_additive {
        convention.remove("fabricated_by");
    }
    store
        .append_record("rust", convention)
        .expect("convention written");

    let mut failure = Record::new("failure", "tactical", NOW);
    failure.set("description", serde_json::Value::String("fail desc".into()));
    failure.set("resolution", serde_json::Value::String("fixed it".into()));
    failure.set_id(RecordId::parse("mx-7b33dd").unwrap());
    store
        .append_record("rust", failure)
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
        let store = Store::open(&dir.0).expect("reopen");
        let mut convention = store.records("rust").expect("rust domain")[0].clone();
        convention.append(
            "fabricated_by",
            serde_json::Value::String("mulch-rs".into()),
        );
        let mut failure = store.records("rust").expect("rust domain")[1].clone();
        failure.append(
            "fabricated_by",
            serde_json::Value::String("mulch-rs".into()),
        );
        let root = dir.0.join(".mulch/expertise/rust.jsonl");
        std::fs::write(
            &root,
            format!(
                "{}\n{}\n",
                convention.to_json_line(),
                failure.to_json_line()
            ),
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
    let store = Store::open(&dir.0).expect("reopen after reference mutations");
    assert_eq!(store.domains(), vec!["rust"]);
    let records = store.records("rust").expect("rust domain");
    let convention = records
        .iter()
        .find(|record| record.get("id").and_then(serde_json::Value::as_str) == Some("mx-e4f59f"))
        .expect("convention record survives");
    // ADR-0023 acceptance gate: the additive field outlived the
    // reference's rewrite of the file.
    assert_eq!(
        convention
            .get("fabricated_by")
            .and_then(serde_json::Value::as_str),
        Some("mulch-rs")
    );
    let outcomes = convention
        .get("outcomes")
        .expect("reference outcome visible");
    assert_eq!(
        outcomes
            .pointer("/0/status")
            .and_then(serde_json::Value::as_str),
        Some("success")
    );
    let restored = records
        .iter()
        .find(|record| record.get("id").and_then(serde_json::Value::as_str) == Some("mx-7b33dd"))
        .expect("restored failure is live again");
    assert_eq!(
        restored
            .get("fabricated_by")
            .and_then(serde_json::Value::as_str),
        Some("mulch-rs")
    );
    assert!(restored.get("status").is_none(), "archive fields stripped");
    assert!(
        records
            .iter()
            .any(|record| record.get("name").and_then(serde_json::Value::as_str) == Some("pat")),
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

    let store = Store::open(&dir.0).expect("reopen");
    store.write_all().expect("rewrite");

    let after_us = std::fs::read_to_string(&live_path).expect("rewritten file");
    assert_eq!(after_reference, after_us, "our rewrite is byte-identical");
}
