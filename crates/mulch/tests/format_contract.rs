//! The ADR-0023 format contract, anchored by OUR tests (ADR-0008
//! point 2): what the old `reference_roundtrip.rs` proved END-TO-END
//! by spawning the retired `ml` binary is pinned here at the schema
//! level, through the one seam the CLI uses (`mulch::StoreFiles` and
//! the one record shape `serde_json::Value`).
//!
//! WHAT IS PINNED WHERE (mapping of the old roundtrip):
//! - Serialization laws (key order, one JSON object per line, trailing newline,
//!   timestamp shape) — `writer_corpus_…` below.
//! - ADDITIVE/unknown-field preservation through our own rewrite cycles (write
//!   → rewrite_domain → read back; append → read) — `additive_fields_survive_…`
//!   below.
//! - The config rewrite preserving unknown keys: already fully pinned in the
//!   lib as `config::tests::unknown_keys_and_knobs_survive_round_trip` (unknown
//!   top-level keys, `custom_types`, `prime`/`search` knobs through parse →
//!   to_yaml → reparse) and
//!   `store_files::tests::alias_block_round_trips_through_write_config` (the
//!   alias block survives write_config's canonical rewrite). CITED, not
//!   duplicated; the gap those leave — `register_domain`, the path
//!   `add`/`record` take, rewriting the config while unknown keys ride along —
//!   is pinned below in `register_domain_preserves_unknown_config_keys`.
//! - The reference's archive/restore/outcome mutations of our corpus were
//!   BEHAVIOR parity (ADR-0008 point 3 frees them); the store mechanics they
//!   exercised are pinned by the lib's `archive_records_*`, `revive_*`,
//!   `move_to_archive_*`, and `rewrite_archive_*` tests plus the cli battery's
//!   own pins.
//!
//! The `.mulch/` store stays ml-0.10.7-readable: additive changes
//! only (the ADR-0023/0006 line).

use std::path::Path;

use mulch::{StoreFiles, StoreLocation};
use serde_json::{Value, json};

mod common;

use common::TempDir;

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
    // Fresh store through the seam: a minimal parseable config seed,
    // then `register_domain` owns the canonical config write (the same
    // path the add/record commands take).
    std::fs::create_dir_all(dir.join(".mulch/expertise")).expect("expertise dir");
    std::fs::write(dir.join(".mulch/mulch.config.yaml"), "version: '1'\n").expect("config seed");
    let mut store = open_store(dir);
    store.register_domain("rust").expect("domain registered");
    store
        .append_domain_line("rust", &convention_line(with_additive))
        .expect("convention written");
    store
        .append_domain_line("rust", &failure_line())
        .expect("failure written");
}

/// The domain's records as plain values (the one record shape).
fn read_values(store: &StoreFiles, domain: &str) -> Vec<Value> {
    store
        .read_records(domain, false)
        .expect("domain readable")
        .into_iter()
        .map(|line| line.record)
        .collect()
}

/// A scratch dir holding the freshly written corpus (the TempDir must
/// outlive the test body — its Drop removes the tree).
fn scratch(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    write_corpus(&dir.0, false);
    dir
}

/// Serialization laws: our writer's domain file is one compact JSON
/// object per line, in the canonical key order, ending in exactly one
/// trailing newline, and `recorded_at` carries the millisecond ISO
/// shape the format contract names.
#[test]
fn writer_corpus_follows_the_serialization_laws() {
    let dir = scratch("contract-laws");
    let root = &dir.0;
    let store = open_store(root);
    assert_eq!(store.domains(), vec!["rust"]);
    let text =
        std::fs::read_to_string(root.join(".mulch/expertise/rust.jsonl")).expect("domain file");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "one object per record: {text}");
    assert_eq!(
        text,
        format!("{}\n{}\n", convention_line(false), failure_line()),
        "canonical key order, compact separators, one trailing newline"
    );
    for line in &lines {
        let value: Value = serde_json::from_str(line).expect("each line is one JSON object");
        let recorded = value["recorded_at"].as_str().expect("recorded_at");
        assert!(
            recorded.len() == 24
                && recorded.ends_with('Z')
                && recorded.as_bytes()[10] == b'T'
                && recorded.as_bytes()[19] == b'.',
            "millisecond ISO timestamp shape: {recorded}"
        );
    }
    // the config seed became the canonical config (version + domain)
    let config =
        std::fs::read_to_string(root.join(".mulch/mulch.config.yaml")).expect("config file");
    assert!(
        config.starts_with("version: '1'\ndomains:\n  rust: {}\n"),
        "{config}"
    );
}

/// ADDITIVE fields survive our own rewrite cycle: read →
/// rewrite_domain → read back preserves the unknown key, its value,
/// AND its trailing position in the key order (the ADR-0023
/// acceptance the old roundtrip proved against the reference).
#[test]
fn additive_fields_survive_the_rewrite_cycle() {
    let dir = TempDir::new("contract-additive");
    write_corpus(&dir.0, true);
    let root = &dir.0;
    let store = open_store(root);
    // inject the additive field into EVERY record (the failure record
    // of the corpus does not carry it from the writer), then rewrite
    // through the same owner the mutating commands use
    let mut payload = read_values(&store, "rust");
    assert_eq!(payload.len(), 2);
    for record in &mut payload {
        record["fabricated_by"] = json!("mulch-rs");
    }
    store
        .rewrite_domain("rust", &payload)
        .expect("rewrite cycle");
    let after = read_values(&store, "rust");
    for record in &after {
        assert_eq!(
            record.get("fabricated_by").and_then(Value::as_str),
            Some("mulch-rs"),
            "the additive field outlived our rewrite: {record}"
        );
    }
    // byte law: the rewrite is compact and order-preserving — the
    // additive key rides at the END of each line
    let text =
        std::fs::read_to_string(root.join(".mulch/expertise/rust.jsonl")).expect("domain file");
    for line in text.lines() {
        assert!(
            line.ends_with("\"fabricated_by\":\"mulch-rs\"}"),
            "additive keys keep their trailing position: {line}"
        );
    }
}

/// ADDITIVE fields survive the append path too: a fresh record
/// written by `append_domain_line` keeps whatever unknown keys the
/// caller provided, verbatim.
#[test]
fn additive_fields_survive_the_append_path() {
    let dir = scratch("contract-append");
    let root = &dir.0;
    let store = open_store(root);
    let mut extended = json!({
        "type": "guide",
        "classification": "tactical",
        "recorded_at": NOW,
        "name": "Guide One",
        "description": "gd",
        "id": "mx-guid01",
    });
    extended["vendor_extra"] = json!({"kept": true});
    store
        .append_domain_line("rust", &serde_json::to_string(&extended).expect("compact"))
        .expect("guide written");
    let records = read_values(&store, "rust");
    let guide = records
        .iter()
        .find(|record| record.get("id").and_then(Value::as_str) == Some("mx-guid01"))
        .expect("guide readable");
    assert_eq!(guide["vendor_extra"]["kept"], json!(true));
}

/// The config gap the lib tests leave open: `register_domain` — the
/// path `add`/`record` take — rewrites the config canonically while
/// unknown top-level keys (the additive-config mechanism) ride along
/// byte-preserved. (The parse→serialize round trip is pinned by
/// `config::tests::unknown_keys_and_knobs_survive_round_trip`; the
/// alias block's survival by
/// `store_files::tests::alias_block_round_trips_through_write_config`.)
#[test]
fn register_domain_preserves_unknown_config_keys() {
    let dir = TempDir::new("contract-config");
    std::fs::create_dir_all(dir.0.join(".mulch/expertise")).expect("expertise dir");
    std::fs::write(
        dir.0.join(".mulch/mulch.config.yaml"),
        "version: '1'\ndomains:\n  rust: {}\nfuture_extension:\n  knob: bespoke\n",
    )
    .expect("config seed");
    let mut store = open_store(&dir.0);
    store.register_domain("tooling").expect("domain registered");
    let config =
        std::fs::read_to_string(dir.0.join(".mulch/mulch.config.yaml")).expect("config file");
    assert!(
        config.contains("future_extension:\n  knob: bespoke\n"),
        "the unknown top-level block survives the canonical rewrite: {config}"
    );
    assert!(
        config.contains("  tooling: {}"),
        "the new domain registered: {config}"
    );
    // the rewritten config still parses through the same seam
    let store = open_store(&dir.0);
    assert_eq!(store.domains(), vec!["rust", "tooling"]);
}
