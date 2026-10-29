//! `mulch status` pins (ADR-0008 edition: OUR byte pins only).
//!
//! Fresh-store and no-store surfaces are byte-frozen; the reader
//! surfaces pin the exact error texts and the json health-block laws
//! (raw stamp strings, distribution keys); relative-age wording
//! ("Xd ago") is volatile by design and pinned structurally.

use std::path::Path;

use serde_json::json;

use super::support::*;

#[test]
fn status_no_store_error_pins_both_channels() {
    let ours = TempDir::new("nostore-ours");
    let plain = run_in(&ours.0, Path::new(mulch_bin()), &["status"]);
    assert_eq!(plain.code, 1);
    assert_eq!(plain.stdout, "");
    assert_eq!(
        plain.stderr,
        "No .mulch/ directory found. Run `mulch init` first.\n"
    );
    let json = run_in(&ours.0, Path::new(mulch_bin()), &["status", "--json"]);
    assert_eq!(json.code, 1);
    assert_eq!(json.stdout, "");
    assert_eq!(
        json.stderr,
        error_envelope(
            "status",
            "No .mulch/ directory found. Run `mulch init` first."
        )
    );
}

#[test]
fn status_fresh_store_pins_the_empty_table() {
    let ours = TempDir::new("fresh-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let plain = run_in(&ours.0, Path::new(mulch_bin()), &["status"]);
    assert_eq!(plain.code, 0);
    assert_eq!(
        plain.stdout,
        "Mulch Status\n============\n\nNo domains configured. Run `mulch add <domain>` to get \
         started.\n"
    );
    let json = run_in(&ours.0, Path::new(mulch_bin()), &["status", "--json"]);
    assert_eq!(json.code, 0);
    assert_eq!(
        normalize(&json.stdout),
        "{\n  \"success\": true,\n  \"command\": \"status\",\n  \"domains\": [],\n  \
         \"governance\": {\n    \"max_entries\": 100,\n    \"warn_entries\": 150,\n    \
         \"hard_limit\": 200\n  },\n  \"shelf_life\": {\n    \"tactical\": 14,\n    \
         \"observational\": 30\n  }\n}\n"
    );
}

/// A populated store: the plain line shape (counts, volatile ages
/// unpinned) and the json health-block key law.
#[test]
fn status_with_records_pins_the_line_and_json_laws() {
    let ours = TempDir::new("recs-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "dev",
        "--type",
        "pattern",
        "--name",
        "p1",
        "--description",
        "dp1",
    ]);
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "dev",
        "--type",
        "convention",
        "--content",
        "c1",
    ]);

    let plain = run_in(&ours.0, Path::new(mulch_bin()), &["status"]);
    assert_eq!(plain.code, 0);
    assert_eq!(
        plain.stdout.lines().count(),
        4,
        "header, rule, blank, one domain line: {}",
        plain.stdout
    );
    assert!(plain.stdout.contains("dev: 2 records"));

    // json: the domain row's key order and the distributions (the
    // integer-first type key is JS object insertion order).
    let json = run_in(&ours.0, Path::new(mulch_bin()), &["status", "--json"]);
    assert_eq!(json.code, 0);
    let value: serde_json::Value = serde_json::from_str(&json.stdout).expect("json parses");
    let row = &value["domains"][0];
    assert_eq!(row["domain"], json!("dev"));
    assert_eq!(row["count"], json!(2));
    let keys = row
        .as_object()
        .expect("row")
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        keys,
        [
            "domain",
            "count",
            "lastUpdated",
            "oldest_recorded",
            "newest_recorded",
            "rotting",
            "rotting_days",
            "health"
        ]
        .map(String::from)
        .to_vec()
    );
    let distribution = row["health"]["type_distribution"]
        .as_object()
        .expect("distribution")
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        distribution,
        [
            "convention",
            "pattern",
            "failure",
            "decision",
            "reference",
            "guide"
        ]
        .map(String::from)
        .to_vec()
    );
}

/// The reader surfaces over a fixed-stamp corpus: the array line
/// counts, the non-string type renders its raw key, the file-less
/// domain says "never", and the rotting banner fires.
#[test]
fn status_reader_surfaces_pin_the_laws() {
    let ours = TempDir::new("stat-rd-ours");
    let config = format!("version: '1'\ndomains:\n  d1: {{}}\n  d2: {{}}\n{CONFIG_TAIL}");
    write_seed_with_config(&ours.0, &config, &[]);
    let corpus = "{\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"2026-08-26T08:00:00.000Z\",\"content\":\"old\",\"id\":\"mx-bbbbbb\"}\n{\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"2026-08-27T08:00:00.000Z\",\"content\":\"new\",\"id\":\"mx-cccccc\"}\n[1,2,3]\n{\"type\":5,\"classification\":\"tactical\",\"recorded_at\":\"2026-08-26T08:00:00.000Z\",\"content\":\"numtype\",\"id\":\"mx-333333\"}\n";
    std::fs::write(ours.0.join(".mulch/expertise/d1.jsonl"), corpus).expect("corpus writable");
    // d2 keeps no file; a file-less domain renders "never"
    std::fs::write(ours.0.join(".mulch/expertise/d2.jsonl"), "").expect("d2 seed");
    std::fs::remove_file(ours.0.join(".mulch/expertise/d2.jsonl")).expect("d2 file removable");

    // plain: count includes the array line, the range renders, the
    // file-less domain says "never" (ages stay volatile)
    let plain = run_in(&ours.0, Path::new(mulch_bin()), &["status"]);
    assert_eq!(plain.code, 0);
    let d1_line = plain
        .stdout
        .lines()
        .find(|line| line.starts_with("  d1:"))
        .expect("d1 line");
    assert!(
        d1_line.starts_with("  d1: 4 records (updated just now) — recorded"),
        "{d1_line}"
    );
    assert!(d1_line.contains("⚠ ROTTING"), "{d1_line}");
    assert!(plain.stdout.contains("  d2: 0 records (updated never)"));

    // json: same story — raw stamps, both extra distribution keys
    let json = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "status"]);
    assert_eq!(json.code, 0);
    let value: serde_json::Value = serde_json::from_str(&json.stdout).expect("json parses");
    let d1 = &value["domains"][0];
    assert_eq!(d1["count"], json!(4));
    assert_eq!(d1["oldest_recorded"], json!("2026-08-26T08:00:00.000Z"));
    assert_eq!(d1["newest_recorded"], json!("2026-08-27T08:00:00.000Z"));
    assert_eq!(d1["rotting"], json!(true));
    assert_eq!(d1["health"]["type_distribution"]["5"], json!(1));
    assert_eq!(d1["health"]["type_distribution"]["undefined"], json!(1));
    assert_eq!(
        d1["health"]["classification_distribution"]["undefined"],
        json!(null)
    );

    // scalar line: clean error, exit 1
    std::fs::write(ours.0.join(".mulch/expertise/d1.jsonl"), "5\n").expect("scalar writable");
    let scalar = run_in(&ours.0, Path::new(mulch_bin()), &["status"]);
    assert_eq!(scalar.code, 1);
    assert!(
        normalize_dir(&scalar.stderr, &ours.0)
            .starts_with("Error: non-object record at <DIR>/.mulch/expertise/d1.jsonl line 1"),
        "clean scalar error, got: {}",
        scalar.stderr
    );

    // malformed line: same class
    std::fs::write(
        ours.0.join(".mulch/expertise/d1.jsonl"),
        "{\"type\":\"convention\" ...\n",
    )
    .expect("malformed writable");
    let bad = run_in(&ours.0, Path::new(mulch_bin()), &["status"]);
    assert_eq!(bad.code, 1);
    assert!(
        bad.stderr.contains("Malformed JSONL at"),
        "clean malformed error, got: {}",
        bad.stderr
    );

    // unknown type: clean error
    std::fs::write(
        ours.0.join(".mulch/expertise/d1.jsonl"),
        "{\"type\":\"wat\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"content\":\"x\",\"id\":\"mx-dddddd\"}\n",
    )
    .expect("unknown writable");
    let unknown = run_in(&ours.0, Path::new(mulch_bin()), &["status"]);
    assert_eq!(unknown.code, 1);
    assert!(
        normalize_dir(&unknown.stderr, &ours.0).starts_with(
            "Error: Unknown record type \"wat\" at <DIR>/.mulch/expertise/d1.jsonl:1 \
             (id=mx-dddddd)."
        ),
        "{}",
        unknown.stderr
    );
    // --allow-unknown-types keeps status alive
    let flagged = run_in(&ours.0, Path::new(mulch_bin()), &[
        "status",
        "--allow-unknown-types",
    ]);
    assert_eq!(flagged.code, 0, "{}", flagged.stderr);
}

/// Comment lines are findings for the raw-loop readers (validate) —
/// only the strict reader skips them; a legacy singular-outcome
/// record warns there.
#[test]
fn status_commented_corpus_pins_the_validate_findings() {
    let ours = TempDir::new("stat-cmt-ours");
    let config = format!("version: '1'\ndomains:\n  d1: {{}}\n{CONFIG_TAIL}");
    write_seed_with_config(&ours.0, &config, &[]);
    let commented = "# ARCHIVED — not for active use.\n{\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"content\":\"ok\",\"id\":\"mx-aaaaaa\"}\n{\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"content\":\"legacy\",\"id\":\"mx-444444\",\"outcome\":{\"status\":\"success\"}}\n";
    std::fs::write(ours.0.join(".mulch/expertise/d1.jsonl"), commented)
        .expect("commented writable");
    let validate = run_in(&ours.0, Path::new(mulch_bin()), &["validate"]);
    assert_eq!(validate.code, 1);
    assert_eq!(validate.stdout, "3 records validated, 1 errors found\n");
    assert_eq!(
        validate.stderr,
        "d1:1 - Invalid JSON: failed to parse\nd1:3 - Legacy \"outcome\" field (singular); run \
         `mulch doctor --fix` to migrate to \"outcomes[]\"\n"
    );
    // the strict reader (status) skips comments and warns nowhere
    let status = run_in(&ours.0, Path::new(mulch_bin()), &["status"]);
    assert_eq!(status.code, 0);
    assert!(status.stdout.contains("d1: 2 records"));
}

/// Partial config blocks keep their set values and backfill the rest
/// per field (the withDefaults semantics): governance 5/150/200 and
/// shelf 1/30 reach the renderers, and the staleness computes from
/// the effective shelf.
#[test]
fn status_partial_config_blocks_backfill_per_field() {
    let ours = TempDir::new("cfg-ours");
    let now = jiff::Timestamp::now();
    let days_ago = |days: i64| (now - jiff::Span::new().hours(days * 24)).to_string();
    let config = "version: '1'\ndomains:\n  dev: {}\ngovernance:\n  max_entries: 5\nclassification_defaults:\n  shelf_life:\n    tactical: 1\n";
    let store = [
        // stale under tactical: 1, fresh in the default of 14
        format!(
            r#"{{"type":"pattern","name":"p1","description":"d","classification":"tactical","recorded_at":"{}"}}"#,
            days_ago(3)
        ),
        // fresh under the backfilled observational default of 30
        format!(
            r#"{{"type":"pattern","name":"p2","description":"d","classification":"observational","recorded_at":"{}"}}"#,
            days_ago(10)
        ),
    ]
    .join("\n")
        + "\n";
    let before = store.clone();
    std::fs::create_dir_all(ours.0.join(".mulch/expertise")).expect("expertise dir");
    std::fs::write(ours.0.join(".mulch/mulch.config.yaml"), config).expect("config");
    std::fs::write(ours.0.join(".mulch/expertise/dev.jsonl"), &store).expect("dev store");

    // json: the effective accessors render (governance 5 with the
    // backfilled warn/hard, shelf 1/30)
    let json = run_in(&ours.0, Path::new(mulch_bin()), &["status", "--json"]);
    assert_eq!(json.code, 0);
    let value: serde_json::Value = serde_json::from_str(&json.stdout).expect("json parses");
    assert_eq!(value["governance"]["max_entries"], json!(5));
    assert_eq!(value["governance"]["warn_entries"], json!(150));
    assert_eq!(value["governance"]["hard_limit"], json!(200));
    assert_eq!(value["shelf_life"]["tactical"], json!(1));
    assert_eq!(value["shelf_life"]["observational"], json!(30));
    // the 3-day tactical record is stale under shelf 1
    assert_eq!(value["domains"][0]["health"]["stale_count"], json!(1));

    // plain + doctor stay healthy exits; the store is untouched
    for args in [&["status"][..], &["doctor"][..]] {
        let run = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(run.code, 0, "{args:?}: {}", run.stderr);
    }
    assert_eq!(
        read_store_file(&ours.0, "expertise/dev.jsonl"),
        before,
        "read-only surfaces never rewrite"
    );
}

// mulch-4d25 battery: the health block parses recorded_at with the
// JS Date law (date-only stamps, the space separator, zone-less
// civil time) and its oldest/newest selection compares the RAW field
// strings; the json health block pins the raw winners verbatim.
#[test]
fn status_non_iso_recorded_at_stamps_pin_the_raw_law() {
    // The fresh zone-less stamp rides the runtime clock two days
    // back, rendered as civil time in the process zone exactly as the
    // CLI reads it back; the ancient stamps sit in stable buckets.
    let then = jiff::Timestamp::now() - jiff::Span::new().hours(48);
    let fresh_zoneless = then
        .to_zoned(jiff::tz::TimeZone::system())
        .strftime("%Y-%m-%dT%H:%M:%S")
        .to_string();
    let convention = |body: &str, at: &str, id: &str| {
        format!(
            r#"{{"type":"convention","classification":"tactical","recorded_at":"{at}","content":"{body}","id":"{id}"}}"#
        )
    };
    let config = "version: '1'\ndomains:\n  dev: {}\n  olddev: {}\n";
    let dev = [
        convention("iso control", "2020-01-02T12:00:00.000Z", "mx-900001"),
        convention("date only", "2020-01-01", "mx-900002"),
        convention("space separator", "2020-01-01 12:00:00Z", "mx-900003"),
        convention("zone less ancient", "2020-01-01T12:00:00", "mx-900004"),
        convention("zone less fresh", &fresh_zoneless, "mx-900005"),
    ]
    .join("\n")
        + "\n";
    let olddev = [
        convention("date only", "2020-01-01", "mx-900006"),
        convention(
            "space separator newest",
            "2020-01-02 12:00:00Z",
            "mx-900007",
        ),
    ]
    .join("\n")
        + "\n";
    let ours = seeded_store(
        "noniso",
        Some(config),
        &[("dev", &dev), ("olddev", &olddev)],
        &[],
    );

    // json: the health block's RAW stamp strings pin the winners —
    // the date-only string is the raw oldest, the zone-less fresh
    // civil stamp the raw newest (its ISO rendering appears in
    // oldest/newest_recorded after date math).
    let json = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "status"]);
    assert_eq!(json.code, 0);
    let value: serde_json::Value = serde_json::from_str(&json.stdout).expect("json parses");
    let dev_row = &value["domains"][0];
    assert_eq!(dev_row["health"]["oldest_timestamp"], json!("2020-01-01"));
    assert_eq!(
        dev_row["health"]["newest_timestamp"],
        serde_json::json!(fresh_zoneless)
    );
    let olddev_row = &value["domains"][1];
    assert_eq!(
        olddev_row["health"]["oldest_timestamp"],
        json!("2020-01-01")
    );
    // the space separator BEATS the T separator as newest by the raw
    // string law... in olddev it IS the newest either way
    assert_eq!(
        olddev_row["health"]["newest_timestamp"],
        "2020-01-02 12:00:00Z"
    );
    // the ancient domain rots, the fresh-stamped one does not
    assert_eq!(olddev_row["rotting"], json!(true));
    assert_eq!(dev_row["rotting"], json!(false));

    // plain: byte law on both channels minus the volatile ages
    let plain = run_in(&ours.0, Path::new(mulch_bin()), &["status"]);
    assert_eq!(plain.code, 0);
    assert!(plain.stdout.contains("  dev: 5 records"));
    assert!(
        plain
            .stdout
            .contains("  olddev: 2 records (updated just now) — recorded ")
    );
    assert!(plain.stdout.contains("⚠ ROTTING"));
}

/// The raw-comparison law's DISCRIMINATOR (spec-review nit, sprint
/// 38): a space-separated stamp BEATS a T-separated one as `oldest`
/// even though its instant is an hour LATER (the raw-string law
/// compares `"2020-01-01 "` < `"2020-01-01T"` lexicographically —
/// date math would pick the other record), and a NUMBER recorded_at
/// never displaces the fold's first winner (NaN comparisons).
#[test]
fn status_raw_compare_discriminator_and_number_stamp() {
    let convention = |body: &str, at: &str, id: &str| {
        format!(
            r#"{{"type":"convention","classification":"tactical","recorded_at":{at},"content":"{body}","id":"{id}"}}"#
        )
    };
    let config = "version: '1'\ndomains:\n  rawdev: {}\n  numdev: {}\n";
    let rawdev = [
        convention(
            "t earlier instant",
            r#""2020-01-01T04:00:00Z""#,
            "mx-900011",
        ),
        convention(
            "space later instant",
            r#""2020-01-01 05:00:00Z""#,
            "mx-900012",
        ),
    ]
    .join("\n")
        + "\n";
    // A number stamp is deliberately schema-invalid — status reads
    // leniently; NaN comparisons never displace the fold winner, so
    // the FIRST record's string holds both slots.
    let numdev = [
        convention("string first", r#""2020-01-01""#, "mx-900013"),
        convention("number never displaces", "5", "mx-900014"),
    ]
    .join("\n")
        + "\n";
    let ours = seeded_store(
        "rawcmp",
        Some(config),
        &[("rawdev", &rawdev), ("numdev", &numdev)],
        &[],
    );
    for args in [&["status"][..], &["--json", "status"][..]] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 0, "{args:?}: {}", our.stderr);
        if args[0] == "--json" {
            let value: serde_json::Value = serde_json::from_str(&our.stdout).expect("json parses");
            let raw = &value["domains"][0];
            // raw-string law: the SPACE stamp is oldest (space < T)
            // even though its instant is an hour later; the T stamp
            // is newest.
            assert_eq!(
                raw["health"]["oldest_timestamp"],
                json!("2020-01-01 05:00:00Z")
            );
            assert_eq!(
                raw["health"]["newest_timestamp"],
                json!("2020-01-01T04:00:00Z")
            );
            let num = &value["domains"][1];
            assert_eq!(num["health"]["oldest_timestamp"], json!("2020-01-01"));
            assert_eq!(num["health"]["newest_timestamp"], json!("2020-01-01"));
        } else {
            assert!(our.stdout.contains("  rawdev: 2 records"));
            assert!(our.stdout.contains("  numdev: 2 records"));
        }
    }
}
