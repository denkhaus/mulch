//! The archive read half (mulch-27ec): `.mulch/archive/
//! <domain>.jsonl` reads through the same line law as live reads —
//! `#` banner and blank lines skip, aliases apply — but ALWAYS
//! tolerate unknown types (`readArchiveFile`'s hard-wired escape
//! hatch). The archive WRITE side is 7c55's; these pins hold the
//! read law the `search --archived` consumer builds on. Probed
//! against ml 0.10.7, 2026-10-18 (/tmp/sprint34/c2-probes).

use std::path::Path;

use mulch::{StoreFiles, StoreLocation};
use serde_json::Value;

mod common;

use common::TempDir;

fn store_in(dir: &Path, custom_types: &str) -> StoreFiles {
    std::fs::create_dir_all(dir.join(".mulch/expertise")).expect("expertise dir");
    std::fs::write(
        dir.join(".mulch/mulch.config.yaml"),
        format!("version: '1'\ndomains:\n  alpha: {{}}{custom_types}"),
    )
    .expect("config seed");
    match StoreFiles::locate(dir).expect("store locates") {
        StoreLocation::Open(store) => store,
        StoreLocation::Missing | StoreLocation::NoConfig => panic!("store must exist"),
    }
}

fn write_archive(dir: &Path, domain: &str, lines: &str) {
    let path = dir.join(".mulch/archive").join(format!("{domain}.jsonl"));
    std::fs::create_dir_all(path.parent().expect("archive dir")).expect("archive dir");
    std::fs::write(path, lines).expect("archive written");
}

fn types(records: &[mulch::LineRecord]) -> Vec<String> {
    records
        .iter()
        .map(|line| {
            line.record
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        })
        .collect()
}

/// The banner, blank lines, and physical line numbers survive the
/// shared reader; archived records carry their stamps through
/// untouched.
#[test]
fn archive_read_skips_banner_and_keeps_line_numbers() {
    let dir = TempDir::new("banner");
    let store = store_in(&dir.0, "");
    write_archive(
        &dir.0,
        "alpha",
        concat!(
            "# ARCHIVED — not for active use. Run `ml restore <id>` to revive.\n",
            "\n",
            "{\"type\":\"pattern\",\"name\":\"A\",\"description\":\"d\",\"recorded_at\":\"2026-01-01T10:00:00.000Z\",\"id\":\"mx-aaaaaa\",\"status\":\"archived\",\"archived_at\":\"2026-10-18T16:01:59.865Z\",\"archive_reason\":\"manual: probe\"}\n",
        ),
    );
    let records = store.read_archive("alpha").expect("archive reads");
    assert_eq!(types(&records), ["pattern"]);
    assert_eq!(records[0].line, 3);
    assert_eq!(
        records[0]
            .record
            .get("archive_reason")
            .and_then(Value::as_str),
        Some("manual: probe")
    );
}

/// Unknown types ALWAYS pass on the archive path (the live read's
/// `--allow-unknown-types` hatch, hard-wired here) — a type removed
/// from the config after archiving must not brick the archive read.
#[test]
fn archive_read_tolerates_unknown_types() {
    let dir = TempDir::new("unknown");
    let store = store_in(&dir.0, "");
    write_archive(
        &dir.0,
        "alpha",
        "{\"type\":\"hypothesis\",\"prediction\":\"p\",\"recorded_at\":\"2026-01-01T10:00:00.000Z\"}\n",
    );
    let records = store.read_archive("alpha").expect("archive reads");
    assert_eq!(types(&records), ["hypothesis"]);
    // the LIVE read of the same shape stays gated (the escape hatch
    // is archive-only)
    std::fs::write(
        dir.0.join(".mulch/expertise/alpha.jsonl"),
        "{\"type\":\"hypothesis\",\"prediction\":\"p\",\"recorded_at\":\"2026-01-01T10:00:00.000Z\"}\n",
    )
    .expect("live file");
    assert!(store.read_records("alpha", false).is_err());
}

/// Read-time aliases apply on the archive path too (the shared read
/// seam's law — a field renamed after archiving canonicalizes).
#[test]
fn archive_read_applies_aliases() {
    let dir = TempDir::new("aliases");
    let custom = "\ncustom_types:\n  recipe:\n    required: [dish]\n    dedup_key: dish\n    summary: \"{dish}\"\n    aliases:\n      dish: [plate]\n";
    let store = store_in(&dir.0, custom);
    write_archive(
        &dir.0,
        "alpha",
        "{\"type\":\"recipe\",\"plate\":\"soup\",\"recorded_at\":\"2026-01-01T10:00:00.000Z\"}\n",
    );
    let records = store.read_archive("alpha").expect("archive reads");
    assert_eq!(
        records[0].record.get("dish").and_then(Value::as_str),
        Some("soup")
    );
}

/// No archive file: an empty corpus, not an error (the restore /
/// `--archived` consumers build on the empty case).
#[test]
fn archive_read_missing_is_empty() {
    let dir = TempDir::new("missing");
    let store = store_in(&dir.0, "");
    assert!(
        store
            .read_archive("alpha")
            .expect("archive reads")
            .is_empty()
    );
    assert!(store.archive_path("alpha").ends_with("archive/alpha.jsonl"));
}
