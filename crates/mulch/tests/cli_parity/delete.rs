//! `mulch delete`/`delete-domain` probes (plus the shared
//! delete-and-move surfaces) — OUR byte pins (ADR-0008: the
//! differential battery is dissolved; our bytes are the regression
//! memory).
//!
//! Sprint banners stay with their probes.

use std::path::Path;

use super::support::*;

#[test]
fn delete_error_paths_leave_the_store_untouched() {
    let seed = format!("{ALPHA_ONE}\n{ALPHA_TWO}\n");
    let errors: [&[&str]; 5] = [
        &["delete", "alpha"],
        &["delete", "alpha", "mx-1bb21d", "--records", "mx-c7129f"],
        &["delete", "alpha", "--records", ""],
        &["delete", "alpha", "--all-except", ""],
        &["delete", "alpha", "mx-ffffff"],
    ];
    for (index, args) in errors.iter().enumerate() {
        let ours = seeded_store(
            &format!("dele{index}"),
            None,
            &[("alpha", seed.as_str())],
            &[],
        );
        let run = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(run.code, 1, "{args:?} exit");
        assert!(
            run.stdout.is_empty(),
            "{args:?} nothing on stdout: {}",
            run.stdout
        );
        assert_eq!(
            read_store_file(&ours.0, "expertise/alpha.jsonl"),
            seed,
            "{args:?} must not touch the store"
        );
    }
}

#[test]
fn delete_junk_rewrite_and_last_record_pin_our_bytes() {
    let junk = format!(
        "# comment\n{{\"id\":\"mx-aaaaaa\",\"type\":\"pattern\",\"name\":\"Weird Order\",\"zzz_custom\":\"keepme\",\"description\":\"weird\",\"classification\":\"tactical\",\"recorded_at\":\"2026-01-01T00:00:00.000Z\"}}\n\n{ALPHA_ONE}\n"
    );
    let ours = seeded_store("djunk", None, &[("alpha", junk.as_str())], &[]);
    let run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "delete",
        "alpha",
        "mx-1bb21d",
    ]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    // the junk is dropped, the survivor stays
    let after = read_store_file(&ours.0, "expertise/alpha.jsonl");
    assert!(!after.contains("comment"), "junk dropped: {after}");
    assert!(!after.contains("mx-1bb21d"), "target gone: {after}");
    assert!(
        after.contains("\"zzz_custom\":\"keepme\""),
        "additive fields survive the rewrite: {after}"
    );

    // deleting the last record leaves a 0-byte file (never removes it)
    let ours = seeded_store(
        "dlast",
        None,
        &[("beta", format!("{ALPHA_ONE}\n").as_str())],
        &[],
    );
    let run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "delete",
        "beta",
        "mx-1bb21d",
    ]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert_eq!(read_store_file(&ours.0, "expertise/beta.jsonl"), "");
    assert!(ours.0.join(".mulch/expertise/beta.jsonl").is_file());
}

#[test]
fn delete_domain_file_presence_pins_the_modes() {
    let seed = format!("{ALPHA_ONE}\n{ALPHA_TWO}\n");
    // --yes and --json (prompt skipped) remove the file; --dry-run
    // keeps it
    let cases: [&[&str]; 3] = [
        &["delete-domain", "alpha", "--yes"],
        &["delete-domain", "alpha", "--json"],
        &["delete-domain", "alpha", "--yes", "--dry-run"],
    ];
    for (index, args) in cases.iter().enumerate() {
        let ours = seeded_store(
            &format!("dd{index}"),
            None,
            &[("alpha", seed.as_str()), ("beta", "")],
            &[],
        );
        let run = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(run.code, 0, "{args:?}: {}", run.stderr);
        // --dry-run keeps the file, the apply modes remove it
        let keeps_file = args.contains(&"--dry-run");
        assert_eq!(
            ours.0.join(".mulch/expertise/alpha.jsonl").exists(),
            keeps_file,
            "{args:?} file presence"
        );
    }
}

#[test]
fn delete_and_move_abort_on_malformed_jsonl_with_the_pinned_template() {
    let seed = format!("{ALPHA_ONE}\n{MALFORMED_LINE}\n{ALPHA_TWO}\n");
    let beta = format!("{BETA_ONE_LINE}\n");
    let commands: [&[&str]; 4] = [
        &["delete", "alpha", "mx-1bb21d"],
        &["delete", "alpha", "--records", "mx-1bb21d"],
        &["delete-domain", "alpha", "--yes"],
        &["move", "alpha", "mx-1bb21d", "beta"],
    ];
    for (index, args) in commands.iter().enumerate() {
        let ours = seeded_store(
            &format!("mal{index}"),
            None,
            &[("alpha", seed.as_str()), ("beta", beta.as_str())],
            &[],
        );
        let run = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(run.code, 1, "{args:?} aborts");
        assert_eq!(run.stdout, "", "{args:?} prints nothing to stdout");
        // The template is pinned; the parser's own reason wording
        // (serde) inside it is not.
        let prefix = "Error: Malformed JSONL at <DIR>/.mulch/expertise/alpha.jsonl:2: ";
        let suffix = format!(". Line: {MALFORMED_LINE}\n");
        let text = normalize_dir(&run.stderr, &ours.0);
        assert!(text.starts_with(prefix), "template prefix: {text}");
        assert!(text.ends_with(&suffix), "template suffix: {text}");
        // Nothing was written.
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
        assert_eq!(read_store_file(&ours.0, "expertise/beta.jsonl"), beta);
    }

    // The preview is the trimmed line, cut at 77 chars plus "...".
    let long_line = format!("{{\"type\":\"convention\",\"name\":\"{}\"", "x".repeat(120));
    let preview = format!("{}...", &long_line[..77]);
    let ours = seeded_store(
        "mallong",
        None,
        &[("alpha", format!("{long_line}\n").as_str())],
        &[],
    );
    let run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "delete",
        "alpha",
        "mx-1bb21d",
    ]);
    assert_eq!(run.code, 1);
    let suffix = format!(". Line: {preview}\n");
    assert!(normalize_dir(&run.stderr, &ours.0).ends_with(&suffix));

    // `move` reads only the SOURCE strictly: a malformed line in the
    // target file neither blocks the move nor changes the append.
    let target = format!("{BETA_ONE_LINE}\n{MALFORMED_LINE}\n");
    for (index, extra) in [&[] as &[&str], &["--json"]].into_iter().enumerate() {
        let mut args = vec!["move", "alpha", "mx-1bb21d", "beta"];
        args.extend_from_slice(extra);
        let ours = seeded_store(
            &format!("maltgt{index}"),
            None,
            &[
                ("alpha", format!("{ALPHA_ONE}\n").as_str()),
                ("beta", target.as_str()),
            ],
            &[],
        );
        let run = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(run.code, 0, "{args:?}: {}", run.stderr);
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), "");
        let beta_after = read_store_file(&ours.0, "expertise/beta.jsonl");
        assert!(beta_after.contains(MALFORMED_LINE), "kept: {beta_after}");
        assert!(beta_after.contains("mx-1bb21d"), "appended: {beta_after}");
    }
}

#[test]
fn delete_all_except_unknown_id_pins_our_bytes() {
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
        let ours = seeded_store(
            &format!("except{index}"),
            None,
            &[("alpha", seed.as_str())],
            &[],
        );
        let run = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(run.code, 1, "{args:?} aborts");
        assert_eq!(run.stdout, "", "{args:?} nothing on stdout");
        assert_eq!(run.stderr, expected, "{args:?} our stderr");
        // The keep-list is resolved before anything is written.
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
    }
}

#[test]
fn identifier_resolution_pins_our_bytes() {
    let seed = format!("{ALPHA_ONE}\n{ALPHA_TWO}\n");
    let survivor = format!("{ALPHA_TWO}\n");
    // Exact id, bare hash and a unique prefix all resolve; the output
    // carries the RESOLVED id.
    let accepted: [&[&str]; 4] = [
        &["delete", "alpha", "mx-1bb21d"],
        &["delete", "alpha", "1bb21d"],
        &["delete", "alpha", "mx-1b"],
        &["delete", "alpha", "1bb21"],
    ];
    for (index, args) in accepted.iter().enumerate() {
        let ours = seeded_store(
            &format!("res{index}"),
            None,
            &[("alpha", seed.as_str())],
            &[],
        );
        let run = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(run.code, 0, "{args:?} resolves");
        assert_eq!(
            run.stdout, "✓ Deleted pattern mx-1bb21d from alpha: Alpha One\n",
            "{args:?} stdout"
        );
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), survivor);
    }

    let moved: [&str; 3] = ["mx-1bb21d", "1bb21d", "mx-1b"];
    for (index, id) in moved.iter().enumerate() {
        let ours = seeded_store(
            &format!("resmv{index}"),
            None,
            &[
                ("alpha", seed.as_str()),
                ("beta", format!("{BETA_ONE_LINE}\n").as_str()),
            ],
            &[],
        );
        let args = ["move", "alpha", id, "beta"];
        let run = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(run.code, 0, "{args:?} resolves");
        assert_eq!(
            run.stdout,
            "✓ Moved pattern mx-1bb21d from alpha → beta: Alpha One\n"
        );
        assert!(read_store_file(&ours.0, "expertise/beta.jsonl").contains("mx-1bb21d"));
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
        let ours = seeded_store(
            &format!("amb{index}"),
            None,
            &[
                ("alpha", seed.as_str()),
                ("beta", format!("{BETA_ONE_LINE}\n").as_str()),
            ],
            &[],
        );
        let run = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(run.code, 1, "{args:?} aborts");
        assert_eq!(run.stdout, "", "{args:?} nothing on stdout");
        assert_eq!(run.stderr, expected, "{args:?} our stderr");
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
    }

    let ours = seeded_store(
        "ambjson",
        None,
        &[
            ("alpha", seed.as_str()),
            ("beta", format!("{BETA_ONE_LINE}\n").as_str()),
        ],
        &[],
    );
    let args = ["move", "alpha", "mx-", "beta", "--json"];
    let run = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(run.code, 1);
    assert_eq!(run.stderr, error_envelope("move", ambiguous));
}

// ---- sprint 10 (mulch-5f8a): required common fields + batch enrichment ----

#[test]
fn delete_domain_preserves_remaining_domain_order() {
    // Removing the FIRST of three domains must not shuffle the rest.
    let ours = seeded_store(
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
    let run = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(run.code, 0, "delete-domain succeeds");
    let config = read_store_file(&ours.0, "mulch.config.yaml");
    let expected = format!("version: '1'\ndomains:\n  beta: {{}}\n  gamma: {{}}\n{CONFIG_TAIL}");
    assert_eq!(config, expected, "remaining domains keep their order");
    assert!(!config.contains("alpha"), "removed domain is gone");
    assert!(!ours.0.join(".mulch/expertise/alpha.jsonl").exists());

    // Removing the MIDDLE domain keeps the outer order too.
    let ours = seeded_store(
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
    let run = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert_eq!(
        read_store_file(&ours.0, "mulch.config.yaml"),
        format!("version: '1'\ndomains:\n  alpha: {{}}\n  gamma: {{}}\n{CONFIG_TAIL}")
    );

    // A registered domain whose file is already gone still deletes.
    let ours = seeded_store(
        "ddgone",
        None,
        &[
            ("alpha", ""),
            ("beta", format!("{BETA_ONE_LINE}\n").as_str()),
        ],
        &[],
    );
    std::fs::remove_file(ours.0.join(".mulch/expertise/alpha.jsonl")).expect("seed file");
    let args = ["delete-domain", "alpha", "--yes"];
    let run = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(run.code, 0, "missing file is tolerated");
}

#[test]
fn delete_domain_backfills_missing_config_blocks() {
    // The minimal config (version + domains only) gains the full
    // governance and shelf-life blocks on rewrite.
    let minimal = "version: '1'\ndomains:\n  alpha: {}\n  beta: {}\n";
    let ours = seeded_store(
        "ddmin",
        Some(minimal),
        &[("alpha", format!("{ALPHA_ONE}\n").as_str()), ("beta", "")],
        &[],
    );
    let args = ["delete-domain", "alpha", "--yes"];
    let run = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(run.code, 0, "{}", run.stderr);
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
    let ours = seeded_store(
        "ddmin2",
        Some("version: '1'\ndomains:\n  alpha: {}\n  beta: {}\n  gamma: {}\n"),
        &[("alpha", ""), ("beta", ""), ("gamma", "")],
        &[],
    );
    let args = ["delete-domain", "beta", "--yes"];
    let run = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert_eq!(
        read_store_file(&ours.0, "mulch.config.yaml"),
        format!("version: '1'\ndomains:\n  alpha: {{}}\n  gamma: {{}}\n{CONFIG_TAIL}")
    );
}

#[test]
fn delete_domain_dry_run_json_pins_our_envelope() {
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
        let ours = seeded_store(
            &format!("dddry{index}"),
            None,
            &[("alpha", seed.as_str()), ("beta", "")],
            &[],
        );
        let run = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(run.code, 0, "{args:?} succeeds");
        assert_eq!(run.stdout, expected, "{args:?} is the json envelope");
        assert_eq!(run.stderr, "", "{args:?} silent");
        // Dry run: file and config stay untouched.
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
        assert!(ours.0.join(".mulch/expertise/alpha.jsonl").is_file());
    }
}

#[test]
fn delete_mode_and_unknown_domain_errors_pin_our_bytes() {
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
        let ours = seeded_store(
            &format!("mode{index}"),
            None,
            &[("alpha", seed.as_str()), ("beta", "")],
            &[],
        );
        let run = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(run.code, 1, "{args:?} aborts");
        assert_eq!(run.stdout, "", "{args:?} nothing on stdout");
        assert_eq!(run.stderr, expected, "{args:?} our stderr");
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
        let ours = seeded_store(
            &format!("movedom{index}"),
            None,
            &[("alpha", seed.as_str()), ("beta", "")],
            &[],
        );
        let run = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(run.code, 1, "{args:?} aborts");
        assert_eq!(run.stderr, expected, "{args:?} our stderr");
    }

    // No domains at all renders `(none)`; the list joins with ", ".
    let ours = seeded_store("modenone", None, &[], &[]);
    let args = ["delete", "nope", "mx-1bb21d"];
    let run = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(run.code, 1);
    assert_eq!(
        run.stderr,
        "Error: domain \"nope\" not found in config.\nAvailable domains: (none)\n"
    );

    let ours = seeded_store(
        "modemulti",
        None,
        &[("alpha", ""), ("beta", ""), ("gamma", "")],
        &[],
    );
    let args = ["delete", "nope", "mx-1bb21d", "--json"];
    let run = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(run.code, 1);
    assert_eq!(
        run.stderr,
        error_envelope(
            "delete",
            "Domain \"nope\" not found in config. Available domains: alpha, beta, gamma",
        )
    );
}

#[test]
fn delete_all_except_keeping_all_prints_nothing() {
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
        let ours = seeded_store(
            &format!("keepall{index}"),
            None,
            &[("alpha", seed.as_str())],
            &[],
        );
        let run = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(run.code, 0, "{args:?} succeeds");
        // Zero deletions: plain mode prints NOTHING (not an empty line).
        assert_eq!(run.stdout, expected, "{args:?} stdout");
        assert_eq!(run.stderr, "", "{args:?} no stderr");
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
    }
}

#[test]
fn delete_and_move_truncate_convention_summary() {
    let seed = format!("{}\n", long_convention_line());
    let ours = seeded_store("trunc", None, &[("alpha", seed.as_str())], &[]);
    let args = ["delete", "alpha", "mx-1bb21d"];
    let run = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert_eq!(
        run.stdout,
        format!(
            "✓ Deleted convention mx-1bb21d from alpha: {}\n",
            truncated_summary()
        )
    );

    let ours = seeded_store(
        "truncmv",
        None,
        &[("alpha", seed.as_str()), ("beta", "")],
        &[],
    );
    let args = ["move", "alpha", "mx-1bb21d", "beta"];
    let run = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert_eq!(
        run.stdout,
        format!(
            "✓ Moved convention mx-1bb21d from alpha → beta: {}\n",
            truncated_summary()
        )
    );
}

#[test]
fn delete_reserializes_survivors() {
    // A survivor written with spaces must come back compact, and a
    // survivor without an id gains a generated one.
    let spaced = "{ \"type\": \"pattern\" ,  \"classification\":\"tactical\", \"recorded_at\":\"2026-10-03T20:47:57.690Z\", \"name\":\"Spaced\", \"description\":\"d\", \"id\":\"mx-1bb21d\" }";
    let id_less = "{\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:58.000Z\",\"content\":\"no id here\"}";
    let seed = format!("{spaced}\n{ALPHA_TWO}\n{id_less}\n");
    let ours = seeded_store("reser", None, &[("alpha", seed.as_str())], &[]);
    let args = ["delete", "alpha", "mx-c7129f"];
    let run = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(run.code, 0, "{}", run.stderr);
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
    let ours = seeded_store(
        "resermv",
        None,
        &[
            ("alpha", format!("{spaced}\n{id_less}\n").as_str()),
            ("beta", format!("{BETA_ONE_LINE}\n").as_str()),
        ],
        &[],
    );
    let args = ["move", "alpha", "mx-1bb21d", "beta"];
    let run = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(run.code, 0, "{}", run.stderr);
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
fn delete_duplicate_records_delete_once() {
    let seed = format!("{ALPHA_ONE}\n{ALPHA_TWO}\n");
    let survivor = format!("{ALPHA_TWO}\n");
    let ours = seeded_store("dup", None, &[("alpha", seed.as_str())], &[]);
    let args = ["delete", "alpha", "--records", "mx-1bb21d,mx-1bb21d"];
    let run = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(run.code, 0, "{}", run.stderr);
    // One deletion line, no bulk tally.
    assert_eq!(
        run.stdout, "Deleted pattern mx-1bb21d from alpha: Alpha One\n",
        "a repeated id deletes once"
    );
    assert!(
        !run.stdout.contains("✓ Deleted 2 records"),
        "no phantom second deletion: {}",
        run.stdout
    );
    assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), survivor);

    let ours = seeded_store("dupjson", None, &[("alpha", seed.as_str())], &[]);
    let args = [
        "delete",
        "alpha",
        "--records",
        "mx-1bb21d,mx-1bb21d",
        "--json",
    ];
    let run = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert_eq!(
        run.stdout,
        "{\n  \"success\": true,\n  \"command\": \"delete\",\n  \"domain\": \"alpha\",\n  \"dryRun\": false,\n  \"deleted\": [\n    {\n      \"id\": \"mx-1bb21d\",\n      \"type\": \"pattern\",\n      \"summary\": \"Alpha One\"\n    }\n  ],\n  \"kept\": 1\n}\n"
    );
    assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), survivor);
}

/// `delete <unknown-domain>` with no id reports the unknown domain,
/// not the missing mode (delete.ts order; mulch-e645).
#[test]
fn delete_unknown_domain_precedes_mode_validation() {
    let ours = seeded_store("e645", None, &[("d", "")], &[]);
    let json_envelope = error_envelope(
        "delete",
        "Domain \"x\" not found in config. Available domains: d",
    );
    let plain = "Error: domain \"x\" not found in config.\nAvailable domains: d\n";
    for (args, expected) in [
        (vec!["delete", "x"], plain),
        (vec!["delete", "x", "--json"], json_envelope.as_str()),
        // mode-shaped invocations take the same domain-first order
        (vec!["delete", "x", "--records", "mx-aaaa01"], plain),
        (vec!["delete", "x", "--all-except", "mx-aaaa01"], plain),
    ] {
        let run = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(run.code, 1, "{args:?}");
        assert_eq!(run.stdout, "", "{args:?}");
        assert_eq!(run.stderr, expected, "{args:?}");
    }
}

/// The reverse pin: a KNOWN domain with no id still fails the mode
/// validation (the seam must not swallow the mode error).
#[test]
fn delete_known_domain_without_id_still_fails_mode_validation() {
    let ours = seeded_store("e645r", None, &[("d", "")], &[]);
    // plain keeps the lowercase wording, json capitalizes inside the
    // envelope — both still fail the mode validation
    let run = run_in(&ours.0, Path::new(mulch_bin()), &["delete", "d"]);
    assert_eq!(run.code, 1);
    assert_eq!(
        run.stderr,
        "Error: must provide a record ID, --records, or --all-except.\n"
    );
    let run = run_in(&ours.0, Path::new(mulch_bin()), &["delete", "d", "--json"]);
    assert_eq!(run.code, 1);
    assert_eq!(
        run.stderr,
        error_envelope(
            "delete",
            "Must provide a record ID, --records, or --all-except."
        )
    );
}

// Sprint 22, mulch-7896 (lane iterate-s22-c2).
// Pins: edit validates the UPDATED record against the schema before
// writing — missing required payload, unknown extra fields,
// wrong-typed payloads, and the legacy singular `outcome` key all
// abort (exit 1, store untouched), in plain and json; a fixing edit
// still writes; --files is dropped for types that do not declare it;
// new keys append in application order; move aborts on legacy-outcome
// records.
