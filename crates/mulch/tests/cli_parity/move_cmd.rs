//! `mulch move` parity probes.
//!
//! Moved verbatim from the one-file battery (mulch-3e7c); sprint
//! banners stay with their probes.

use std::path::Path;

use super::support::*;

#[test]
fn move_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let alpha = format!("{ALPHA_ONE}\n{ALPHA_TWO}\n");
    let beta = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:58.427Z\",\"name\":\"Beta One\",\"description\":\"first beta pattern\",\"id\":\"mx-3242b5\"}\n";
    let cases: [&[&str]; 3] = [
        &["move", "alpha", "mx-1bb21d", "beta"],
        &["move", "alpha", "mx-1bb21d", "beta", "--json"],
        &["move", "alpha", "mx-1bb21d", "beta", "--dry-run"],
    ];
    for (index, args) in cases.iter().enumerate() {
        let ours = TempDir::new(format!("mv{index}-o").as_str());
        let theirs = TempDir::new(format!("mv{index}-t").as_str());
        for dir in [&ours.0, &theirs.0] {
            write_seed(dir, &[("alpha", alpha.as_str()), ("beta", beta)]);
        }
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), args);
        let theirs_run = run_in(&theirs.0, &ml, args);
        assert_eq!(ours_run.code, theirs_run.code, "{args:?} exit");
        assert_eq!(ours_run.stdout, theirs_run.stdout, "{args:?} stdout");
        assert_eq!(ours_run.stderr, theirs_run.stderr, "{args:?} stderr");
        assert_eq!(
            read_store_file(&ours.0, "expertise/beta.jsonl"),
            read_store_file(&theirs.0, "expertise/beta.jsonl"),
            "{args:?} target"
        );
        assert_eq!(
            read_store_file(&ours.0, "expertise/alpha.jsonl"),
            read_store_file(&theirs.0, "expertise/alpha.jsonl"),
            "{args:?} source"
        );
    }

    // errors: same domain, unknown target, schema-invalid record
    let invalid = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:57.690Z\",\"name\":\"Broken\",\"id\":\"mx-f16294\"}\n";
    let error_cases: [(&[&str], &str); 3] = [
        (&["move", "alpha", "mx-1bb21d", "alpha"], alpha.as_str()),
        (&["move", "alpha", "mx-1bb21d", "zeta"], alpha.as_str()),
        (&["move", "alpha", "mx-f16294", "beta"], invalid),
    ];
    for (index, (args, body)) in error_cases.iter().enumerate() {
        let ours = TempDir::new(format!("mve{index}-o").as_str());
        let theirs = TempDir::new(format!("mve{index}-t").as_str());
        for dir in [&ours.0, &theirs.0] {
            write_seed(dir, &[("alpha", body), ("beta", beta)]);
        }
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), args);
        let theirs_run = run_in(&theirs.0, &ml, args);
        assert_eq!(ours_run.code, theirs_run.code, "{args:?} exit");
        assert_eq!(ours_run.stderr, theirs_run.stderr, "{args:?} stderr");
    }
}

#[test]
fn move_incoming_references_and_allowed_types() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    // gamma refers to the moved id -> incomingReferences entry
    let gamma = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:48:00.000Z\",\"name\":\"Gamma One\",\"description\":\"refers\",\"relates_to\":[\"mx-1bb21d\"],\"id\":\"mx-da09e8\"}\n";
    let alpha = format!("{ALPHA_ONE}\n");
    let beta = format!("{ALPHA_TWO}\n");
    let ours = TempDir::new("mvr-o");
    let theirs = TempDir::new("mvr-t");
    for dir in [&ours.0, &theirs.0] {
        write_seed(dir, &[
            ("alpha", alpha.as_str()),
            ("beta", beta.as_str()),
            ("gamma", gamma),
        ]);
    }
    let args = ["move", "alpha", "mx-1bb21d", "beta", "--json"];
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let theirs_run = run_in(&theirs.0, &ml, &args);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);
    if ours_run.stdout.contains("incomingReferences") {
        assert!(ours_run.stdout.contains("mx-da09e8"), "referrer reported");
    }
}

// ---- sprint 3 (mulch-dc67) hardening: strict reads, identifier
// resolution, config order and the move gates ----

#[test]
fn move_same_domain_check_precedes_domain_lookup() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{ALPHA_ONE}\n");
    let message = "Error: Source and target domain are the same — nothing to move.\n";
    // Both domains unknown, both known, and only the source known: the
    // same-domain check always wins.
    let cases: [&[&str]; 3] = [
        &["move", "nope", "mx-1bb21d", "nope"],
        &["move", "alpha", "mx-1bb21d", "alpha"],
        &["move", "zeta", "mx-1bb21d", "zeta"],
    ];
    for (index, args) in cases.iter().enumerate() {
        let (ours, theirs) = twin_seeded(
            &format!("samedom{index}"),
            None,
            &[("alpha", seed.as_str()), ("beta", "")],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, 1, "{args:?} aborts");
        assert_eq!(our.stdout, "", "{args:?} nothing on stdout");
        assert_same_stderr(&ours, &our, &theirs, &their, message);
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
    }

    let (ours, theirs) = twin_seeded(
        "samedomjson",
        None,
        &[("alpha", seed.as_str()), ("beta", "")],
        &[],
    );
    let args = ["move", "nope", "mx-1bb21d", "nope", "--json"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 1);
    assert_same_stderr(
        &ours,
        &our,
        &theirs,
        &their,
        &error_envelope(
            "move",
            "Source and target domain are the same — nothing to move.",
        ),
    );

    // Two DIFFERENT unknown domains fall through to the domain lookup.
    let (ours, theirs) = twin_seeded(
        "unkdoms",
        None,
        &[("alpha", seed.as_str()), ("beta", "")],
        &[],
    );
    let args = ["move", "zeta", "mx-1bb21d", "nope"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 1);
    assert_same_stderr(
        &ours,
        &our,
        &theirs,
        &their,
        "Error: Domain \"zeta\" not found in config. Available domains: alpha, beta\n",
    );
}

#[test]
fn move_refuses_archived_records_like_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{ARCHIVED_ALPHA_ONE}\n");
    let message = "Error: Record mx-1bb21d is archived. Run `ml restore mx-1bb21d` first.\n";
    for (index, args) in [
        &["move", "alpha", "mx-1bb21d", "beta"] as &[&str],
        &["move", "alpha", "mx-1bb21d", "beta", "--force"],
        &["move", "alpha", "1bb21d", "beta"],
    ]
    .into_iter()
    .enumerate()
    {
        let (ours, theirs) = twin_seeded(
            &format!("arch{index}"),
            None,
            &[("alpha", seed.as_str()), ("beta", "")],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, 1, "{args:?} aborts");
        assert_eq!(our.stdout, "", "{args:?} nothing on stdout");
        assert_same_stderr(&ours, &our, &theirs, &their, message);
        // The archived record stays put, the target stays empty.
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
        assert_eq!(read_store_file(&ours.0, "expertise/beta.jsonl"), "");
    }

    let (ours, theirs) = twin_seeded(
        "archjson",
        None,
        &[("alpha", seed.as_str()), ("beta", "")],
        &[],
    );
    let args = ["move", "alpha", "mx-1bb21d", "beta", "--json"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 1);
    assert_same_stderr(
        &ours,
        &our,
        &theirs,
        &their,
        &error_envelope(
            "move",
            "Record mx-1bb21d is archived. Run `ml restore mx-1bb21d` first.",
        ),
    );
}

#[test]
fn move_required_fields_gate_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{ALPHA_ONE}\n");
    let message = "Record is missing field(s) required by target domain \"beta\": \"owner\". Edit the record (`ml edit mx-1bb21d`) before moving.";
    // `--force` bypasses allowed_types only; required_fields always holds.
    for (index, args) in [
        &["move", "alpha", "mx-1bb21d", "beta"] as &[&str],
        &["move", "alpha", "mx-1bb21d", "beta", "--force"],
        &["move", "alpha", "1bb21d", "beta"],
    ]
    .into_iter()
    .enumerate()
    {
        let (ours, theirs) = twin_seeded(
            &format!("req{index}"),
            Some(REQUIRED_FIELDS_CONFIG),
            &[("alpha", seed.as_str()), ("beta", "")],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, 1, "{args:?} aborts");
        assert_eq!(our.stdout, "", "{args:?} nothing on stdout");
        // Reference plain text carries the `Error: ` prefix (the json
        // field drops it — see the envelope case below).
        assert_same_stderr(&ours, &our, &theirs, &their, &format!("Error: {message}\n"));
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
        assert_eq!(read_store_file(&ours.0, "expertise/beta.jsonl"), "");
    }

    let (ours, theirs) = twin_seeded(
        "reqjson",
        Some(REQUIRED_FIELDS_CONFIG),
        &[("alpha", seed.as_str()), ("beta", "")],
        &[],
    );
    let args = ["move", "alpha", "mx-1bb21d", "beta", "--json"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 1);
    assert_same_stderr(
        &ours,
        &our,
        &theirs,
        &their,
        &error_envelope("move", message),
    );

    // A target that does not require the field moves normally.
    let (ours, theirs) = twin_seeded("reqok", None, &[("alpha", seed.as_str()), ("beta", "")], &[
    ]);
    let args = ["move", "alpha", "mx-1bb21d", "beta", "--json"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 0);
    assert_eq!(our.stdout, their.stdout);
    assert_same_file(&ours, &theirs, "expertise/beta.jsonl");
}

#[test]
fn move_allowed_types_gate_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{ALPHA_ONE}\n");
    let message = "Type \"pattern\" is not in target domain \"beta\" allowed_types (convention). Pass --force to override, or adjust mulch.config.yaml.";
    let (ours, theirs) = twin_seeded(
        "allowtype",
        Some(ALLOWED_TYPES_CONFIG),
        &[("alpha", seed.as_str()), ("beta", "")],
        &[],
    );
    let args = ["move", "alpha", "mx-1bb21d", "beta"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 1, "the gate refuses");
    assert_eq!(our.stdout, "", "nothing on stdout");
    assert_same_stderr(&ours, &our, &theirs, &their, &format!("Error: {message}\n"));
    assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
    assert_eq!(read_store_file(&ours.0, "expertise/beta.jsonl"), "");

    let (ours, theirs) = twin_seeded(
        "allowtypejson",
        Some(ALLOWED_TYPES_CONFIG),
        &[("alpha", seed.as_str()), ("beta", "")],
        &[],
    );
    let args = ["move", "alpha", "mx-1bb21d", "beta", "--json"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 1);
    assert_same_stderr(
        &ours,
        &our,
        &theirs,
        &their,
        &error_envelope("move", message),
    );

    // `--force` overrides the allowed_types gate.
    let (ours, theirs) = twin_seeded(
        "allowtypeforce",
        Some(ALLOWED_TYPES_CONFIG),
        &[("alpha", seed.as_str()), ("beta", "")],
        &[],
    );
    let args = ["move", "alpha", "mx-1bb21d", "beta", "--force"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 0, "--force overrides allowed_types");
    assert_eq!(our.stdout, their.stdout);
    assert_eq!(our.stderr, their.stderr);
    assert_same_file(&ours, &theirs, "expertise/beta.jsonl");
}

#[test]
fn move_incoming_references_scan_orphan_files_like_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    // The source and the target are skipped, an ORPHAN file (present on
    // disk, absent from the config) is scanned. One referrer has an id,
    // one has none.
    let ref_source = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:48:00.000Z\",\"name\":\"In Alpha\",\"description\":\"refers\",\"relates_to\":[\"mx-1bb21d\"],\"id\":\"mx-aaa111\"}";
    let ref_target = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:48:00.500Z\",\"name\":\"In Beta\",\"description\":\"refers\",\"relates_to\":[\"mx-1bb21d\"],\"id\":\"mx-bbb111\"}";
    let ref_orphan = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:48:01.000Z\",\"name\":\"In Delta\",\"description\":\"refers\",\"relates_to\":[\"mx-1bb21d\"],\"id\":\"mx-ddd222\"}";
    let ref_orphan_no_id = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:48:02.000Z\",\"name\":\"No Id\",\"description\":\"refers\",\"relates_to\":[\"mx-1bb21d\"]}";
    let domains = [
        ("alpha", format!("{ALPHA_ONE}\n{ref_source}\n")),
        ("beta", format!("{BETA_ONE_LINE}\n{ref_target}\n")),
    ];
    let orphans = [("delta.jsonl", format!("{ref_orphan}\n{ref_orphan_no_id}\n"))];
    let seeds: Vec<(&str, &str)> = domains
        .iter()
        .map(|(domain, body)| (*domain, body.as_str()))
        .collect();
    let orphan_seeds: Vec<(&str, &str)> = orphans
        .iter()
        .map(|(name, body)| (*name, body.as_str()))
        .collect();

    let expected_json = "{\n  \"success\": true,\n  \"command\": \"move\",\n  \"sourceDomain\": \"alpha\",\n  \"targetDomain\": \"beta\",\n  \"record\": {\n    \"id\": \"mx-1bb21d\",\n    \"type\": \"pattern\",\n    \"summary\": \"Alpha One\"\n  },\n  \"incomingReferences\": [\n    {\n      \"domain\": \"delta\",\n      \"id\": \"mx-ddd222\",\n      \"field\": \"relates_to\"\n    },\n    {\n      \"domain\": \"delta\",\n      \"id\": null,\n      \"field\": \"relates_to\"\n    }\n  ]\n}\n";
    let expected_plain = "✓ Moved pattern mx-1bb21d from alpha → beta: Alpha One\n  2 inbound reference(s) found; ID preserved so existing links still resolve:\n    delta/mx-ddd222 via relates_to\n    delta/(no id) via relates_to\n";
    for (index, (extra, expected)) in [(&[][..], expected_plain), (&["--json"][..], expected_json)]
        .into_iter()
        .enumerate()
    {
        let tag = format!("incref{index}");
        let (ours, theirs) = twin_seeded(&tag, None, &seeds, &orphan_seeds);
        let mut args = vec!["move", "alpha", "mx-1bb21d", "beta"];
        args.extend_from_slice(extra);
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        let their = run_in(&theirs.0, &ml, &args);
        assert_eq!(our.code, 0, "{args:?} succeeds");
        assert_eq!(
            our.stdout, expected,
            "{args:?} reports the orphan referrers"
        );
        assert_eq!(our.stdout, their.stdout, "{args:?} reference stdout");
        assert_eq!(our.stderr, their.stderr, "{args:?} stderr");
        // Referrers living in the source or target file are not listed.
        assert!(!our.stdout.contains("mx-aaa111"), "source domain skipped");
        assert!(!our.stdout.contains("mx-bbb111"), "target domain skipped");
        // The orphan file itself is never rewritten.
        assert_eq!(
            read_store_file(&ours.0, "expertise/delta.jsonl"),
            format!("{ref_orphan}\n{ref_orphan_no_id}\n")
        );
    }
}

#[test]
fn move_aborts_on_legacy_outcome_records_like_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("mleg-ours");
    let theirs = TempDir::new("mleg-theirs");
    let ours_ids = craft_gate_store(&ours.0, &ml);
    let _ = craft_gate_store(&theirs.0, &ml);
    let legacy = ours_ids["legacy body"].clone();

    let args = ["move", "d", &legacy, "d2"];
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let theirs_run = run_in(&theirs.0, &ml, &args);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stderr, theirs_run.stderr);
    assert_eq!(
        normalize(&read_store_file(&ours.0, "expertise/d.jsonl")),
        normalize(&read_store_file(&theirs.0, "expertise/d.jsonl")),
    );
    // d2 untouched on both sides.
    assert_eq!(read_store_file(&ours.0, "expertise/d2.jsonl"), "");
}

// ---------------------------------------------------------------------------
// Sprint 23 lane B proposals (mulch-4ee2 / mulch-3368-B): three
// differential pins for the payload<->flag adapter surfaces. The
// flag-table coverage itself lives as unit pins on PayloadFlags
// (commands/mod.rs payload_flags_tests) — these pin the OBSERVABLE
// differential behavior the tables feed: the plain missing-flags
// echo, record's content precedence, and edit's payload append order.
// ---------------------------------------------------------------------------
