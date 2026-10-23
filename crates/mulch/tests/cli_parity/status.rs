//! `mulch status` parity probes.
//!
//! Moved verbatim from the one-file battery (mulch-3e7c); sprint
//! banners stay with their probes.

use std::path::Path;

use super::support::*;

#[test]
fn status_no_store_error_matches() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("nostore-ours");
    let theirs = TempDir::new("nostore-theirs");

    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["status"]);
    let theirs_run = run_in(&theirs.0, &ml, &["status"]);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);
    assert_eq!(ours_run.stderr, theirs_run.stderr);

    let ours_json = run_in(&ours.0, Path::new(mulch_bin()), &["status", "--json"]);
    let theirs_json = run_in(&theirs.0, &ml, &["status", "--json"]);
    assert_eq!(ours_json.code, theirs_json.code);
    assert_eq!(ours_json.stdout, theirs_json.stdout);
    assert_eq!(ours_json.stderr, theirs_json.stderr);
}

#[test]
fn status_fresh_store_matches() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("fresh-ours");
    let theirs = TempDir::new("fresh-theirs");
    run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    run_in(&theirs.0, &ml, &["init"]);

    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["status"]);
    let theirs_run = run_in(&theirs.0, &ml, &["status"]);
    assert_eq!(ours_run.code, theirs_run.code);
    // ours promotes `mulch add` where the reference says `ml add`
    assert_eq!(ours_run.stdout, promote_theirs(&theirs_run.stdout));
    assert!(
        ours_run
            .stdout
            .contains("No domains configured. Run `mulch add <domain>` to get started.")
    );

    let ours_json = run_in(&ours.0, Path::new(mulch_bin()), &["status", "--json"]);
    let theirs_json = run_in(&theirs.0, &ml, &["status", "--json"]);
    assert_eq!(ours_json.code, theirs_json.code);
    assert_eq!(normalize(&ours_json.stdout), normalize(&theirs_json.stdout));
}

#[test]
fn status_with_records_matches_structurally() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("recs-ours");
    let theirs = TempDir::new("recs-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let _ = run_in(dir, &ml, &[
            "record",
            "dev",
            "--type",
            "pattern",
            "--name",
            "p1",
            "--description",
            "dp1",
        ]);
        let _ = run_in(dir, &ml, &[
            "record",
            "dev",
            "--type",
            "convention",
            "--content",
            "c1",
        ]);
    }

    let ours_json = run_in(&ours.0, Path::new(mulch_bin()), &["status", "--json"]);
    let theirs_json = run_in(&theirs.0, &ml, &["status", "--json"]);
    assert_eq!(ours_json.code, theirs_json.code);
    assert_eq!(normalize(&ours_json.stdout), normalize(&theirs_json.stdout));

    // plain lines: counts and suffix shape (relative times stay volatile)
    let ours_plain = run_in(&ours.0, Path::new(mulch_bin()), &["status"]);
    let theirs_plain = run_in(&theirs.0, &ml, &["status"]);
    assert_eq!(ours_plain.code, theirs_plain.code);
    assert_eq!(
        ours_plain.stdout.lines().count(),
        theirs_plain.stdout.lines().count()
    );
    assert!(ours_plain.stdout.contains("dev: 2 records"));
}

#[test]
fn status_reader_surfaces_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("stat-rd-ours");
    let theirs = TempDir::new("stat-rd-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let _ = run_in(dir, &ml, &["add", "d1"]);
        let _ = run_in(dir, &ml, &["add", "d2"]);
    }
    // fixed, far-apart recorded_at stamps keep the "Xd ago" buckets
    // stable for the test duration AND cross the observational shelf
    // life (rotting); the array line counts, the numeric type line
    // pins the integer-first distribution key order.
    let corpus = "{\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"2026-08-26T08:00:00.000Z\",\"content\":\"old\",\"id\":\"mx-bbbbbb\"}\n{\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"2026-08-27T08:00:00.000Z\",\"content\":\"new\",\"id\":\"mx-cccccc\"}\n[1,2,3]\n{\"type\":5,\"classification\":\"tactical\",\"recorded_at\":\"2026-08-26T08:00:00.000Z\",\"content\":\"numtype\",\"id\":\"mx-333333\"}\n";
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(
            dir.join(".mulch").join("expertise").join("d1.jsonl"),
            corpus,
        )
        .expect("corpus writable");
        // d2 keeps its empty file; a file-less domain renders "never"
        std::fs::remove_file(dir.join(".mulch").join("expertise").join("d2.jsonl"))
            .expect("d2 file removable");
    }

    // plain: count includes the array line, the recorded range renders,
    // the file-less domain says "never"
    let ours_plain = run_in(&ours.0, Path::new(mulch_bin()), &["status"]);
    let theirs_plain = run_in(&theirs.0, &ml, &["status"]);
    assert_eq!(ours_plain.code, theirs_plain.code);
    assert_eq!(ours_plain.stdout, theirs_plain.stdout);

    // json: same story (timestamps normalized)
    let ours_json = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "status"]);
    let theirs_json = run_in(&theirs.0, &ml, &["--json", "status"]);
    assert_eq!(ours_json.code, theirs_json.code);
    assert_eq!(normalize(&ours_json.stdout), normalize(&theirs_json.stdout));

    // scalar line: both exit 1; the reference stack-traces (DEVIATIONS:
    // ours is a clean error), so only the code is comparable
    let scalar = "5\n";
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(
            dir.join(".mulch").join("expertise").join("d1.jsonl"),
            scalar,
        )
        .expect("scalar writable");
    }
    let ours_scalar = run_in(&ours.0, Path::new(mulch_bin()), &["status"]);
    let theirs_scalar = run_in(&theirs.0, &ml, &["status"]);
    assert_eq!(ours_scalar.code, theirs_scalar.code);
    assert_eq!(ours_scalar.code, 1);
    assert!(
        ours_scalar.stderr.contains("non-object record at"),
        "clean scalar error, got: {}",
        ours_scalar.stderr
    );

    // malformed line: same class (reference stack-traces)
    let malformed = "{\"type\":\"convention\" ...\n";
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(
            dir.join(".mulch").join("expertise").join("d1.jsonl"),
            malformed,
        )
        .expect("malformed writable");
    }
    let ours_bad = run_in(&ours.0, Path::new(mulch_bin()), &["status"]);
    let theirs_bad = run_in(&theirs.0, &ml, &["status"]);
    assert_eq!(ours_bad.code, theirs_bad.code);
    assert_eq!(ours_bad.code, 1);
    assert!(
        ours_bad.stderr.contains("Malformed JSONL at"),
        "clean malformed error, got: {}",
        ours_bad.stderr
    );

    // unknown type: both exit 1 (reference stack-traces; ours clean)
    let unknown = "{\"type\":\"wat\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"content\":\"x\",\"id\":\"mx-dddddd\"}\n";
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(
            dir.join(".mulch").join("expertise").join("d1.jsonl"),
            unknown,
        )
        .expect("unknown writable");
    }
    let ours_unknown = run_in(&ours.0, Path::new(mulch_bin()), &["status"]);
    let theirs_unknown = run_in(&theirs.0, &ml, &["status"]);
    assert_eq!(ours_unknown.code, theirs_unknown.code);
    assert_eq!(ours_unknown.code, 1);

    // comment lines are findings for the raw-loop readers (validate,
    // doctor jsonl-integrity) — only the strict reader skips them
    // ...plus a legacy singular-outcome record: warning, not error
    let commented = "# ARCHIVED — not for active use.\n{\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"content\":\"ok\",\"id\":\"mx-aaaaaa\"}\n{\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"content\":\"legacy\",\"id\":\"mx-444444\",\"outcome\":{\"status\":\"success\"}}\n";
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(
            dir.join(".mulch").join("expertise").join("d1.jsonl"),
            commented,
        )
        .expect("commented writable");
    }
    let ours_validate = run_in(&ours.0, Path::new(mulch_bin()), &["validate"]);
    let theirs_validate = run_in(&theirs.0, &ml, &["validate"]);
    assert_eq!(ours_validate.code, theirs_validate.code);
    assert_eq!(ours_validate.stdout, theirs_validate.stdout);
    assert_eq!(ours_validate.stderr, theirs_validate.stderr);

    // --allow-unknown-types keeps status alive (reference flag too)
    let ours_flag = run_in(&ours.0, Path::new(mulch_bin()), &[
        "status",
        "--allow-unknown-types",
    ]);
    let theirs_flag = run_in(&theirs.0, &ml, &["status", "--allow-unknown-types"]);
    assert_eq!(ours_flag.code, theirs_flag.code);
    assert_eq!(ours_flag.stdout, theirs_flag.stdout);
}

/// Partial config blocks keep their set values and backfill the rest
/// per field (the reference's withDefaults semantics). The old typed
/// readers dropped partial blocks wholesale — this pins the effective
/// accessors against the reference on both surfaces that render them:
/// the status blocks and the staleness computed from them.
#[test]
fn status_partial_config_blocks_backfill_per_field_like_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("cfg-ours");
    let theirs = TempDir::new("cfg-theirs");
    let now = jiff::Timestamp::now();
    let days_ago = |days: i64| (now - jiff::Span::new().hours(days * 24)).to_string();
    let config = "version: '1'\ndomains:\n  dev: {}\ngovernance:\n  max_entries: 5\nclassification_defaults:\n  shelf_life:\n    tactical: 1\n";
    let store = [
        // stale under tactical: 1, fresh in the reference default of 14
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
    for dir in [&ours.0, &theirs.0] {
        std::fs::create_dir_all(dir.join(".mulch/expertise")).expect("expertise dir");
        std::fs::write(dir.join(".mulch/mulch.config.yaml"), config).expect("config");
        std::fs::write(dir.join(".mulch/expertise/dev.jsonl"), &store).expect("dev store");
    }

    for args in [&["status", "--json"][..], &["status"][..], &["doctor"][..]] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, their.code, "rc differs for {args:?}");
        let strip = |text: &str| -> String {
            text.lines()
                .filter(|line| {
                    !line.contains("Update available")
                        && !line.contains("Run `mulch upgrade`")
                        && !line.contains("Native binary")
                        && !line.contains("passed,")
                        && !line.contains("lastUpdated")
                        && !line.contains("oldest_recorded")
                        && !line.contains("newest_recorded")
                        && !line.contains("oldest_timestamp")
                        && !line.contains("newest_timestamp")
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        assert_eq!(strip(&our.stdout), strip(&their.stdout), "stdout {args:?}");
        assert_eq!(strip(&our.stderr), strip(&their.stderr), "stderr {args:?}");
    }
    assert_same_file(&ours, &theirs, "expertise/dev.jsonl");
}

// mulch-15d3 battery: the legacy singular `outcome` field with odd
// values (absorbs mulch-1609) — listing, append, and doctor --fix
// parity across number/string/null/array/empty-object shapes.
