//! `mulch outcome` + duration + legacy-outcome pins (ADR-0008
//! edition: OUR byte pins only; literals frozen from the parity
//! era's twin runs).

use std::path::Path;

use super::support::*;

#[test]
fn outcome_duration_rejects() {
    let ours = TempDir::new("dur-bad-ours");
    duration_fixture(&ours.0);
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
        assert_eq!(ours_run.code, 1, "exit differs: {args:?}");
        assert!(
            ours_run.stderr.contains("must be a non-negative number"),
            "{args:?}: {}",
            ours_run.stderr
        );
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
    assert_eq!(ours_json.code, 1);
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
fn outcome_duration_decimals_canonicalize() {
    let ours = TempDir::new("dur-ok-ours");
    duration_fixture(&ours.0);
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
        // above 2^53: the JS Number law rounds the literal before
        // storing
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
        assert_eq!(
            ours_run.code, 0,
            "exit differs: {args:?}: {}",
            ours_run.stderr
        );
    }
    let store = normalize(&read_store_file(&ours.0, "expertise/dev.jsonl"));
    assert!(store.contains("\"duration\":42"));
    assert!(store.contains("\"duration\":7"));
    assert!(store.contains("\"duration\":0.5"));
}

// ---- sprint 17 (mulch-8c9c) ----

#[test]
fn outcome_success_and_unknown_id_pin_their_bytes() {
    let ours = TempDir::new("outc-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--type",
        "pattern",
        "--name",
        "p",
        "--description",
        "d",
    ]);
    let id = pattern_p_id();
    // with agent: the attribution parens; second outcome appends
    // without them
    let first = run_in(&ours.0, Path::new(mulch_bin()), &[
        "outcome",
        "d",
        &id,
        "--status",
        "success",
        "--agent",
        "probe-agent",
    ]);
    assert_eq!(first.code, 0);
    assert_eq!(
        first.stdout,
        format!("✓ Outcome recorded: success (probe-agent) on {id}\n")
    );
    let second = run_in(&ours.0, Path::new(mulch_bin()), &[
        "outcome", "d", &id, "--status", "partial",
    ]);
    assert_eq!(second.code, 0);
    assert_eq!(
        second.stdout,
        format!("✓ Outcome recorded: partial on {id}\n")
    );
    // two entries on the record
    let store = normalize_line(&read_store_file(&ours.0, "expertise/d.jsonl"));
    assert!(
        store.contains(&format!("\"id\":\"{id}\",\"outcomes\":["))
            && store.contains("\"status\":\"success\"")
            && store.contains("\"agent\":\"probe-agent\""),
        "{store}"
    );

    // unknown id: the query hint, plain and json
    let missing = run_in(&ours.0, Path::new(mulch_bin()), &[
        "outcome",
        "d",
        "mx-deadbe",
        "--status",
        "success",
    ]);
    assert_eq!(missing.code, 1);
    assert_eq!(
        missing.stderr,
        "Error: Record \"mx-deadbe\" not found. Run `mulch query` to see record IDs.\n"
    );
    let missing_json = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json",
        "outcome",
        "d",
        "mx-deadbe",
        "--status",
        "success",
    ]);
    assert_eq!(missing_json.code, 1);
    assert_eq!(
        missing_json.stderr,
        error_envelope(
            "outcome",
            "Record \"mx-deadbe\" not found. Run `mulch query` to see record IDs."
        )
    );
}

// ---- sprint 5 (mulch-b88b): outcome without --status is read-only ----

#[test]
fn outcome_read_only_pins_its_surfaces() {
    let ours = TempDir::new("outc-ro-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--type",
        "pattern",
        "--name",
        "p",
        "--description",
        "d",
    ]);
    let id = pattern_p_id();

    // empty listing: the notice, store untouched
    let ours_before = read_store_file(&ours.0, "expertise/d.jsonl");
    let listing = ["outcome", "d", &id];
    let empty = run_in(&ours.0, Path::new(mulch_bin()), &listing);
    assert_eq!(empty.code, 0);
    assert_eq!(empty.stdout, "No outcomes recorded for this record.\n");
    assert_eq!(
        read_store_file(&ours.0, "expertise/d.jsonl"),
        ours_before,
        "read-only listing must not write"
    );

    // json envelope of the empty listing: outcomes stays []
    let json_empty = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json", "outcome", "d", &id,
    ]);
    assert_eq!(json_empty.code, 0);
    assert!(json_empty.stdout.contains("\"outcomes\": []"));

    // two outcomes with different field shapes, then the populated
    // listing (every detail line renders, timestamps normalized)
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
        let add = run_in(&ours.0, Path::new(mulch_bin()), &outcome);
        assert_eq!(add.code, 0);
    }
    let ours_before = read_store_file(&ours.0, "expertise/d.jsonl");
    let list = run_in(&ours.0, Path::new(mulch_bin()), &listing);
    assert_eq!(list.code, 0);
    let listing_text = normalize(&list.stdout);
    assert!(listing_text.starts_with(&format!("Outcomes for {id} (2):\n")));
    assert!(listing_text.contains("  1. success (probe-agent)"));
    assert!(listing_text.contains("     recorded: <TS>"));
    assert!(listing_text.contains("  2. failure"));
    assert_eq!(
        read_store_file(&ours.0, "expertise/d.jsonl"),
        ours_before,
        "populated listing must not write"
    );

    // json listing carries the raw outcomes array (key order
    // preserved: status first, recorded_at second)
    let json = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json", "outcome", "d", &id,
    ]);
    assert_eq!(json.code, 0);
    let json_text = normalize(&json.stdout);
    assert!(json_text.contains("\"outcomes\": [\n    {\n      \"status\": \"success\""));
    assert!(json_text.contains("\"duration\": 42"));
    assert!(json_text.contains("\"notes\": \"went fine\""));
    assert!(json_text.contains("\"test_results\": \"3 passed\""));
    assert!(json_text.contains("\"status\": \"failure\""));

    // quiet suppresses the plain listing entirely
    let quiet = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--quiet", "outcome", "d", &id,
    ]);
    assert_eq!(quiet.code, 0);
    assert_eq!(quiet.stdout, "");

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
    std::fs::write(
        ours.0.join(".mulch").join("expertise").join("d.jsonl"),
        format!("{legacy}\n"),
    )
    .expect("crafted store writable");
    let legacy_run = run_in(&ours.0, Path::new(mulch_bin()), &listing);
    assert_eq!(legacy_run.code, 0);
    assert_eq!(
        legacy_run.stdout,
        format!("Outcomes for {id} (1):\n  1. success (legacy)\n")
    );
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
    std::fs::write(
        ours.0.join(".mulch").join("expertise").join("d.jsonl"),
        format!("{corrupt}\n"),
    )
    .expect("crafted store writable");
    let corrupt_run = run_in(&ours.0, Path::new(mulch_bin()), &listing);
    assert_eq!(corrupt_run.code, 1);
    assert_eq!(
        corrupt_run.stderr,
        "Error: outcomes.entries is not a function. (In 'outcomes.entries()', \
         'outcomes.entries' is undefined)\n"
    );
    assert_eq!(
        read_store_file(&ours.0, "expertise/d.jsonl"),
        format!("{corrupt}\n"),
        "corrupted-store listing must not write"
    );
    // json mode renders the raw outcomes value verbatim and succeeds
    let corrupt_json = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json", "outcome", "d", &id,
    ]);
    assert_eq!(corrupt_json.code, 0);
    assert!(
        corrupt_json
            .stdout
            .contains("\"outcomes\": \"not-an-array\"")
    );
    assert_eq!(
        read_store_file(&ours.0, "expertise/d.jsonl"),
        format!("{corrupt}\n"),
        "corrupted-store json listing must not write"
    );

    // unknown id stays an error in read-only mode
    let missing = run_in(&ours.0, Path::new(mulch_bin()), &[
        "outcome",
        "d",
        "mx-deadbe",
    ]);
    assert_eq!(missing.code, 1);
    assert!(missing.stderr.contains("not found"));
}

// ---- sprint 8 (mulch-00aa): status reads strictly, lenient seam ----

#[test]
fn legacy_outcome_odd_values_listing_pins_its_bytes() {
    let ours = TempDir::new("odd1609-list-ours");
    odd_legacy_fixture(&ours.0);

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
        assert_eq!(ours_run.code, 0, "exit code differs for {id}");
        assert_eq!(ours_run.stdout, expected, "listing drifted for {id}");
    }
    // json listing: the coerced entry serializes as [{}]
    let json = ["--json", "outcome", "probe", "mx-111111"];
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &json);
    assert_eq!(ours_run.code, 0);
    assert!(ours_run.stdout.contains("\"outcomes\": [\n    {}\n  ]"));
    // read-only: the crafted bytes survive untouched
    let before = read_store_file(&ours.0, "expertise/probe.jsonl");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &[
        "outcome",
        "probe",
        "mx-111111",
    ]);
    assert_eq!(
        read_store_file(&ours.0, "expertise/probe.jsonl"),
        before,
        "listing must not write"
    );
}

#[test]
fn legacy_outcome_odd_values_append_pins_the_rewrite_law() {
    let ours = TempDir::new("odd1609-app-ours");
    odd_legacy_fixture(&ours.0);

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
        assert_eq!(ours_run.code, 0, "exit code differs for {id}");
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
