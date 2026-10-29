//! `mulch edit` probes — OUR byte pins (ADR-0008: the differential
//! battery is dissolved; our bytes are the regression memory).
//!
//! Sprint banners stay with their probes.

use std::path::Path;

use super::support::*;

#[test]
fn edit_updates_and_rejects_pin_our_bytes() {
    let ours = TempDir::new("edit-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--type",
        "pattern",
        "--name",
        "p",
        "--description",
        "old",
    ]);
    let id = pattern_p_id();
    let edit = ["edit", "d", &id, "--description", "new"];
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &edit);
    assert_eq!(ours_run.code, 0, "{}", ours_run.stderr);
    assert_eq!(ours_run.stdout, format!("✓ Updated pattern {id} in d\n"));
    assert!(
        read_store_file(&ours.0, "expertise/d.jsonl").contains("\"description\":\"new\""),
        "the edit landed"
    );

    // unknown id
    let missing = ["edit", "d", "mx-deadbe", "--description", "x"];
    let ours_missing = run_in(&ours.0, Path::new(mulch_bin()), &missing);
    assert_eq!(ours_missing.code, 1);
    assert!(
        ours_missing.stderr.contains("mx-deadbe"),
        "the not-found wording names the id: {}",
        ours_missing.stderr
    );

    // idKey edit does not recompute the id
    let rename = ["edit", "d", &id, "--name", "p2"];
    let ours_rename = run_in(&ours.0, Path::new(mulch_bin()), &rename);
    assert_eq!(ours_rename.code, 0, "{}", ours_rename.stderr);
    assert!(read_store_file(&ours.0, "expertise/d.jsonl").contains(&id));
}

#[test]
fn edit_outcome_flags_and_list_replace_pin_our_laws() {
    let ours = TempDir::new("eo-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--type",
        "pattern",
        "--name",
        "p",
        "--description",
        "d1",
        "--files",
        "f1.rs",
    ]);
    let id = pattern_p_id();

    // edit --outcome-status uses the --outcome-* longs and appends
    let edit = [
        "edit",
        "d",
        &id,
        "--outcome-status",
        "success",
        "--outcome-agent",
        "ag",
    ];
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &edit);
    assert_eq!(ours_run.code, 0, "{}", ours_run.stderr);
    let file = read_store_file(&ours.0, "expertise/d.jsonl");
    assert!(
        file.contains("\"outcomes\":[{\"status\":\"success\",\"agent\":\"ag\"}]"),
        "outcome appended: {file}"
    );
    assert!(
        file.contains("\"files\":[\"f1.rs\"]"),
        "files untouched by the outcome edit: {file}"
    );

    // edit --files REPLACES the list
    let replace = ["edit", "d", &id, "--files", "g1.rs,g2.rs"];
    let ours_rep = run_in(&ours.0, Path::new(mulch_bin()), &replace);
    assert_eq!(ours_rep.code, 0, "{}", ours_rep.stderr);
    assert!(read_store_file(&ours.0, "expertise/d.jsonl").contains("g1.rs"));
    assert!(!read_store_file(&ours.0, "expertise/d.jsonl").contains("f1.rs"));
}

#[test]
fn edit_unknown_id_json_envelope_lands_on_stderr() {
    let ours = TempDir::new("ejs-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["add", "d"]);
    let missing = ["edit", "d", "mx-deadbe", "--description", "x", "--json"];
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &missing);
    assert_eq!(ours_run.code, 1);
    assert_eq!(ours_run.stdout, "");
    assert!(
        ours_run.stderr.contains("\"command\": \"edit\"")
            && ours_run.stderr.contains("\"success\": false"),
        "the json error envelope rides stderr: {}",
        ours_run.stderr
    );
}

// ---- sprint 9 (mulch-351d): edit/outcome identifier resolution ----

#[test]
fn edit_outcome_identifier_resolution_pins_our_bytes() {
    let seed = format!("{ALPHA_ONE}\n{ALPHA_TWO}\n");

    // edit and outcome resolve bare hashes and unique prefixes; their
    // output surfaces carry the RESOLVED id, not the input.
    for (index, id) in ["1bb21d", "mx-1b", "1bb21"].iter().enumerate() {
        let ours = seeded_store(
            &format!("eoed{index}"),
            None,
            &[("alpha", seed.as_str())],
            &[],
        );
        let args = ["edit", "alpha", id, "--description", "touched"];
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(our.code, 0, "{args:?} resolves");
        assert_eq!(our.stdout, "✓ Updated pattern mx-1bb21d in alpha\n");
        assert!(
            read_store_file(&ours.0, "expertise/alpha.jsonl")
                .contains("\"description\":\"touched\""),
            "the edit landed"
        );
    }
    for (index, id) in ["1bb21d", "mx-1b"].iter().enumerate() {
        let ours = seeded_store(
            &format!("eoout{index}"),
            None,
            &[("alpha", seed.as_str())],
            &[],
        );
        let args = ["outcome", "alpha", id, "--status", "success"];
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(our.code, 0, "{args:?} resolves");
        assert_eq!(our.stdout, "✓ Outcome recorded: success on mx-1bb21d\n");
        assert!(
            read_store_file(&ours.0, "expertise/alpha.jsonl").contains("\"outcomes\":["),
            "the outcome landed"
        );
    }

    // Ambiguous prefix: the shared text, nothing written, plain + json.
    let ambiguous = "Ambiguous identifier \"mx-\" matches 2 records: mx-1bb21d, mx-c7129f. Use more characters to disambiguate.";
    let expected = format!("Error: {ambiguous}\n");
    for (index, args) in [
        &["edit", "alpha", "mx-", "--description", "x"] as &[&str],
        &["outcome", "alpha", "mx-", "--status", "success"],
    ]
    .into_iter()
    .enumerate()
    {
        let ours = seeded_store(
            &format!("eoamb{index}"),
            None,
            &[("alpha", seed.as_str())],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 1, "{args:?} aborts");
        assert_eq!(our.stderr, expected, "{args:?} our stderr");
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
    }
    let ours = seeded_store("eoambj", None, &[("alpha", seed.as_str())], &[]);
    let args = ["edit", "alpha", "mx-", "--description", "x", "--json"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(our.code, 1);
    assert_eq!(our.stderr, error_envelope("edit", ambiguous));

    // outcome LISTING (no --status) resolves prefixes too and prints
    // the resolved id in its header
    let ours = seeded_store("eolist", None, &[("alpha", seed.as_str())], &[]);
    // craft a store where the first record carries an outcome array
    let with_outcome = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"name\":\"Alpha One\",\"description\":\"d\",\"id\":\"mx-1bb21d\",\"outcomes\":[{\"status\":\"success\"}]}\n";
    std::fs::write(
        ours.0.join(".mulch").join("expertise").join("alpha.jsonl"),
        with_outcome,
    )
    .expect("store writable");
    let list_prefix = ["outcome", "alpha", "1bb21d"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &list_prefix);
    assert_eq!(our.code, 0, "listing resolves the prefix");
    assert_eq!(our.stdout, "Outcomes for mx-1bb21d (1):\n  1. success\n");

    // a non-object record under a prefix input: the clean error
    // (the retired reference crashed here — DEVIATIONS)
    let scalar_store = "5\n";
    std::fs::write(
        ours.0.join(".mulch").join("expertise").join("alpha.jsonl"),
        scalar_store,
    )
    .expect("scalar store writable");
    let our_scalar = run_in(&ours.0, Path::new(mulch_bin()), &[
        "edit",
        "alpha",
        "1bb",
        "--description",
        "x",
    ]);
    assert_eq!(our_scalar.code, 1);
    assert!(
        our_scalar.stderr.contains("non-object record at"),
        "clean scalar error, got: {}",
        our_scalar.stderr
    );

    // json surfaces carry the resolved id too: edit success and the
    // ambiguous envelope
    let ours = seeded_store("eojson", None, &[("alpha", seed.as_str())], &[]);
    let edit_json = [
        "--json",
        "edit",
        "alpha",
        "1bb21d",
        "--description",
        "touched",
    ];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &edit_json);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert!(
        our.stdout.contains("\"id\": \"mx-1bb21d\""),
        "{}",
        our.stdout
    );

    let ours = seeded_store("eoambout", None, &[("alpha", seed.as_str())], &[]);
    let amb_out_json = ["--json", "outcome", "alpha", "mx-", "--status", "success"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &amb_out_json);
    assert_eq!(our.code, 1);
    assert_eq!(our.stderr, error_envelope("outcome", ambiguous));

    // Unknown identifier keeps the not-found text.
    for (index, args) in [
        &["edit", "alpha", "zzz", "--description", "x"] as &[&str],
        &["outcome", "alpha", "zzz", "--status", "success"],
    ]
    .into_iter()
    .enumerate()
    {
        let ours = seeded_store(
            &format!("eomiss{index}"),
            None,
            &[("alpha", seed.as_str())],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 1);
        assert!(
            our.stderr.contains("zzz"),
            "the not-found wording names the id: {}",
            our.stderr
        );
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
    }
}

#[test]
fn edit_write_gate_aborts_on_schema_invalid_updates() {
    let ours = TempDir::new("egate-ours");
    let ours_ids = craft_gate_store(&ours.0);

    let missing = ours_ids["contentless body"].clone();
    let extra = ours_ids["extra body"].clone();
    let wrong = ours_ids["wrongtyped body"].clone();
    let legacy = ours_ids["legacy body"].clone();
    let _ok = ours_ids["ok body"].clone();

    let crafted = normalize(&read_store_file(&ours.0, "expertise/d.jsonl"));
    for args in [
        // missing required payload — plain and json
        vec!["edit", "d", &missing, "--classification", "tactical"],
        vec![
            "edit",
            "d",
            &missing,
            "--classification",
            "tactical",
            "--json",
        ],
        // unknown extra field
        vec!["edit", "d", &extra, "--classification", "tactical"],
        // wrong-typed payload (no-op edit still aborts)
        vec!["edit", "d", &wrong],
        // legacy singular outcome key (kept in memory, the write gate
        // reports it as additional)
        vec!["edit", "d", &legacy, "--classification", "tactical"],
        vec!["edit", "d", &legacy, "--json"],
    ] {
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(ours_run.code, 1, "{args:?} aborts");
        // plain carries the `Error:` line, json the envelope — both
        // name the failed schema validation
        assert!(
            ours_run.stderr.contains("failed schema validation"),
            "{args:?}: {}",
            ours_run.stderr
        );
    }
    // Nothing was written.
    assert_eq!(
        normalize(&read_store_file(&ours.0, "expertise/d.jsonl")),
        crafted,
        "the crafted state survives every aborted edit"
    );
    // A fixing edit still writes: --content repairs the wrong-typed
    // payload and the edit succeeds.
    let fix = ["edit", "d", &wrong, "--content", "fixed body"];
    let ours_fix = run_in(&ours.0, Path::new(mulch_bin()), &fix);
    assert_eq!(ours_fix.code, 0, "{}", ours_fix.stderr);
    assert!(
        read_store_file(&ours.0, "expertise/d.jsonl").contains("fixed body"),
        "the fix landed"
    );
}

#[test]
fn edit_files_flag_drops_for_undeclaring_types_and_orders_new_keys() {
    let ours = TempDir::new("efile-ours");
    let ours_ids = craft_gate_store(&ours.0);
    let ok = ours_ids["ok body"].clone();
    let missing = ours_ids["contentless body"].clone();

    // --files on a convention (which does not declare it) is DROPPED
    // by the def-gated field loop — the edit succeeds and no files
    // key is written.
    let drop = ["edit", "d", &ok, "--files", "x.rs"];
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &drop);
    assert_eq!(ours_run.code, 0, "{}", ours_run.stderr);
    let file = read_store_file(&ours.0, "expertise/d.jsonl");
    assert!(!file.contains("x.rs"), "files dropped: {file}");
    assert!(!file.contains("\"files\""), "no files key written: {file}");

    // New keys append at object end in the application order: the
    // outcome entry lands BEFORE the relates_to link (the singular
    // legacy outcome migrates to outcomes[] in the same rewrite).
    let mixed = [
        "edit",
        "d",
        &ours_ids["legacy body"],
        "--relates-to",
        &missing,
        "--outcome-status",
        "partial",
    ];
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &mixed);
    assert_eq!(ours_run.code, 0, "{}", ours_run.stderr);
    let file = read_store_file(&ours.0, "expertise/d.jsonl");
    let line = file
        .lines()
        .find(|l| l.contains("relates_to"))
        .expect("edited line");
    assert!(
        line.find("\"outcomes\":").unwrap() < line.find("\"relates_to\":").unwrap(),
        "outcomes precedes relates_to: {line}"
    );
    assert!(
        line.contains("\"status\":\"partial\""),
        "the outcome entry landed: {line}"
    );
}
