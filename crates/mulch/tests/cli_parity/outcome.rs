//! `mulch outcome` + duration + legacy-outcome parity probes.
//!
//! Moved verbatim from the one-file battery (mulch-3e7c); sprint
//! banners stay with their probes.

use std::path::Path;

use super::support::*;

#[test]
fn outcome_duration_rejects_like_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("dur-bad-ours");
    let theirs = TempDir::new("dur-bad-theirs");
    duration_fixture(&ours.0, &ml);
    duration_fixture(&theirs.0, &ml);
    let cases: &[&[&str]] = &[
        // record family
        &[
            "record",
            "dev",
            "--type",
            "pattern",
            "--name",
            "p2",
            "--description",
            "d2",
            "--outcome-status",
            "success",
            "--outcome-duration",
            "abc",
        ],
        &[
            "record",
            "dev",
            "--type",
            "pattern",
            "--name",
            "p3",
            "--description",
            "d3",
            "--outcome-status",
            "success",
            "--outcome-duration",
            "-1",
        ],
        // edit family
        &[
            "edit",
            "dev",
            "mx-31be4b",
            "--outcome-status",
            "failure",
            "--outcome-duration",
            "1e3",
        ],
        // outcome family
        &[
            "outcome",
            "dev",
            "mx-31be4b",
            "--status",
            "partial",
            "--duration",
            "-5",
        ],
        &[
            "outcome",
            "dev",
            "mx-31be4b",
            "--status",
            "success",
            "--duration",
            "5.",
        ],
    ];
    for args in cases {
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), args);
        let theirs_run = run_in(&theirs.0, &ml, args);
        assert_eq!(ours_run.code, theirs_run.code, "exit differs: {args:?}");
        assert_eq!(
            ours_run.stdout, theirs_run.stdout,
            "stdout differs: {args:?}"
        );
        assert_eq!(
            ours_run.stderr, theirs_run.stderr,
            "stderr differs: {args:?}"
        );
        assert_eq!(ours_run.code, 1);
        assert!(ours_run.stderr.contains("must be a non-negative number"));
    }
    // json mode renders the error envelope on stderr (probe-pinned)
    let ours_json = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "dev",
        "--type",
        "pattern",
        "--name",
        "p4",
        "--description",
        "d4",
        "--outcome-status",
        "success",
        "--outcome-duration",
        "abc",
        "--json",
    ]);
    let theirs_json = run_in(&theirs.0, &ml, &[
        "record",
        "dev",
        "--type",
        "pattern",
        "--name",
        "p4",
        "--description",
        "d4",
        "--outcome-status",
        "success",
        "--outcome-duration",
        "abc",
        "--json",
    ]);
    assert_eq!(ours_json.code, theirs_json.code);
    assert_eq!(ours_json.stdout, theirs_json.stdout);
    assert_eq!(ours_json.stderr, theirs_json.stderr);
    assert!(ours_json.stderr.contains("--outcome-duration"));
    // rejected runs never write their record
    assert_eq!(
        read_store_file(&ours.0, "expertise/dev.jsonl")
            .lines()
            .count(),
        1
    );
}

#[test]
fn outcome_duration_decimals_canonicalize_like_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("dur-ok-ours");
    let theirs = TempDir::new("dur-ok-theirs");
    duration_fixture(&ours.0, &ml);
    duration_fixture(&theirs.0, &ml);
    let cases: &[&[&str]] = &[
        // "42.0" stores as the integer 42; decimals stay floats
        &[
            "record",
            "dev",
            "--type",
            "pattern",
            "--name",
            "p3",
            "--description",
            "d3",
            "--outcome-status",
            "success",
            "--outcome-duration",
            "42.0",
            "--outcome-agent",
            "ra",
            "--outcome-test-results",
            "rt",
        ],
        &[
            "record",
            "dev",
            "--type",
            "pattern",
            "--name",
            "p8",
            "--description",
            "d8",
            "--outcome-status",
            "success",
            "--outcome-duration",
            "007",
        ],
        &[
            "edit",
            "dev",
            "mx-31be4b",
            "--outcome-status",
            "failure",
            "--outcome-duration",
            "0.5",
        ],
        &[
            "outcome",
            "dev",
            "mx-31be4b",
            "--status",
            "success",
            "--duration",
            "7.25",
            "--agent",
            "oa",
            "--notes",
            "on",
            "--test-results",
            "ot",
        ],
        // above 2^53: JS Number rounds the literal before storing
        &[
            "outcome",
            "dev",
            "mx-31be4b",
            "--status",
            "partial",
            "--duration",
            "9007199254740993",
        ],
    ];
    for args in cases {
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), args);
        let theirs_run = run_in(&theirs.0, &ml, args);
        assert_eq!(ours_run.code, theirs_run.code, "exit differs: {args:?}");
        assert_eq!(
            ours_run.stdout, theirs_run.stdout,
            "stdout differs: {args:?}"
        );
        assert_eq!(
            ours_run.stderr, theirs_run.stderr,
            "stderr differs: {args:?}"
        );
    }
    assert_eq!(
        normalize(&read_store_file(&ours.0, "expertise/dev.jsonl")),
        normalize(&read_store_file(&theirs.0, "expertise/dev.jsonl"))
    );
    let store = normalize(&read_store_file(&ours.0, "expertise/dev.jsonl"));
    assert!(store.contains("\"duration\":42"));
    assert!(store.contains("\"duration\":7"));
    assert!(store.contains("\"duration\":0.5"));
}

// ---- sprint 17 (mulch-8c9c): mixed-batch partial writes + the C2
// upsert-plan seam + nested outcome/evidence subschema errors ----

#[test]
fn outcome_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("outc-ours");
    let theirs = TempDir::new("outc-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let _ = run_in(dir, &ml, &[
            "record",
            "d",
            "--type",
            "pattern",
            "--name",
            "p",
            "--description",
            "d",
        ]);
    }
    let id = pattern_p_id();
    let outcome = [
        "outcome",
        "d",
        &id,
        "--status",
        "success",
        "--agent",
        "probe-agent",
    ];
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &outcome);
    let theirs_run = run_in(&theirs.0, &ml, &outcome);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);
    assert_eq!(
        normalize_line(&read_store_file(&ours.0, "expertise/d.jsonl")),
        normalize_line(&read_store_file(&theirs.0, "expertise/d.jsonl"))
    );

    // second outcome appends; no-agent form omits the parens
    let second = ["outcome", "d", &id, "--status", "partial"];
    let ours_second = run_in(&ours.0, Path::new(mulch_bin()), &second);
    let theirs_second = run_in(&theirs.0, &ml, &second);
    assert_eq!(ours_second.code, theirs_second.code);
    assert_eq!(ours_second.stdout, theirs_second.stdout);
    assert_eq!(
        normalize_line(&read_store_file(&ours.0, "expertise/d.jsonl")),
        normalize_line(&read_store_file(&theirs.0, "expertise/d.jsonl"))
    );

    // unknown id
    let missing = ["outcome", "d", "mx-deadbe", "--status", "success"];
    let ours_missing = run_in(&ours.0, Path::new(mulch_bin()), &missing);
    let theirs_missing = run_in(&theirs.0, &ml, &missing);
    assert_eq!(ours_missing.code, theirs_missing.code);
    assert_eq!(ours_missing.stderr, theirs_missing.stderr);
}

// ---- sprint 5 (mulch-b88b): outcome without --status is read-only ----

#[test]
fn outcome_read_only_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("outc-ro-ours");
    let theirs = TempDir::new("outc-ro-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let _ = run_in(dir, &ml, &[
            "record",
            "d",
            "--type",
            "pattern",
            "--name",
            "p",
            "--description",
            "d",
        ]);
    }
    let id = pattern_p_id();

    // empty listing: same notice on both sides, store untouched
    let ours_before = read_store_file(&ours.0, "expertise/d.jsonl");
    let theirs_before = read_store_file(&theirs.0, "expertise/d.jsonl");
    let listing = ["outcome", "d", &id];
    let ours_empty = run_in(&ours.0, Path::new(mulch_bin()), &listing);
    let theirs_empty = run_in(&theirs.0, &ml, &listing);
    assert_eq!(ours_empty.code, theirs_empty.code);
    assert_eq!(ours_empty.stdout, theirs_empty.stdout);
    assert_eq!(ours_empty.stderr, theirs_empty.stderr);
    assert_eq!(
        read_store_file(&ours.0, "expertise/d.jsonl"),
        ours_before,
        "read-only listing must not write"
    );
    assert_eq!(
        read_store_file(&theirs.0, "expertise/d.jsonl"),
        theirs_before
    );

    // json envelope of the empty listing: outcomes stays []
    let json_empty = ["--json", "outcome", "d", &id];
    let ours_json_empty = run_in(&ours.0, Path::new(mulch_bin()), &json_empty);
    let theirs_json_empty = run_in(&theirs.0, &ml, &json_empty);
    assert_eq!(ours_json_empty.code, theirs_json_empty.code);
    assert_eq!(ours_json_empty.stdout, theirs_json_empty.stdout);

    // two outcomes with different field shapes, then the populated listing
    for outcome in [
        vec![
            "outcome",
            "d",
            &id,
            "--status",
            "success",
            "--agent",
            "probe-agent",
            "--duration",
            "42",
            "--notes",
            "went fine",
            "--test-results",
            "3 passed",
        ],
        vec![
            "outcome", "d", &id, "--status", "failure", "--notes", "second",
        ],
    ] {
        let ours_add = run_in(&ours.0, Path::new(mulch_bin()), &outcome);
        let theirs_add = run_in(&theirs.0, &ml, &outcome);
        assert_eq!(ours_add.code, theirs_add.code);
        assert_eq!(ours_add.stdout, theirs_add.stdout);
    }
    let ours_before = read_store_file(&ours.0, "expertise/d.jsonl");
    let ours_list = run_in(&ours.0, Path::new(mulch_bin()), &listing);
    let theirs_list = run_in(&theirs.0, &ml, &listing);
    assert_eq!(ours_list.code, theirs_list.code);
    assert_eq!(
        normalize(&ours_list.stdout),
        normalize(&theirs_list.stdout),
        "populated listing must render every detail line"
    );
    assert_eq!(
        read_store_file(&ours.0, "expertise/d.jsonl"),
        ours_before,
        "populated listing must not write"
    );

    // json listing carries the raw outcomes array (key order preserved)
    let json = ["--json", "outcome", "d", &id];
    let ours_json = run_in(&ours.0, Path::new(mulch_bin()), &json);
    let theirs_json = run_in(&theirs.0, &ml, &json);
    assert_eq!(ours_json.code, theirs_json.code);
    assert_eq!(normalize(&ours_json.stdout), normalize(&theirs_json.stdout));

    // quiet suppresses the plain listing entirely
    let quiet = ["--quiet", "outcome", "d", &id];
    let ours_quiet = run_in(&ours.0, Path::new(mulch_bin()), &quiet);
    let theirs_quiet = run_in(&theirs.0, &ml, &quiet);
    assert_eq!(ours_quiet.code, theirs_quiet.code);
    assert_eq!(ours_quiet.stdout, theirs_quiet.stdout);
    assert_eq!(ours_quiet.stdout, "");

    // crafted stores pin the odd read-only surfaces: a legacy singular
    // `outcome` object (reader normalization) and a hand-corrupted
    // non-array `outcomes` value (header + engine error, raw json)
    let legacy = serde_json::json!({
        "type": "pattern",
        "classification": "tactical",
        "recorded_at": "2026-10-04T08:00:00.000Z",
        "name": "p",
        "description": "d",
        "id": id.clone(),
        "outcome": {"status": "success", "agent": "legacy"}
    });
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(
            dir.join(".mulch").join("expertise").join("d.jsonl"),
            format!("{legacy}\n"),
        )
        .expect("crafted store writable");
    }
    let ours_legacy = run_in(&ours.0, Path::new(mulch_bin()), &listing);
    let theirs_legacy = run_in(&theirs.0, &ml, &listing);
    assert_eq!(ours_legacy.code, theirs_legacy.code);
    assert_eq!(ours_legacy.stdout, theirs_legacy.stdout);
    assert_eq!(
        read_store_file(&ours.0, "expertise/d.jsonl"),
        format!("{legacy}\n"),
        "legacy listing must not write"
    );

    let corrupt = serde_json::json!({
        "type": "pattern",
        "classification": "tactical",
        "recorded_at": "2026-10-04T08:00:00.000Z",
        "name": "p",
        "description": "d",
        "id": id.clone(),
        "outcomes": "not-an-array"
    });
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(
            dir.join(".mulch").join("expertise").join("d.jsonl"),
            format!("{corrupt}\n"),
        )
        .expect("crafted store writable");
    }
    let ours_corrupt = run_in(&ours.0, Path::new(mulch_bin()), &listing);
    let theirs_corrupt = run_in(&theirs.0, &ml, &listing);
    assert_eq!(ours_corrupt.code, theirs_corrupt.code);
    assert_eq!(ours_corrupt.stdout, theirs_corrupt.stdout);
    assert_eq!(ours_corrupt.stderr, theirs_corrupt.stderr);
    assert_eq!(
        read_store_file(&ours.0, "expertise/d.jsonl"),
        format!("{corrupt}\n"),
        "corrupted-store listing must not write"
    );
    let corrupt_json = ["--json", "outcome", "d", &id];
    let ours_corrupt_json = run_in(&ours.0, Path::new(mulch_bin()), &corrupt_json);
    let theirs_corrupt_json = run_in(&theirs.0, &ml, &corrupt_json);
    assert_eq!(ours_corrupt_json.code, theirs_corrupt_json.code);
    assert_eq!(ours_corrupt_json.stdout, theirs_corrupt_json.stdout);
    assert_eq!(
        read_store_file(&ours.0, "expertise/d.jsonl"),
        format!("{corrupt}\n"),
        "corrupted-store json listing must not write"
    );

    // unknown id stays an error in read-only mode
    let missing = ["outcome", "d", "mx-deadbe"];
    let ours_missing = run_in(&ours.0, Path::new(mulch_bin()), &missing);
    let theirs_missing = run_in(&theirs.0, &ml, &missing);
    assert_eq!(ours_missing.code, theirs_missing.code);
    assert_eq!(ours_missing.stderr, theirs_missing.stderr);
}

// ---- sprint 8 (mulch-00aa): status reads strictly, lenient seam ----

#[test]
fn legacy_outcome_odd_values_listing_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("odd1609-list-ours");
    let theirs = TempDir::new("odd1609-list-theirs");
    odd_legacy_fixture(&ours.0, &ml);
    odd_legacy_fixture(&theirs.0, &ml);

    for (id, expected) in [
        // non-object values coerce to a one-element entry whose status
        // renders as `undefined` (mulch-1609)
        ("mx-111111", "Outcomes for mx-111111 (1):\n  1. undefined\n"),
        ("mx-222222", "Outcomes for mx-222222 (1):\n  1. undefined\n"),
        ("mx-444444", "Outcomes for mx-444444 (1):\n  1. undefined\n"),
        ("mx-555555", "Outcomes for mx-555555 (1):\n  1. undefined\n"),
        // a null outcome never normalizes: no outcomes array at all
        ("mx-333333", "No outcomes recorded for this record.\n"),
        // the proper-object control keeps its extracted entry
        (
            "mx-666666",
            "Outcomes for mx-666666 (1):\n  1. success (a)\n",
        ),
    ] {
        let listing = ["outcome", "probe", id];
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &listing);
        let theirs_run = run_in(&theirs.0, &ml, &listing);
        assert_eq!(ours_run.code, theirs_run.code, "exit codes differ for {id}");
        assert_eq!(
            ours_run.stdout, theirs_run.stdout,
            "stdout differs for {id}"
        );
        assert_eq!(ours_run.stdout, expected, "listing drifted for {id}");
        assert_eq!(
            ours_run.stderr, theirs_run.stderr,
            "stderr differs for {id}"
        );
    }
    // json listing: the coerced entry serializes as [{}]
    let json = ["--json", "outcome", "probe", "mx-111111"];
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &json);
    let theirs_run = run_in(&theirs.0, &ml, &json);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);
    assert!(ours_run.stdout.contains("\"outcomes\": [\n    {}\n  ]"));
    // read-only: the crafted bytes survive untouched
    assert_eq!(
        read_store_file(&ours.0, "expertise/probe.jsonl"),
        read_store_file(&theirs.0, "expertise/probe.jsonl"),
        "listing must not write"
    );
}

#[test]
fn legacy_outcome_odd_values_append_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("odd1609-app-ours");
    let theirs = TempDir::new("odd1609-app-theirs");
    odd_legacy_fixture(&ours.0, &ml);
    odd_legacy_fixture(&theirs.0, &ml);

    // one append per odd shape: number, null, array, mid-object
    for id in ["mx-111111", "mx-333333", "mx-444444", "mx-777777"] {
        let append = [
            "outcome",
            "probe",
            id,
            "--status",
            "failure",
            "--agent",
            "ag",
            "--duration",
            "7",
        ];
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &append);
        let theirs_run = run_in(&theirs.0, &ml, &append);
        assert_eq!(ours_run.code, theirs_run.code, "exit codes differ for {id}");
        assert_eq!(
            ours_run.stdout, theirs_run.stdout,
            "stdout differs for {id}"
        );
        assert_eq!(
            ours_run.stderr, theirs_run.stderr,
            "stderr differs for {id}"
        );
        assert_eq!(
            mask_timestamps(&read_store_file(&ours.0, "expertise/probe.jsonl")),
            mask_timestamps(&read_store_file(&theirs.0, "expertise/probe.jsonl")),
            "store bytes differ after appending to {id}"
        );
    }
    // the number record keeps its coerced empty entry first...
    let store = read_store_file(&ours.0, "expertise/probe.jsonl");
    let num_line = store.lines().next().expect("num record");
    assert!(
        num_line.contains("\"outcomes\":[{},"),
        "coerced entry must precede the appended one: {num_line}"
    );
    // ...the null record keeps its null key AND gains outcomes...
    let nul_line = store.lines().nth(2).expect("nul record");
    assert!(
        nul_line.contains("\"outcome\":null,\"outcomes\":"),
        "null outcome key must survive the rewrite: {nul_line}"
    );
    // ...and the mid-object record gets outcomes appended at the END
    // (directly after id, the last original key)
    let mid_line = store.lines().nth(6).expect("mid record");
    assert!(
        mid_line.contains("\"id\":\"mx-777777\",\"outcomes\":["),
        "migrated outcomes must land at line end: {mid_line}"
    );
}
