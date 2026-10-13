//! `mulch edit` parity probes.
//!
//! Moved verbatim from the one-file battery (mulch-3e7c); sprint
//! banners stay with their probes.

use std::path::Path;

use super::support::*;

#[test]
fn edit_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("edit-ours");
    let theirs = TempDir::new("edit-theirs");
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
            "old",
        ]);
    }
    let id = pattern_p_id();
    let edit = ["edit", "d", &id, "--description", "new"];
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &edit);
    let theirs_run = run_in(&theirs.0, &ml, &edit);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);
    assert_eq!(
        normalize_line(&read_store_file(&ours.0, "expertise/d.jsonl")),
        normalize_line(&read_store_file(&theirs.0, "expertise/d.jsonl"))
    );

    // unknown id
    let missing = ["edit", "d", "mx-deadbe", "--description", "x"];
    let ours_missing = run_in(&ours.0, Path::new(mulch_bin()), &missing);
    let theirs_missing = run_in(&theirs.0, &ml, &missing);
    assert_eq!(ours_missing.code, theirs_missing.code);
    assert_eq!(ours_missing.stdout, theirs_missing.stdout);
    assert_eq!(ours_missing.stderr, theirs_missing.stderr);

    // idKey edit does not recompute the id
    let rename = ["edit", "d", &id, "--name", "p2"];
    let ours_rename = run_in(&ours.0, Path::new(mulch_bin()), &rename);
    assert_eq!(ours_rename.code, 0);
    assert!(read_store_file(&ours.0, "expertise/d.jsonl").contains(&id));
}

#[test]
fn edit_outcome_flags_and_list_replace_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("eo-ours");
    let theirs = TempDir::new("eo-theirs");
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
            "d1",
            "--files",
            "f1.rs",
        ]);
    }
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
    let theirs_run = run_in(&theirs.0, &ml, &edit);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);
    assert_eq!(
        normalize_line(&read_store_file(&ours.0, "expertise/d.jsonl")),
        normalize_line(&read_store_file(&theirs.0, "expertise/d.jsonl"))
    );

    // edit --files REPLACES the list
    let replace = ["edit", "d", &id, "--files", "g1.rs,g2.rs"];
    let ours_rep = run_in(&ours.0, Path::new(mulch_bin()), &replace);
    let theirs_rep = run_in(&theirs.0, &ml, &replace);
    assert_eq!(ours_rep.code, theirs_rep.code);
    assert_eq!(
        normalize_line(&read_store_file(&ours.0, "expertise/d.jsonl")),
        normalize_line(&read_store_file(&theirs.0, "expertise/d.jsonl"))
    );
    assert!(read_store_file(&ours.0, "expertise/d.jsonl").contains("g1.rs"));
    assert!(!read_store_file(&ours.0, "expertise/d.jsonl").contains("f1.rs"));
}

#[test]
fn edit_unknown_id_json_envelope_on_stderr() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("ejs-ours");
    let theirs = TempDir::new("ejs-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let _ = run_in(dir, &ml, &["add", "d"]);
    }
    let missing = ["edit", "d", "mx-deadbe", "--description", "x", "--json"];
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &missing);
    let theirs_run = run_in(&theirs.0, &ml, &missing);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);
    assert_eq!(ours_run.stderr, theirs_run.stderr);
}

#[test]
fn edit_outcome_identifier_resolution_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{ALPHA_ONE}\n{ALPHA_TWO}\n");

    // edit and outcome resolve bare hashes and unique prefixes; their
    // output surfaces carry the RESOLVED id, not the input.
    for (index, id) in ["1bb21d", "mx-1b", "1bb21"].iter().enumerate() {
        let (ours, theirs) = twin_seeded(
            &format!("eoed{index}"),
            None,
            &[("alpha", seed.as_str())],
            &[],
        );
        let args = ["edit", "alpha", id, "--description", "touched"];
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        let their = run_in(&theirs.0, &ml, &args);
        assert_eq!(our.code, 0, "{args:?} resolves");
        assert_eq!(our.stdout, their.stdout, "{args:?} stdout");
        assert_eq!(our.stdout, "✓ Updated pattern mx-1bb21d in alpha\n");
        assert_same_file(&ours, &theirs, "expertise/alpha.jsonl");
    }
    for (index, id) in ["1bb21d", "mx-1b"].iter().enumerate() {
        let (ours, theirs) = twin_seeded(
            &format!("eoout{index}"),
            None,
            &[("alpha", seed.as_str())],
            &[],
        );
        let args = ["outcome", "alpha", id, "--status", "success"];
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        let their = run_in(&theirs.0, &ml, &args);
        assert_eq!(our.code, 0, "{args:?} resolves");
        assert_eq!(our.stdout, their.stdout, "{args:?} stdout");
        assert_eq!(our.stdout, "✓ Outcome recorded: success on mx-1bb21d\n");
        let ours_file = read_store_file(&ours.0, "expertise/alpha.jsonl");
        let theirs_file = read_store_file(&theirs.0, "expertise/alpha.jsonl");
        assert_eq!(normalize_line(&ours_file), normalize_line(&theirs_file));
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
        let (ours, theirs) = twin_seeded(
            &format!("eoamb{index}"),
            None,
            &[("alpha", seed.as_str())],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, 1, "{args:?} aborts");
        assert_same_stderr(&ours, &our, &theirs, &their, &expected);
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
    }
    let (ours, theirs) = twin_seeded("eoambj", None, &[("alpha", seed.as_str())], &[]);
    let args = ["edit", "alpha", "mx-", "--description", "x", "--json"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 1);
    assert_same_stderr(
        &ours,
        &our,
        &theirs,
        &their,
        &error_envelope("edit", ambiguous),
    );

    // outcome LISTING (no --status) resolves prefixes too and prints
    // the resolved id in its header
    let (ours, theirs) = twin_seeded(
        "eolist",
        None,
        &[(
            "alpha",
            format!("{ALPHA_ONE}\nmx-ab1111-placeholder\n").as_str(),
        )],
        &[],
    );
    // craft a store where the first record carries an outcome array
    let with_outcome = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"name\":\"Alpha One\",\"description\":\"d\",\"id\":\"mx-1bb21d\",\"outcomes\":[{\"status\":\"success\"}]}\n";
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(
            dir.join(".mulch").join("expertise").join("alpha.jsonl"),
            with_outcome,
        )
        .expect("store writable");
    }
    let list_prefix = ["outcome", "alpha", "1bb21d"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &list_prefix);
    let their = run_in(&theirs.0, &ml, &list_prefix);
    assert_eq!(our.code, their.code, "listing resolves the prefix");
    assert_eq!(our.stdout, their.stdout, "listing stdout");
    assert_eq!(our.stdout, "Outcomes for mx-1bb21d (1):\n  1. success\n");

    // a non-object record under a prefix input: both sides report the
    // non-object clean error (reference crashes — DEVIATIONS)
    let scalar_store = "5\n";
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(
            dir.join(".mulch").join("expertise").join("alpha.jsonl"),
            scalar_store,
        )
        .expect("scalar store writable");
    }
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

    // json surfaces carry the resolved id too: edit success, outcome
    // append success, outcome listing, and the ambiguous envelope
    let (ours, theirs) = twin_seeded("eojson", None, &[("alpha", seed.as_str())], &[]);
    let edit_json = [
        "--json",
        "edit",
        "alpha",
        "1bb21d",
        "--description",
        "touched",
    ];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &edit_json);
    let their = run_in(&theirs.0, &ml, &edit_json);
    assert_eq!(our.code, their.code);
    assert_eq!(
        normalize(&our.stdout),
        normalize(&their.stdout),
        "edit json"
    );
    assert!(
        our.stdout.contains("\"id\": \"mx-1bb21d\""),
        "{}",
        our.stdout
    );

    let (ours, theirs) = twin_seeded("eojson2", None, &[("alpha", seed.as_str())], &[]);
    let out_json = [
        "--json", "outcome", "alpha", "1bb21d", "--status", "success",
    ];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &out_json);
    let their = run_in(&theirs.0, &ml, &out_json);
    assert_eq!(our.code, their.code);
    assert_eq!(
        normalize(&our.stdout),
        normalize(&their.stdout),
        "outcome json"
    );

    let listing_json = ["--json", "outcome", "alpha", "mx-1b"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &listing_json);
    let their = run_in(&theirs.0, &ml, &listing_json);
    assert_eq!(our.code, their.code);
    assert_eq!(
        normalize(&our.stdout),
        normalize(&their.stdout),
        "outcome listing json"
    );

    let (ours, theirs) = twin_seeded("eoambout", None, &[("alpha", seed.as_str())], &[]);
    let amb_out_json = ["--json", "outcome", "alpha", "mx-", "--status", "success"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &amb_out_json);
    let their = run_in(&theirs.0, &ml, &amb_out_json);
    assert_eq!(our.code, 1);
    assert_same_stderr(
        &ours,
        &our,
        &theirs,
        &their,
        &error_envelope("outcome", ambiguous),
    );

    // Unknown identifier keeps the not-found text.
    for (index, args) in [
        &["edit", "alpha", "zzz", "--description", "x"] as &[&str],
        &["outcome", "alpha", "zzz", "--status", "success"],
    ]
    .into_iter()
    .enumerate()
    {
        let (ours, theirs) = twin_seeded(
            &format!("eomiss{index}"),
            None,
            &[("alpha", seed.as_str())],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, 1);
        assert_eq!(our.stderr, their.stderr, "{args:?} stderr");
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
    }
}

#[test]
fn edit_write_gate_aborts_on_schema_invalid_updates() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("egate-ours");
    let theirs = TempDir::new("egate-theirs");
    let ours_ids = craft_gate_store(&ours.0, &ml);
    let theirs_ids = craft_gate_store(&theirs.0, &ml);

    let missing = theirs_ids["contentless body"].clone();
    let extra = theirs_ids["extra body"].clone();
    let wrong = theirs_ids["wrongtyped body"].clone();
    let legacy = theirs_ids["legacy body"].clone();
    let ok = ours_ids["ok body"].clone();

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
        // legacy singular outcome key (the reference keeps the key
        // in memory and its ajv gate reports it as additional)
        vec!["edit", "d", &legacy, "--classification", "tactical"],
        vec!["edit", "d", &legacy, "--json"],
    ] {
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
        let theirs_run = run_in(&theirs.0, &ml, &args);
        assert_eq!(ours_run.code, theirs_run.code, "{args:?}");
        assert_eq!(ours_run.stdout, theirs_run.stdout, "{args:?}");
        assert_eq!(ours_run.stderr, theirs_run.stderr, "{args:?}");
    }
    // Nothing was written: both stores byte-equal to the crafted
    // state (recorded_at ids aside, the files are deterministic).
    assert_eq!(
        normalize(&read_store_file(&ours.0, "expertise/d.jsonl")),
        normalize(&read_store_file(&theirs.0, "expertise/d.jsonl")),
    );
    // A fixing edit still writes: --content repairs the wrong-typed
    // payload and the edit succeeds.
    let fix = ["edit", "d", &wrong, "--content", "fixed body"];
    let ours_fix = run_in(&ours.0, Path::new(mulch_bin()), &fix);
    let theirs_fix = run_in(&theirs.0, &ml, &fix);
    assert_eq!(ours_fix.code, theirs_fix.code);
    assert_eq!(ours_fix.stdout, theirs_fix.stdout);
    assert_eq!(
        normalize(&read_store_file(&ours.0, "expertise/d.jsonl")),
        normalize(&read_store_file(&theirs.0, "expertise/d.jsonl")),
    );
    let _ = ok; // silence unused when the matrix above changes
}

#[test]
fn edit_files_flag_drops_for_undeclaring_types_and_orders_new_keys() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("efile-ours");
    let theirs = TempDir::new("efile-theirs");
    let ours_ids = craft_gate_store(&ours.0, &ml);
    let _ = craft_gate_store(&theirs.0, &ml);
    let ok = ours_ids["ok body"].clone();
    let missing = ours_ids["contentless body"].clone();

    // --files on a convention (which does not declare it) is DROPPED
    // by the reference's def-gated field loop — the edit succeeds and
    // no files key is written.
    let drop = ["edit", "d", &ok, "--files", "x.rs"];
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &drop);
    let theirs_run = run_in(&theirs.0, &ml, &drop);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);
    assert_eq!(
        normalize(&read_store_file(&ours.0, "expertise/d.jsonl")),
        normalize(&read_store_file(&theirs.0, "expertise/d.jsonl")),
    );

    // New keys append in the reference's application order:
    // relates_to before the outcome entry.
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
    let theirs_run = run_in(&theirs.0, &ml, &mixed);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(
        normalize(&read_store_file(&ours.0, "expertise/d.jsonl")),
        normalize(&read_store_file(&theirs.0, "expertise/d.jsonl")),
    );
}
