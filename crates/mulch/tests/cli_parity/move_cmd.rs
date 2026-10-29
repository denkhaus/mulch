//! `mulch move` pins (ADR-0008: our byte pins only; literals frozen
//! from the parity era's twin runs — the reference twin is gone).

use std::path::Path;

use super::support::*;

#[test]
fn move_success_and_error_surfaces_pin_their_bytes() {
    let alpha = format!("{ALPHA_ONE}\n{ALPHA_TWO}\n");
    let beta = format!("{BETA_ONE_LINE}\n");
    // plain, --json, and --dry-run share the success law
    for (index, (args, expect_write)) in [
        (&["move", "alpha", "mx-1bb21d", "beta"][..], true),
        (&["move", "alpha", "mx-1bb21d", "beta", "--json"][..], true),
        (
            &["move", "alpha", "mx-1bb21d", "beta", "--dry-run"][..],
            false,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let ours = seeded_store(
            &format!("mv{index}"),
            None,
            &[("alpha", alpha.as_str()), ("beta", beta.as_str())],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 0, "{args:?}: {}", our.stderr);
        if args.contains(&"--dry-run") {
            assert_eq!(
                our.stdout, "[DRY RUN] Would move pattern mx-1bb21d from alpha → beta: Alpha One\n",
                "{args:?}"
            );
        } else if args.contains(&"--json") {
            assert_eq!(
                our.stdout,
                "{\n  \"success\": true,\n  \"command\": \"move\",\n  \"sourceDomain\": \
                 \"alpha\",\n  \"targetDomain\": \"beta\",\n  \"record\": {\n    \"id\": \
                 \"mx-1bb21d\",\n    \"type\": \"pattern\",\n    \"summary\": \"Alpha One\"\n  },\n  \
                 \"incomingReferences\": []\n}\n",
                "{args:?}"
            );
        } else {
            assert_eq!(
                our.stdout, "✓ Moved pattern mx-1bb21d from alpha → beta: Alpha One\n",
                "{args:?}"
            );
        }
        assert_eq!(our.stderr, "", "{args:?}");
        if expect_write {
            assert_eq!(
                read_store_file(&ours.0, "expertise/beta.jsonl"),
                format!("{BETA_ONE_LINE}\n{ALPHA_ONE}\n"),
                "{args:?} target"
            );
            assert_eq!(
                read_store_file(&ours.0, "expertise/alpha.jsonl"),
                format!("{ALPHA_TWO}\n"),
                "{args:?} source"
            );
        } else {
            // the dry run mutates nothing
            assert_eq!(
                read_store_file(&ours.0, "expertise/alpha.jsonl"),
                alpha,
                "{args:?} source untouched"
            );
            assert_eq!(
                read_store_file(&ours.0, "expertise/beta.jsonl"),
                beta,
                "{args:?} target untouched"
            );
        }
    }

    // errors: same domain (checked by its own test), unknown target,
    // schema-invalid record — exit 1, nothing on stdout, store intact
    let invalid = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:57.690Z\",\"name\":\"Broken\",\"id\":\"mx-f16294\"}\n";
    let error_cases: [(&[&str], &str, &str); 2] = [
        (
            &["move", "alpha", "mx-1bb21d", "zeta"],
            alpha.as_str(),
            "Error: Domain \"zeta\" not found in config. Available domains: alpha, beta\n",
        ),
        (
            &["move", "alpha", "mx-f16294", "beta"],
            invalid,
            "", // the invalid-record wording is the schema battery's
        ),
    ];
    for (index, (args, body, expected)) in error_cases.iter().enumerate() {
        let ours = seeded_store(
            &format!("mve{index}"),
            None,
            &[("alpha", body), ("beta", beta.as_str())],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 1, "{args:?} aborts");
        assert_eq!(our.stdout, "", "{args:?} nothing on stdout");
        if expected.is_empty() {
            assert!(
                normalize_dir(&our.stderr, &ours.0).contains("Record fails schema validation"),
                "{args:?}: {}",
                our.stderr
            );
        } else {
            assert_eq!(our.stderr, *expected, "{args:?}");
        }
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), *body);
    }
}

#[test]
fn move_incoming_references() {
    // gamma refers to the moved id -> incomingReferences entry
    let gamma = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:48:00.000Z\",\"name\":\"Gamma One\",\"description\":\"refers\",\"relates_to\":[\"mx-1bb21d\"],\"id\":\"mx-da09e8\"}\n";
    let ours = seeded_store(
        "mvr",
        None,
        &[
            ("alpha", &format!("{ALPHA_ONE}\n")),
            ("beta", &format!("{ALPHA_TWO}\n")),
            ("gamma", gamma),
        ],
        &[],
    );
    let args = ["move", "alpha", "mx-1bb21d", "beta", "--json"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert!(our.stdout.contains("incomingReferences"));
    assert!(our.stdout.contains("mx-da09e8"), "referrer reported");
}

// ---- sprint 3 (mulch-dc67) hardening: strict reads, identifier
// resolution, config order and the move gates ----

#[test]
fn move_same_domain_check_precedes_domain_lookup() {
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
        let ours = seeded_store(
            &format!("samedom{index}"),
            None,
            &[("alpha", seed.as_str()), ("beta", "")],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 1, "{args:?} aborts");
        assert_eq!(our.stdout, "", "{args:?} nothing on stdout");
        assert_eq!(our.stderr, message, "{args:?} our stderr");
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
    }

    let ours = seeded_store(
        "samedomjson",
        None,
        &[("alpha", seed.as_str()), ("beta", "")],
        &[],
    );
    let args = ["move", "nope", "mx-1bb21d", "nope", "--json"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(our.code, 1);
    assert_eq!(
        our.stderr,
        error_envelope(
            "move",
            "Source and target domain are the same — nothing to move.",
        )
    );

    // Two DIFFERENT unknown domains fall through to the domain lookup.
    let ours = seeded_store(
        "unkdoms",
        None,
        &[("alpha", seed.as_str()), ("beta", "")],
        &[],
    );
    let args = ["move", "zeta", "mx-1bb21d", "nope"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(our.code, 1);
    assert_eq!(
        our.stderr,
        "Error: Domain \"zeta\" not found in config. Available domains: alpha, beta\n",
    );
}

#[test]
fn move_refuses_archived_records() {
    let seed = format!("{ARCHIVED_ALPHA_ONE}\n");
    let message = "Error: Record mx-1bb21d is archived. Run `mulch restore mx-1bb21d` first.\n";
    for (index, args) in [
        &["move", "alpha", "mx-1bb21d", "beta"] as &[&str],
        &["move", "alpha", "mx-1bb21d", "beta", "--force"],
        &["move", "alpha", "1bb21d", "beta"],
    ]
    .into_iter()
    .enumerate()
    {
        let ours = seeded_store(
            &format!("arch{index}"),
            None,
            &[("alpha", seed.as_str()), ("beta", "")],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 1, "{args:?} aborts");
        assert_eq!(our.stdout, "", "{args:?} nothing on stdout");
        assert_eq!(our.stderr, message, "{args:?} our stderr");
        // The archived record stays put, the target stays empty.
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
        assert_eq!(read_store_file(&ours.0, "expertise/beta.jsonl"), "");
    }

    let ours = seeded_store(
        "archjson",
        None,
        &[("alpha", seed.as_str()), ("beta", "")],
        &[],
    );
    let args = ["move", "alpha", "mx-1bb21d", "beta", "--json"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(our.code, 1);
    assert_eq!(
        our.stderr,
        error_envelope(
            "move",
            "Record mx-1bb21d is archived. Run `mulch restore mx-1bb21d` first.",
        )
    );
}

#[test]
fn move_required_fields_gate() {
    let seed = format!("{ALPHA_ONE}\n");
    let message = "Record is missing field(s) required by target domain \"beta\": \"owner\". Edit the record (`mulch edit mx-1bb21d`) before moving.";
    // `--force` bypasses allowed_types only; required_fields always holds.
    for (index, args) in [
        &["move", "alpha", "mx-1bb21d", "beta"] as &[&str],
        &["move", "alpha", "mx-1bb21d", "beta", "--force"],
        &["move", "alpha", "1bb21d", "beta"],
    ]
    .into_iter()
    .enumerate()
    {
        let ours = seeded_store(
            &format!("req{index}"),
            Some(REQUIRED_FIELDS_CONFIG),
            &[("alpha", seed.as_str()), ("beta", "")],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 1, "{args:?} aborts");
        assert_eq!(our.stdout, "", "{args:?} nothing on stdout");
        assert_eq!(
            our.stderr,
            format!("Error: {message}\n"),
            "{args:?} our stderr"
        );
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
        assert_eq!(read_store_file(&ours.0, "expertise/beta.jsonl"), "");
    }

    let ours = seeded_store(
        "reqjson",
        Some(REQUIRED_FIELDS_CONFIG),
        &[("alpha", seed.as_str()), ("beta", "")],
        &[],
    );
    let args = ["move", "alpha", "mx-1bb21d", "beta", "--json"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(our.code, 1);
    assert_eq!(our.stderr, error_envelope("move", message));

    // A target that does not require the field moves normally.
    let ours = seeded_store("reqok", None, &[("alpha", seed.as_str()), ("beta", "")], &[
    ]);
    let args = ["move", "alpha", "mx-1bb21d", "beta", "--json"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert_eq!(
        read_store_file(&ours.0, "expertise/beta.jsonl"),
        format!("{ALPHA_ONE}\n")
    );
    assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), "");
}

#[test]
fn move_allowed_types_gate() {
    let seed = format!("{ALPHA_ONE}\n");
    let message = "Type \"pattern\" is not in target domain \"beta\" allowed_types (convention). Pass --force to override, or adjust mulch.config.yaml.";
    let ours = seeded_store(
        "allowtype",
        Some(ALLOWED_TYPES_CONFIG),
        &[("alpha", seed.as_str()), ("beta", "")],
        &[],
    );
    let args = ["move", "alpha", "mx-1bb21d", "beta"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(our.code, 1, "the gate refuses");
    assert_eq!(our.stdout, "", "nothing on stdout");
    assert_eq!(our.stderr, format!("Error: {message}\n"), "our stderr");
    assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
    assert_eq!(read_store_file(&ours.0, "expertise/beta.jsonl"), "");

    let ours = seeded_store(
        "allowtypejson",
        Some(ALLOWED_TYPES_CONFIG),
        &[("alpha", seed.as_str()), ("beta", "")],
        &[],
    );
    let args = ["move", "alpha", "mx-1bb21d", "beta", "--json"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(our.code, 1);
    assert_eq!(our.stderr, error_envelope("move", message));

    // `--force` overrides the allowed_types gate.
    let ours = seeded_store(
        "allowtypeforce",
        Some(ALLOWED_TYPES_CONFIG),
        &[("alpha", seed.as_str()), ("beta", "")],
        &[],
    );
    let args = ["move", "alpha", "mx-1bb21d", "beta", "--force"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(
        our.code, 0,
        "--force overrides allowed_types: {}",
        our.stderr
    );
    assert_eq!(
        read_store_file(&ours.0, "expertise/beta.jsonl"),
        format!("{ALPHA_ONE}\n")
    );
}

#[test]
fn move_incoming_references_scan_orphan_files() {
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
        let ours = seeded_store(&format!("incref{index}"), None, &seeds, &orphan_seeds);
        let mut args = vec!["move", "alpha", "mx-1bb21d", "beta"];
        args.extend_from_slice(extra);
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(our.code, 0, "{args:?} succeeds");
        assert_eq!(
            our.stdout, expected,
            "{args:?} reports the orphan referrers"
        );
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
fn move_aborts_on_legacy_outcome_records() {
    let ours = TempDir::new("mleg-ours");
    let ours_ids = craft_gate_store(&ours.0);
    let legacy = ours_ids["legacy body"].clone();

    let args = ["move", "d", &legacy, "d2"];
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(ours_run.code, 1, "the legacy record aborts the move");
    assert_eq!(ours_run.stdout, "");
    assert!(
        normalize_dir(&ours_run.stderr, &ours.0).starts_with(
            "Error: Record fails schema validation:  must NOT have additional properties"
        ),
        "{}",
        ours_run.stderr
    );
    // d2 untouched.
    assert_eq!(read_store_file(&ours.0, "expertise/d2.jsonl"), "");
}

// ---- pre/post-record hooks (mulch-0e87) ----

/// The seeded source/target pair the hook batteries move across
/// (config carries the hook block under test).
fn hook_store(tag: &str, hook_yaml: &str) -> TempDir {
    let config = format!("version: '1'\ndomains:\n  alpha: {{}}\n  beta: {{}}\n{hook_yaml}");
    let alpha = format!("{ALPHA_ONE}\n{ALPHA_TWO}\n");
    let beta = format!("{BETA_ONE_LINE}\n");
    let ours = TempDir::new(format!("{tag}-o").as_str());
    write_seed_with_config(&ours.0, &config, &[
        ("alpha", alpha.as_str()),
        ("beta", beta.as_str()),
    ]);
    ours
}

/// A blocking pre-record hook (exit 3) aborts the move on both
/// channels with nothing written — and, unlike the record path's
/// payload, the hook's stdin carries the record WITH its id (the
/// documented divergence; the stdin file pins it).
#[test]
fn move_pre_hook_block() {
    let ours = hook_store(
        "mvhook-block",
        "hooks:\n  pre-record:\n    - 'cat > hook-stdin.txt; printf from-hook-stderr >&2; exit 3'\n",
    );
    for mode in [&[] as &[&str], &["--json"]] {
        let mut args: Vec<&str> = vec!["move", "alpha", "mx-1bb21d", "beta"];
        args.extend_from_slice(mode);
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(our.code, 1, "exit {mode:?}");
        assert_eq!(our.stdout, "", "stdout {mode:?}");
        assert!(
            normalize(&our.stderr).contains("exited with code 3"),
            "stderr {mode:?}: {}",
            our.stderr
        );
        // nothing moved
        assert!(read_store_file(&ours.0, "expertise/alpha.jsonl").contains(ALPHA_ONE));
        assert_eq!(
            read_store_file(&ours.0, "expertise/beta.jsonl"),
            format!("{BETA_ONE_LINE}\n")
        );
    }
    // The payload kept the id: the captured stdin's record carries
    // mx-1bb21d (the record path sends its payload id-less).
    let stdin = hook_stdin(&ours.0).expect("hook stdin captured");
    assert!(stdin.contains("mx-1bb21d"), "id retained: {stdin}");
}

/// A mutating pre-record hook: the ADOPTED record is what moves (and
/// what post-record reports), validated through the shared gate.
#[test]
fn move_pre_hook_adoption() {
    // Inner double quotes are backslash-escaped for the printf string
    // (compact's hook_replacement_stdout law — an unescaped body
    // prints garbage and takes the non-JSON warning path instead).
    let body = r#"{"payload":{"record":{"type":"pattern","classification":"tactical","recorded_at":"2026-10-03T20:47:58.427Z","name":"Alpha Hooked","description":"mutated by hook","id":"mx-1bb21d"}}}"#;
    let stdout_body = body.replace('"', "\\\"");
    let ours = hook_store(
        "mvhook-adopt",
        &format!("hooks:\n  pre-record:\n    - 'cat > /dev/null; printf %s \"{stdout_body}\"'\n"),
    );
    let args = ["move", "alpha", "mx-1bb21d", "beta"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert!(
        read_store_file(&ours.0, "expertise/beta.jsonl").contains("mutated by hook"),
        "the adopted record moved"
    );
    assert!(!read_store_file(&ours.0, "expertise/alpha.jsonl").contains("Alpha One\""));
}

/// Hook warnings render MOVE's way: plain prints `  warning: <w>`
/// on STDOUT (move.ts:308 — the one command that differs from the
/// shared stderr `Warning:` family); json carries the warnings array.
#[test]
fn move_hook_warnings_render_move_style() {
    let ours = hook_store(
        "mvhook-warn",
        "hooks:\n  pre-record:\n    - 'cat > /dev/null; printf from-hook-stderr >&2'\n  post-record:\n    - 'cat > /dev/null; printf post-hook-note >&2'\n",
    );
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "move",
        "alpha",
        "mx-1bb21d",
        "beta",
    ]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    // both hooks' stderr forwards; the move happened anyway
    assert!(our.stderr.contains("from-hook-stderr"), "{}", our.stderr);
    assert!(our.stderr.contains("post-hook-note"), "{}", our.stderr);
    assert!(
        read_store_file(&ours.0, "expertise/beta.jsonl").contains("mx-1bb21d"),
        "the move happened"
    );
}

/// Dry runs fire NO hooks at all. Ungated: the marker file is ours.
#[test]
fn move_hook_dry_run_fires_no_hooks() {
    let ours = hook_store(
        "mvhook-dry",
        "hooks:\n  pre-record:\n    - 'cat > /dev/null; printf ran > hook-ran.txt'\n",
    );
    let run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "move",
        "alpha",
        "mx-1bb21d",
        "beta",
        "--dry-run",
    ]);
    assert_eq!(run.code, 0);
    assert!(
        !ours.0.join("hook-ran.txt").exists(),
        "dry runs fire no pre-record"
    );
    assert!(hook_stdin(&ours.0).is_none(), "no stdin capture either");
}
