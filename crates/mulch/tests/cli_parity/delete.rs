//! `mulch delete`/`delete-domain` parity probes (plus the shared
//! delete-and-move surfaces).
//!
//! Moved verbatim from the one-file battery (mulch-3e7c); sprint
//! banners stay with their probes.

use std::path::Path;

use super::support::*;

#[test]
fn delete_single_bulk_and_errors_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{ALPHA_ONE}\n{ALPHA_TWO}\n");
    let cases: [&[&str]; 6] = [
        &["delete", "alpha", "mx-1bb21d"],
        &["delete", "alpha", "mx-c7129f"],
        &["delete", "alpha", "--records", "mx-1bb21d,mx-c7129f"],
        &["delete", "alpha", "--all-except", "mx-1bb21d"],
        &["delete", "alpha", "--dry-run"],
        &["delete", "alpha", "--records", "mx-1bb21d", "--json"],
    ];
    for (index, args) in cases.iter().enumerate() {
        let ours = TempDir::new(format!("del{index}-o").as_str());
        let theirs = TempDir::new(format!("del{index}-t").as_str());
        for dir in [&ours.0, &theirs.0] {
            write_seed(dir, &[("alpha", seed.as_str()), ("beta", "")]);
        }
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), args);
        let theirs_run = run_in(&theirs.0, &ml, args);
        assert_eq!(ours_run.code, theirs_run.code, "{args:?} exit");
        assert_eq!(ours_run.stdout, theirs_run.stdout, "{args:?} stdout");
        assert_eq!(ours_run.stderr, theirs_run.stderr, "{args:?} stderr");
        assert_eq!(
            read_store_file(&ours.0, "expertise/alpha.jsonl"),
            read_store_file(&theirs.0, "expertise/alpha.jsonl"),
            "{args:?} file"
        );
    }

    // Error surfaces
    let errors: [&[&str]; 5] = [
        &["delete", "alpha"],
        &["delete", "alpha", "mx-1bb21d", "--records", "mx-c7129f"],
        &["delete", "alpha", "--records", ""],
        &["delete", "alpha", "--all-except", ""],
        &["delete", "alpha", "mx-ffffff"],
    ];
    for (index, args) in errors.iter().enumerate() {
        let ours = TempDir::new(format!("dele{index}-o").as_str());
        let theirs = TempDir::new(format!("dele{index}-t").as_str());
        for dir in [&ours.0, &theirs.0] {
            write_seed(dir, &[("alpha", seed.as_str())]);
        }
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), args);
        let theirs_run = run_in(&theirs.0, &ml, args);
        assert_eq!(ours_run.code, theirs_run.code, "{args:?} exit");
        assert_eq!(ours_run.stderr, theirs_run.stderr, "{args:?} stderr");
        assert_eq!(
            read_store_file(&ours.0, "expertise/alpha.jsonl"),
            seed,
            "{args:?} must not touch the store"
        );
    }
}

#[test]
fn delete_junk_rewrite_and_last_record_match() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let junk = format!(
        "# comment\n{{\"id\":\"mx-aaaaaa\",\"type\":\"pattern\",\"name\":\"Weird Order\",\"zzz_custom\":\"keepme\",\"description\":\"weird\",\"classification\":\"tactical\",\"recorded_at\":\"2026-01-01T00:00:00.000Z\"}}\n\n{ALPHA_ONE}\n"
    );
    let ours = TempDir::new("djunk-o");
    let theirs = TempDir::new("djunk-t");
    for dir in [&ours.0, &theirs.0] {
        write_seed(dir, &[("alpha", junk.as_str())]);
    }
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "delete",
        "alpha",
        "mx-1bb21d",
    ]);
    let theirs_run = run_in(&theirs.0, &ml, &["delete", "alpha", "mx-1bb21d"]);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);
    // the junk is dropped, the survivor stays byte-identical
    assert_eq!(
        read_store_file(&ours.0, "expertise/alpha.jsonl"),
        read_store_file(&theirs.0, "expertise/alpha.jsonl")
    );
    assert!(!read_store_file(&ours.0, "expertise/alpha.jsonl").contains("comment"));

    // deleting the last record leaves a 0-byte file (never removes it)
    let ours_last = TempDir::new("dlast-o");
    let theirs_last = TempDir::new("dlast-t");
    for dir in [&ours_last.0, &theirs_last.0] {
        write_seed(dir, &[("beta", &format!("{ALPHA_ONE}\n"))]);
    }
    let _ = run_in(&ours_last.0, Path::new(mulch_bin()), &[
        "delete",
        "beta",
        "mx-1bb21d",
    ]);
    let _ = run_in(&theirs_last.0, &ml, &["delete", "beta", "mx-1bb21d"]);
    assert_eq!(read_store_file(&ours_last.0, "expertise/beta.jsonl"), "");
    assert_eq!(read_store_file(&theirs_last.0, "expertise/beta.jsonl"), "");
    assert!(ours_last.0.join(".mulch/expertise/beta.jsonl").is_file());
}

#[test]
fn delete_domain_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{ALPHA_ONE}\n{ALPHA_TWO}\n");
    // --yes, --json (prompt skipped) and --dry-run
    let cases: [&[&str]; 3] = [
        &["delete-domain", "alpha", "--yes"],
        &["delete-domain", "alpha", "--json"],
        &["delete-domain", "alpha", "--yes", "--dry-run"],
    ];
    for (index, args) in cases.iter().enumerate() {
        let ours = TempDir::new(format!("dd{index}-o").as_str());
        let theirs = TempDir::new(format!("dd{index}-t").as_str());
        for dir in [&ours.0, &theirs.0] {
            write_seed(dir, &[("alpha", seed.as_str()), ("beta", "")]);
        }
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), args);
        let theirs_run = run_in(&theirs.0, &ml, args);
        assert_eq!(ours_run.code, theirs_run.code, "{args:?} exit");
        assert_eq!(ours_run.stdout, theirs_run.stdout, "{args:?} stdout");
        assert_eq!(ours_run.stderr, theirs_run.stderr, "{args:?} stderr");
        assert_eq!(
            read_store_file(&ours.0, "mulch.config.yaml"),
            read_store_file(&theirs.0, "mulch.config.yaml"),
            "{args:?} config"
        );
        assert_eq!(
            ours.0.join(".mulch/expertise/alpha.jsonl").exists(),
            theirs.0.join(".mulch/expertise/alpha.jsonl").exists(),
            "{args:?} file presence"
        );
    }

    // unknown domain: plain carries the add-hint, json the domain list
    for json in [false, true] {
        let ours = TempDir::new(format!("ddu-{}", if json { "j" } else { "p" }).as_str());
        let theirs = TempDir::new(format!("ddut-{}", if json { "j" } else { "p" }).as_str());
        for dir in [&ours.0, &theirs.0] {
            write_seed(dir, &[("beta", "")]);
        }
        let mut args = vec!["delete-domain", "nope", "--yes"];
        if json {
            args.push("--json");
        }
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
        let theirs_run = run_in(&theirs.0, &ml, &args);
        assert_eq!(ours_run.code, theirs_run.code);
        assert_eq!(ours_run.stdout, theirs_run.stdout);
        assert_eq!(ours_run.stderr, theirs_run.stderr);
    }
}

#[test]
fn delete_and_move_abort_on_malformed_jsonl_like_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{ALPHA_ONE}\n{MALFORMED_LINE}\n{ALPHA_TWO}\n");
    let beta = format!("{BETA_ONE_LINE}\n");
    let commands: [&[&str]; 4] = [
        &["delete", "alpha", "mx-1bb21d"],
        &["delete", "alpha", "--records", "mx-1bb21d"],
        &["delete-domain", "alpha", "--yes"],
        &["move", "alpha", "mx-1bb21d", "beta"],
    ];
    for (index, args) in commands.iter().enumerate() {
        let (ours, theirs) = twin_seeded(
            &format!("mal{index}"),
            None,
            &[("alpha", seed.as_str()), ("beta", beta.as_str())],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, 1, "{args:?} aborts");
        assert_eq!(their.code, 1, "{args:?} reference aborts");
        assert_eq!(our.stdout, "", "{args:?} prints nothing to stdout");
        assert_eq!(our.stdout, their.stdout, "{args:?} stdout");
        // The template is pinned; the parser's own reason wording
        // (serde vs JSON.parse) inside it is not (see the reply/dossier).
        let prefix = "Error: Malformed JSONL at <DIR>/.mulch/expertise/alpha.jsonl:2: ";
        let suffix = format!(". Line: {MALFORMED_LINE}\n");
        let our_stderr = normalize_dir(&our.stderr, &ours.0);
        let their_stderr = normalize_dir(&their.stderr, &theirs.0);
        for text in [&our_stderr, &their_stderr] {
            assert!(text.starts_with(prefix), "template prefix: {text}");
            assert!(text.ends_with(&suffix), "template suffix: {text}");
        }
        // Nothing was written.
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
        assert_eq!(read_store_file(&theirs.0, "expertise/alpha.jsonl"), seed);
        assert_same_file(&ours, &theirs, "expertise/beta.jsonl");
        assert_same_file(&ours, &theirs, "mulch.config.yaml");
    }

    // The preview is the trimmed line, cut at 77 chars plus "...".
    let long_line = format!("{{\"type\":\"convention\",\"name\":\"{}\"", "x".repeat(120));
    let preview = format!("{}...", &long_line[..77]);
    let (ours, theirs) = twin_seeded(
        "mallong",
        None,
        &[("alpha", format!("{long_line}\n").as_str())],
        &[],
    );
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "delete",
        "alpha",
        "mx-1bb21d",
    ]);
    let their = run_in(&theirs.0, &ml, &["delete", "alpha", "mx-1bb21d"]);
    assert_eq!(our.code, 1);
    assert_eq!(their.code, 1);
    let suffix = format!(". Line: {preview}\n");
    assert!(normalize_dir(&our.stderr, &ours.0).ends_with(&suffix));
    assert!(normalize_dir(&their.stderr, &theirs.0).ends_with(&suffix));

    // `move` reads only the SOURCE strictly: a malformed line in the
    // target file neither blocks the move nor changes the append.
    let target = format!("{BETA_ONE_LINE}\n{MALFORMED_LINE}\n");
    for (index, extra) in [&[] as &[&str], &["--json"]].into_iter().enumerate() {
        let mut args = vec!["move", "alpha", "mx-1bb21d", "beta"];
        args.extend_from_slice(extra);
        let (ours, theirs) = twin_seeded(
            &format!("maltgt{index}"),
            None,
            &[
                ("alpha", format!("{ALPHA_ONE}\n").as_str()),
                ("beta", target.as_str()),
            ],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        let their = run_in(&theirs.0, &ml, &args);
        assert_eq!(our.code, their.code, "{args:?} exit");
        assert_eq!(our.stdout, their.stdout, "{args:?} stdout");
        assert_eq!(our.stderr, their.stderr, "{args:?} stderr");
        assert_same_file(&ours, &theirs, "expertise/alpha.jsonl");
        assert_same_file(&ours, &theirs, "expertise/beta.jsonl");
        assert!(read_store_file(&ours.0, "expertise/beta.jsonl").contains(MALFORMED_LINE));
    }
}

#[test]
fn delete_all_except_unknown_id_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{ALPHA_ONE}\n{ALPHA_TWO}\n");
    let expected = "Error: Record \"mx-ffffff\" not found. Run `mulch query` to see record IDs.\n";
    let cases: [(&[&str], &str); 2] = [
        (&["delete", "alpha", "--all-except", "mx-ffffff"], expected),
        (
            &["delete", "alpha", "--all-except", "mx-ffffff", "--json"],
            "{\n  \"success\": false,\n  \"command\": \"delete\",\n  \"error\": \"Record \\\"mx-ffffff\\\" not found. Run `mulch query` to see record IDs.\"\n}\n",
        ),
    ];
    for (index, (args, expected)) in cases.into_iter().enumerate() {
        let (ours, theirs) = twin_seeded(
            &format!("except{index}"),
            None,
            &[("alpha", seed.as_str())],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, 1, "{args:?} aborts");
        assert_eq!(our.stdout, "", "{args:?} nothing on stdout");
        assert_same_stderr(&ours, &our, &theirs, &their, expected);
        // The keep-list is resolved before anything is written.
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
        assert_eq!(read_store_file(&theirs.0, "expertise/alpha.jsonl"), seed);
    }
}

#[test]
fn identifier_resolution_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{ALPHA_ONE}\n{ALPHA_TWO}\n");
    let survivor = format!("{ALPHA_TWO}\n");
    // Exact id, bare hash and a unique prefix all resolve; delete and
    // move use the same rule.
    let accepted: [&[&str]; 4] = [
        &["delete", "alpha", "mx-1bb21d"],
        &["delete", "alpha", "1bb21d"],
        &["delete", "alpha", "mx-1b"],
        &["delete", "alpha", "1bb21"],
    ];
    for (index, args) in accepted.iter().enumerate() {
        let (ours, theirs) = twin_seeded(
            &format!("res{index}"),
            None,
            &[("alpha", seed.as_str())],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, 0, "{args:?} resolves");
        assert_eq!(
            our.stdout, "✓ Deleted pattern mx-1bb21d from alpha: Alpha One\n",
            "{args:?} stdout"
        );
        assert_eq!(our.stdout, their.stdout, "{args:?} reference stdout");
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), survivor);
        assert_same_file(&ours, &theirs, "expertise/alpha.jsonl");
    }

    let moved: [&str; 3] = ["mx-1bb21d", "1bb21d", "mx-1b"];
    for (index, id) in moved.iter().enumerate() {
        let (ours, theirs) = twin_seeded(
            &format!("resmv{index}"),
            None,
            &[
                ("alpha", seed.as_str()),
                ("beta", format!("{BETA_ONE_LINE}\n").as_str()),
            ],
            &[],
        );
        let args = ["move", "alpha", id, "beta"];
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        let their = run_in(&theirs.0, &ml, &args);
        assert_eq!(our.code, 0, "{args:?} resolves");
        assert_eq!(our.stdout, their.stdout, "{args:?} stdout");
        assert_eq!(
            our.stdout,
            "✓ Moved pattern mx-1bb21d from alpha → beta: Alpha One\n"
        );
        assert_same_file(&ours, &theirs, "expertise/beta.jsonl");
    }

    // Two live records share the `mx-` prefix: ambiguous, nothing is
    // written, in both plain and json mode.
    let ambiguous = "Ambiguous identifier \"mx-\" matches 2 records: mx-1bb21d, mx-c7129f. Use more characters to disambiguate.";
    let expected = format!("Error: {ambiguous}\n");
    for (index, args) in [&["delete", "alpha", "mx-"] as &[&str], &[
        "move", "alpha", "mx-", "beta",
    ]]
    .into_iter()
    .enumerate()
    {
        let (ours, theirs) = twin_seeded(
            &format!("amb{index}"),
            None,
            &[
                ("alpha", seed.as_str()),
                ("beta", format!("{BETA_ONE_LINE}\n").as_str()),
            ],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, 1, "{args:?} aborts");
        assert_eq!(our.stdout, "", "{args:?} nothing on stdout");
        assert_same_stderr(&ours, &our, &theirs, &their, &expected);
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
        assert_eq!(read_store_file(&theirs.0, "expertise/alpha.jsonl"), seed);
    }

    let (ours, theirs) = twin_seeded(
        "ambjson",
        None,
        &[
            ("alpha", seed.as_str()),
            ("beta", format!("{BETA_ONE_LINE}\n").as_str()),
        ],
        &[],
    );
    let args = ["move", "alpha", "mx-", "beta", "--json"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 1);
    assert_same_stderr(
        &ours,
        &our,
        &theirs,
        &their,
        &error_envelope("move", ambiguous),
    );
}

// ---- sprint 10 (mulch-5f8a): required common fields + batch enrichment ----

#[test]
fn delete_domain_preserves_remaining_domain_order() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    // Removing the FIRST of three domains must not shuffle the rest.
    let (ours, theirs) = twin_seeded(
        "ddorder",
        None,
        &[
            ("alpha", format!("{ALPHA_ONE}\n").as_str()),
            ("beta", format!("{BETA_ONE_LINE}\n").as_str()),
            ("gamma", ""),
        ],
        &[],
    );
    let args = ["delete-domain", "alpha", "--yes"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 0, "delete-domain succeeds");
    assert_eq!(our.stdout, their.stdout);
    assert_eq!(our.stderr, their.stderr);
    assert_same_file(&ours, &theirs, "mulch.config.yaml");
    let config = read_store_file(&ours.0, "mulch.config.yaml");
    let expected = format!("version: '1'\ndomains:\n  beta: {{}}\n  gamma: {{}}\n{CONFIG_TAIL}");
    assert_eq!(config, expected, "remaining domains keep their order");
    assert!(!config.contains("alpha"), "removed domain is gone");
    assert!(!ours.0.join(".mulch/expertise/alpha.jsonl").exists());
    assert!(!theirs.0.join(".mulch/expertise/alpha.jsonl").exists());

    // Removing the MIDDLE domain keeps the outer order too.
    let (ours, theirs) = twin_seeded(
        "ddorder2",
        None,
        &[
            ("alpha", ""),
            ("beta", format!("{BETA_ONE_LINE}\n").as_str()),
            ("gamma", ""),
        ],
        &[],
    );
    let args = ["delete-domain", "beta", "--json"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 0);
    assert_eq!(our.stdout, their.stdout);
    assert_same_file(&ours, &theirs, "mulch.config.yaml");
    let config = read_store_file(&ours.0, "mulch.config.yaml");
    let expected = format!("version: '1'\ndomains:\n  alpha: {{}}\n  gamma: {{}}\n{CONFIG_TAIL}");
    assert_eq!(config, expected);

    // A registered domain whose file is already gone still deletes.
    let (ours, theirs) = twin_seeded(
        "ddgone",
        None,
        &[
            ("alpha", ""),
            ("beta", format!("{BETA_ONE_LINE}\n").as_str()),
        ],
        &[],
    );
    for dir in [&ours.0, &theirs.0] {
        std::fs::remove_file(dir.join(".mulch/expertise/alpha.jsonl")).expect("seed file");
    }
    let args = ["delete-domain", "alpha", "--yes"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 0, "missing file is tolerated");
    assert_eq!(our.stdout, their.stdout);
    assert_eq!(our.stderr, their.stderr);
    assert_same_file(&ours, &theirs, "mulch.config.yaml");
}

#[test]
fn delete_domain_backfills_missing_config_blocks() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    // The minimal config (version + domains only) gains the full
    // governance and shelf-life blocks on rewrite.
    let minimal = "version: '1'\ndomains:\n  alpha: {}\n  beta: {}\n";
    let (ours, theirs) = twin_seeded(
        "ddmin",
        Some(minimal),
        &[("alpha", format!("{ALPHA_ONE}\n").as_str()), ("beta", "")],
        &[],
    );
    let args = ["delete-domain", "alpha", "--yes"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 0);
    assert_eq!(our.stdout, their.stdout);
    assert_same_file(&ours, &theirs, "mulch.config.yaml");
    let config = read_store_file(&ours.0, "mulch.config.yaml");
    let expected = format!("version: '1'\ndomains:\n  beta: {{}}\n{CONFIG_TAIL}");
    assert_eq!(config, expected, "governance and shelf life are backfilled");
    for needle in [
        "max_entries: 100",
        "warn_entries: 150",
        "hard_limit: 200",
        "classification_defaults:",
        "shelf_life:",
        "tactical: 14",
        "observational: 30",
    ] {
        assert!(config.contains(needle), "missing {needle} in {config}");
    }

    // Same minimal config, middle domain removed.
    let (ours, theirs) = twin_seeded(
        "ddmin2",
        Some("version: '1'\ndomains:\n  alpha: {}\n  beta: {}\n  gamma: {}\n"),
        &[("alpha", ""), ("beta", ""), ("gamma", "")],
        &[],
    );
    let args = ["delete-domain", "beta", "--yes"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 0);
    assert_eq!(our.stdout, their.stdout);
    assert_same_file(&ours, &theirs, "mulch.config.yaml");
    assert_eq!(
        read_store_file(&ours.0, "mulch.config.yaml"),
        format!("version: '1'\ndomains:\n  alpha: {{}}\n  gamma: {{}}\n{CONFIG_TAIL}")
    );
}

#[test]
fn delete_domain_dry_run_json_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{ALPHA_ONE}\n{ALPHA_TWO}\n");
    let expected = "{\n  \"success\": true,\n  \"command\": \"delete-domain\",\n  \"domain\": \"alpha\",\n  \"dryRun\": true,\n  \"recordCount\": 2\n}\n";
    let cases: [&[&str]; 2] = [&["delete-domain", "alpha", "--dry-run", "--json"], &[
        "delete-domain",
        "alpha",
        "--dry-run",
        "--json",
        "--yes",
    ]];
    for (index, args) in cases.iter().enumerate() {
        let (ours, theirs) = twin_seeded(
            &format!("dddry{index}"),
            None,
            &[("alpha", seed.as_str()), ("beta", "")],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, 0, "{args:?} succeeds");
        assert_eq!(our.stdout, expected, "{args:?} is the json envelope");
        assert_eq!(our.stdout, their.stdout, "{args:?} reference stdout");
        assert_eq!(our.stderr, their.stderr, "{args:?} stderr");
        // Dry run: file and config stay untouched.
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
        assert_same_file(&ours, &theirs, "mulch.config.yaml");
        assert!(ours.0.join(".mulch/expertise/alpha.jsonl").is_file());
    }
}

#[test]
fn delete_mode_and_unknown_domain_errors_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{ALPHA_ONE}\n");
    let cases: [(&[&str], String); 8] = [
        (
            &["delete", "alpha"],
            "Error: must provide a record ID, --records, or --all-except.\n".to_string(),
        ),
        (
            &["delete", "alpha", "--json"],
            error_envelope("delete", "Must provide a record ID, --records, or --all-except."),
        ),
        (
            &["delete", "alpha", "--records", ""],
            "Error: --records requires at least one ID.\n".to_string(),
        ),
        (
            &["delete", "alpha", "--records", "", "--json"],
            error_envelope("delete", "--records requires at least one ID."),
        ),
        (
            &["delete", "alpha", "--all-except", ""],
            "Error: --all-except requires at least one ID to keep.\n".to_string(),
        ),
        (
            &["delete", "alpha", "mx-1bb21d", "--records", "mx-c7129f"],
            "Error: cannot combine a record ID with --records or --all-except. Use only one mode.\n"
                .to_string(),
        ),
        (
            &["delete", "nope", "mx-1bb21d"],
            "Error: domain \"nope\" not found in config.\nAvailable domains: alpha, beta\n"
                .to_string(),
        ),
        (
            &["delete", "nope", "mx-1bb21d", "--json"],
            error_envelope(
                "delete",
                "Domain \"nope\" not found in config. Available domains: alpha, beta",
            ),
        ),
    ];
    for (index, (args, expected)) in cases.into_iter().enumerate() {
        let (ours, theirs) = twin_seeded(
            &format!("mode{index}"),
            None,
            &[("alpha", seed.as_str()), ("beta", "")],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, 1, "{args:?} aborts");
        assert_eq!(our.stdout, "", "{args:?} nothing on stdout");
        assert_same_stderr(&ours, &our, &theirs, &their, &expected);
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
    }

    // `delete-domain` plain carries the add-hint, json the domain list.
    let domain_cases: [(&[&str], String); 2] = [
        (
            &["delete-domain", "nope", "--yes"],
            "Error: domain \"nope\" not found in config.\nHint: Run `mulch add nope` to create it, or check `mulch status` for existing domains.\n"
                .to_string(),
        ),
        (
            &["delete-domain", "nope", "--yes", "--json"],
            error_envelope(
                "delete-domain",
                "Domain \"nope\" not found in config. Available domains: alpha, beta",
            ),
        ),
    ];
    for (index, (args, expected)) in domain_cases.into_iter().enumerate() {
        let (ours, theirs) = twin_seeded(
            &format!("movedom{index}"),
            None,
            &[("alpha", seed.as_str()), ("beta", "")],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, 1, "{args:?} aborts");
        assert_same_stderr(&ours, &our, &theirs, &their, &expected);
    }

    // No domains at all renders `(none)`; the list joins with ", ".
    let (ours, theirs) = twin_seeded("modenone", None, &[], &[]);
    let args = ["delete", "nope", "mx-1bb21d"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 1);
    assert_same_stderr(
        &ours,
        &our,
        &theirs,
        &their,
        "Error: domain \"nope\" not found in config.\nAvailable domains: (none)\n",
    );

    let (ours, theirs) = twin_seeded(
        "modemulti",
        None,
        &[("alpha", ""), ("beta", ""), ("gamma", "")],
        &[],
    );
    let args = ["delete", "nope", "mx-1bb21d", "--json"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 1);
    assert_same_stderr(
        &ours,
        &our,
        &theirs,
        &their,
        &error_envelope(
            "delete",
            "Domain \"nope\" not found in config. Available domains: alpha, beta, gamma",
        ),
    );
}

#[test]
fn delete_all_except_keeping_all_prints_nothing_like_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{ALPHA_ONE}\n");
    let expected = "{\n  \"success\": true,\n  \"command\": \"delete\",\n  \"domain\": \"alpha\",\n  \"dryRun\": false,\n  \"deleted\": [],\n  \"kept\": 1\n}\n";
    let cases: [(&[&str], &str); 2] = [
        (&["delete", "alpha", "--all-except", "mx-1bb21d"], ""),
        (
            &["delete", "alpha", "--all-except", "mx-1bb21d", "--json"],
            expected,
        ),
    ];
    for (index, (args, expected)) in cases.into_iter().enumerate() {
        let (ours, theirs) = twin_seeded(
            &format!("keepall{index}"),
            None,
            &[("alpha", seed.as_str())],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, 0, "{args:?} succeeds");
        // Zero deletions: plain mode prints NOTHING (not an empty line).
        assert_eq!(our.stdout, expected, "{args:?} stdout");
        assert_eq!(our.stdout, their.stdout, "{args:?} reference stdout");
        assert_eq!(our.stderr, "", "{args:?} no stderr");
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
        assert_same_file(&ours, &theirs, "expertise/alpha.jsonl");
    }
}

#[test]
fn delete_and_move_truncate_convention_summary_like_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{}\n", long_convention_line());
    let (ours, theirs) = twin_seeded("trunc", None, &[("alpha", seed.as_str())], &[]);
    let args = ["delete", "alpha", "mx-1bb21d"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 0);
    assert_eq!(
        our.stdout,
        format!(
            "✓ Deleted convention mx-1bb21d from alpha: {}\n",
            truncated_summary()
        )
    );
    assert_eq!(our.stdout, their.stdout, "reference truncates the same way");

    let (ours, theirs) = twin_seeded(
        "truncmv",
        None,
        &[("alpha", seed.as_str()), ("beta", "")],
        &[],
    );
    let args = ["move", "alpha", "mx-1bb21d", "beta"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 0);
    assert_eq!(
        our.stdout,
        format!(
            "✓ Moved convention mx-1bb21d from alpha → beta: {}\n",
            truncated_summary()
        )
    );
    assert_eq!(our.stdout, their.stdout);
}

#[test]
fn delete_reserializes_survivors_like_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    // A survivor written with spaces must come back compact, and a
    // survivor without an id gains a generated one.
    let spaced = "{ \"type\": \"pattern\" ,  \"classification\":\"tactical\", \"recorded_at\":\"2026-10-03T20:47:57.690Z\", \"name\":\"Spaced\", \"description\":\"d\", \"id\":\"mx-1bb21d\" }";
    let id_less = "{\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:58.000Z\",\"content\":\"no id here\"}";
    let seed = format!("{spaced}\n{ALPHA_TWO}\n{id_less}\n");
    let (ours, theirs) = twin_seeded("reser", None, &[("alpha", seed.as_str())], &[]);
    let args = ["delete", "alpha", "mx-c7129f"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 0);
    assert_eq!(our.stdout, their.stdout);
    assert_same_file(&ours, &theirs, "expertise/alpha.jsonl");
    let file = read_store_file(&ours.0, "expertise/alpha.jsonl");
    let lines: Vec<&str> = file.lines().collect();
    assert_eq!(lines.len(), 2, "two survivors, one record per line");
    assert!(
        lines[0].starts_with("{\"type\":\"pattern\""),
        "compacted survivor: {}",
        lines[0]
    );
    assert!(
        !file.contains("\"type\": \""),
        "spaces are stripped: {file}"
    );
    assert!(
        lines[1].starts_with("{\"type\":\"convention\""),
        "compacted survivor: {}",
        lines[1]
    );
    assert!(
        lines[1].ends_with("\"id\":\"mx-5670d0\"}"),
        "generated id for the id-less survivor: {}",
        lines[1]
    );
    assert!(file.ends_with('\n'), "the file keeps its trailing newline");

    // `move` compacts the target append and the source survivors alike.
    let (ours, theirs) = twin_seeded(
        "resermv",
        None,
        &[
            ("alpha", format!("{spaced}\n{id_less}\n").as_str()),
            ("beta", format!("{BETA_ONE_LINE}\n").as_str()),
        ],
        &[],
    );
    let args = ["move", "alpha", "mx-1bb21d", "beta"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 0);
    assert_eq!(our.stdout, their.stdout);
    assert_same_file(&ours, &theirs, "expertise/alpha.jsonl");
    assert_same_file(&ours, &theirs, "expertise/beta.jsonl");
    let target = read_store_file(&ours.0, "expertise/beta.jsonl");
    assert_eq!(
        target.lines().count(),
        2,
        "beta keeps its line and gains one"
    );
    assert!(target.ends_with('\n'));
    let compact_moved = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:57.690Z\",\"name\":\"Spaced\",\"description\":\"d\",\"id\":\"mx-1bb21d\"}";
    assert_eq!(
        target.lines().nth(1),
        Some(compact_moved),
        "the moved record is appended compact"
    );
    let source = read_store_file(&ours.0, "expertise/alpha.jsonl");
    assert_eq!(source.lines().count(), 1);
    assert!(source.contains("\"id\":\"mx-5670d0\"}"), "id generated");
}

#[test]
fn delete_duplicate_records_delete_once_like_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{ALPHA_ONE}\n{ALPHA_TWO}\n");
    let survivor = format!("{ALPHA_TWO}\n");
    let (ours, theirs) = twin_seeded("dup", None, &[("alpha", seed.as_str())], &[]);
    let args = ["delete", "alpha", "--records", "mx-1bb21d,mx-1bb21d"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 0);
    // One deletion line, no bulk tally.
    assert_eq!(
        our.stdout, "Deleted pattern mx-1bb21d from alpha: Alpha One\n",
        "a repeated id deletes once"
    );
    assert_eq!(our.stdout, their.stdout);
    assert!(
        !our.stdout.contains("✓ Deleted 2 records"),
        "no phantom second deletion: {}",
        our.stdout
    );
    assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), survivor);
    assert_same_file(&ours, &theirs, "expertise/alpha.jsonl");

    let (ours, theirs) = twin_seeded("dupjson", None, &[("alpha", seed.as_str())], &[]);
    let args = [
        "delete",
        "alpha",
        "--records",
        "mx-1bb21d,mx-1bb21d",
        "--json",
    ];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 0);
    assert_eq!(
        our.stdout,
        "{\n  \"success\": true,\n  \"command\": \"delete\",\n  \"domain\": \"alpha\",\n  \"dryRun\": false,\n  \"deleted\": [\n    {\n      \"id\": \"mx-1bb21d\",\n      \"type\": \"pattern\",\n      \"summary\": \"Alpha One\"\n    }\n  ],\n  \"kept\": 1\n}\n"
    );
    assert_eq!(our.stdout, their.stdout);
    assert_same_file(&ours, &theirs, "expertise/alpha.jsonl");
}

/// `delete <unknown-domain>` with no id reports the unknown domain,
/// not the missing mode (reference delete.ts order; mulch-e645).
#[test]
fn delete_unknown_domain_precedes_mode_validation() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("e645-ours");
    let theirs = TempDir::new("e645-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let _ = run_in(dir, &ml, &["add", "d"]);
    }
    for args in [
        vec!["delete", "x"],
        vec!["delete", "x", "--json"],
        // mode-shaped invocations take the same domain-first order
        vec!["delete", "x", "--records", "mx-aaaa01"],
        vec!["delete", "x", "--all-except", "mx-aaaa01"],
    ] {
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
        let theirs_run = run_in(&theirs.0, &ml, &args);
        assert_eq!(ours_run.code, theirs_run.code, "{args:?}");
        assert_eq!(ours_run.stdout, theirs_run.stdout, "{args:?}");
        assert_eq!(ours_run.stderr, theirs_run.stderr, "{args:?}");
    }
}

/// The reverse pin: a KNOWN domain with no id still fails the mode
/// validation (both orders agree there — the seam must not swallow
/// the mode error).
#[test]
fn delete_known_domain_without_id_still_fails_mode_validation() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("e645r-ours");
    let theirs = TempDir::new("e645r-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let _ = run_in(dir, &ml, &["add", "d"]);
    }
    for args in [vec!["delete", "d"], vec!["delete", "d", "--json"]] {
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
        let theirs_run = run_in(&theirs.0, &ml, &args);
        assert_eq!(ours_run.code, theirs_run.code, "{args:?}");
        assert_eq!(ours_run.stderr, theirs_run.stderr, "{args:?}");
    }
}

// Sprint 22, mulch-7896 (lane iterate-s22-c2).
// Pins: edit validates the UPDATED record against the schema before
// writing — missing required payload, unknown extra fields,
// wrong-typed payloads, and the legacy singular `outcome` key all
// abort (exit 1, reference bytes, store untouched), in plain and
// json; a fixing edit still writes; --files is dropped for types
// that do not declare it; new keys append in the reference's
// application order; move aborts on legacy-outcome records.
