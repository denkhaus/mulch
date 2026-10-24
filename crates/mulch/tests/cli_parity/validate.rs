//! `mulch validate` + schema/parse-layer parity probes.
//!
//! Moved verbatim from the one-file battery (mulch-3e7c); sprint
//! banners stay with their probes.

use std::path::Path;

use super::support::*;

#[test]
fn validate_clean_and_broken_match() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("val-ours");
    let theirs = TempDir::new("val-theirs");
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
    }

    // clean
    let ours_clean = run_in(&ours.0, Path::new(mulch_bin()), &["validate"]);
    let theirs_clean = run_in(&theirs.0, &ml, &["validate"]);
    assert_eq!(ours_clean.code, theirs_clean.code);
    assert_eq!(ours_clean.stdout, theirs_clean.stdout);

    // broken: convention record missing --content + garbage line
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &[
            "record",
            "dev",
            "--type",
            "convention",
            "--name",
            "t1",
            "--description",
            "d",
        ]);
        let file = dir.join(".mulch/expertise/dev.jsonl");
        let mut text = std::fs::read_to_string(&file).expect("domain file");
        // remove the just-added invalid line? no — the reference wrote it and
        // kept it; we only append garbage below (probe scenario B)
        text.push_str("this line is not json\n");
        std::fs::write(&file, text).expect("append garbage");
    }

    let ours_broken = run_in(&ours.0, Path::new(mulch_bin()), &["validate"]);
    let theirs_broken = run_in(&theirs.0, &ml, &["validate"]);
    assert_eq!(ours_broken.code, theirs_broken.code);
    assert_eq!(ours_broken.stdout, theirs_broken.stdout);
    assert_eq!(ours_broken.stderr, theirs_broken.stderr);

    let ours_json = run_in(&ours.0, Path::new(mulch_bin()), &["validate", "--json"]);
    let theirs_json = run_in(&theirs.0, &ml, &["validate", "--json"]);
    assert_eq!(ours_json.code, theirs_json.code);
    assert_eq!(normalize(&ours_json.stdout), normalize(&theirs_json.stdout));
}

#[test]
fn parse_layer_contract() {
    let ours = TempDir::new("parse");
    let bin = Path::new(mulch_bin());

    // no args: help on stderr, exit 1, stdout empty
    let run = run_in(&ours.0, bin, &[]);
    assert_eq!(run.code, 1);
    assert_eq!(run.stdout, "");
    assert!(run.stderr.starts_with("mulch"), "help goes to stderr");

    // -v and --version: bare version, exit 0
    for flag in ["-v", "--version"] {
        let run = run_in(&ours.0, bin, &[flag]);
        assert_eq!(run.code, 0);
        assert_eq!(run.stdout, concat!(env!("CARGO_PKG_VERSION"), "\n"));
    }

    // unknown command: exact two-line stderr, no timing noise
    let run = run_in(&ours.0, bin, &["nonsense", "--timing"]);
    assert_eq!(run.code, 1);
    assert_eq!(run.stdout, "");
    assert_eq!(
        run.stderr,
        "Unknown command: nonsense\nRun 'mulch --help' for usage.\n"
    );
}

#[test]
fn schema_invalid_strings_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("schemainv-ours");
    let theirs = TempDir::new("schemainv-theirs");
    let record = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T10:00:00.000Z\",\"name\":\"x\"}";
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let file = dir.join(".mulch/expertise/dev.jsonl");
        std::fs::write(file, format!("{record}\n")).expect("write record");
        let cfg = dir.join(".mulch/mulch.config.yaml");
        let text = std::fs::read_to_string(&cfg).expect("config");
        std::fs::write(cfg, text.replace("domains: {}", "domains:\n  dev: {}"))
            .expect("register domain");
    }

    // validate plain: summary on stdout, multi-line details on stderr
    let ours_plain = run_in(&ours.0, Path::new(mulch_bin()), &["validate"]);
    let theirs_plain = run_in(&theirs.0, &ml, &["validate"]);
    assert_eq!(ours_plain.code, theirs_plain.code);
    assert_eq!(ours_plain.stdout, theirs_plain.stdout);
    assert_eq!(ours_plain.stderr, theirs_plain.stderr);

    // validate json: envelope on stdout, message strings pinned
    let ours_json = run_in(&ours.0, Path::new(mulch_bin()), &["validate", "--json"]);
    let theirs_json = run_in(&theirs.0, &ml, &["validate", "--json"]);
    assert_eq!(ours_json.code, theirs_json.code);
    assert_eq!(normalize(&ours_json.stdout), normalize(&theirs_json.stdout));

    // doctor plain: the schema-validation detail line matches modulo
    // the upgrade check
    let ours_doc = run_in(&ours.0, Path::new(mulch_bin()), &["doctor"]);
    let theirs_doc = run_in(&theirs.0, &ml, &["doctor"]);
    assert_eq!(ours_doc.code, theirs_doc.code);
    assert!(
        ours_doc
            .stdout
            .contains("must have required property 'description'")
    );
    assert_eq!(
        ours_doc
            .stdout
            .lines()
            .filter(|l| l.contains("must have required property"))
            .collect::<Vec<_>>(),
        theirs_doc
            .stdout
            .lines()
            .filter(|l| l.contains("must have required property"))
            .collect::<Vec<_>>()
    );
}

#[test]
fn blank_lines_keep_physical_line_numbers() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("blank-ours");
    let theirs = TempDir::new("blank-theirs");
    let valid = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T10:00:00.000Z\",\"name\":\"p\",\"description\":\"d\"}";
    let invalid = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T10:00:00.000Z\",\"name\":\"x\"}";
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let file = dir.join(".mulch/expertise/dev.jsonl");
        std::fs::write(file, format!("\n{valid}\n{invalid}\nnot json\n")).expect("write lines");
        let cfg = dir.join(".mulch/mulch.config.yaml");
        let text = std::fs::read_to_string(&cfg).expect("config");
        std::fs::write(cfg, text.replace("domains: {}", "domains:\n  dev: {}"))
            .expect("register domain");
    }

    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["validate"]);
    let theirs_run = run_in(&theirs.0, &ml, &["validate"]);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);
    assert_eq!(ours_run.stderr, theirs_run.stderr);
    // physical addressing: findings at lines 3 and 4, not 2 and 3
    assert!(
        ours_run
            .stderr
            .contains("dev:3 - Schema validation failed:")
    );
    assert!(ours_run.stderr.contains("dev:4 - Invalid JSON"));
}

#[test]
fn unknown_record_type_errors_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let seed = format!("{ALPHA_ONE}\n{UNKNOWN_TYPE_LINE}\n");
    let expected = "Error: Unknown record type \"zzzcustom\" at <DIR>/.mulch/expertise/alpha.jsonl:2 (id=mx-zzz111). Register it under custom_types in mulch.config.yaml, remove the record, or pass --allow-unknown-types to bypass.\n";
    let commands: [&[&str]; 3] = [
        &["delete", "alpha", "mx-1bb21d"],
        &["delete-domain", "alpha", "--yes"],
        &["move", "alpha", "mx-1bb21d", "beta"],
    ];
    for (index, args) in commands.iter().enumerate() {
        let (ours, theirs) = twin_seeded(
            &format!("unk{index}"),
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
        assert_same_stderr(&ours, &our, &theirs, &their, expected);
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
        assert_eq!(read_store_file(&theirs.0, "expertise/alpha.jsonl"), seed);
    }

    // Without an id the `(id=…)` part is omitted.
    let id_less = "{\"type\":\"zzzcustom\",\"name\":\"Odd\"}";
    let expected = "Error: Unknown record type \"zzzcustom\" at <DIR>/.mulch/expertise/alpha.jsonl:1. Register it under custom_types in mulch.config.yaml, remove the record, or pass --allow-unknown-types to bypass.\n";
    let (ours, theirs) = twin_seeded("unknoid", None, &[("alpha", id_less)], &[]);
    let args = ["delete", "alpha", "mx-1bb21d"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, 1);
    assert_same_stderr(&ours, &our, &theirs, &their, expected);

    // `--allow-unknown-types` reads through the unknown line; a KNOWN
    // record can then be removed while the unknown record survives.
    for (index, args) in [
        &["delete", "alpha", "mx-1bb21d", "--allow-unknown-types"] as &[&str],
        &[
            "move",
            "alpha",
            "mx-1bb21d",
            "beta",
            "--allow-unknown-types",
        ],
        &[
            "delete",
            "alpha",
            "--all-except",
            "mx-zzz111",
            "--allow-unknown-types",
        ],
    ]
    .into_iter()
    .enumerate()
    {
        let (ours, theirs) = twin_seeded(
            &format!("unkallow{index}"),
            None,
            &[
                ("alpha", seed.as_str()),
                ("beta", format!("{BETA_ONE_LINE}\n").as_str()),
            ],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, &ml, args);
        assert_eq!(our.code, their.code, "{args:?} exit");
        assert_eq!(our.stdout, their.stdout, "{args:?} stdout");
        assert_eq!(our.stderr, their.stderr, "{args:?} stderr");
        assert_same_file(&ours, &theirs, "expertise/alpha.jsonl");
        assert_same_file(&ours, &theirs, "expertise/beta.jsonl");
    }
}

#[test]
fn required_common_fields_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    // Hand-written stores (the `ml record` fixture would always carry
    // classification + recorded_at, which hid this gap for 9 sprints).
    let stores: [(&str, &str); 4] = [
        (
            "missing-both",
            "{\"type\":\"convention\",\"content\":\"c\",\"id\":\"mx-aaaaaa\"}\n",
        ),
        (
            "missing-recorded",
            "{\"type\":\"pattern\",\"name\":\"n\",\"description\":\"d\",\"classification\":\"tactical\",\"id\":\"mx-bbbbbb\"}\n",
        ),
        (
            "const-mix",
            "{\"type\":\"convention\",\"name\":\"n\",\"description\":\"d\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"id\":\"mx-cccccc\"}\n",
        ),
        (
            "no-type",
            "{\"name\":\"n\",\"description\":\"d\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"id\":\"mx-dddddd\"}\n",
        ),
    ];
    for (index, (tag, corpus)) in stores.iter().enumerate() {
        // Plain `doctor` also runs the network update check (a known
        // deviation), so it is compared by exit code only; the json
        // shape is byte-comparable.
        let (ours, theirs) = twin_seeded(&format!("req{index}"), None, &[("alpha", *corpus)], &[]);
        for args in [
            &["validate"] as &[&str],
            &["--json", "validate"],
            &["--json", "doctor"],
            &["doctor"],
        ] {
            let our = run_in(&ours.0, Path::new(mulch_bin()), args);
            let their = run_in(&theirs.0, &ml, args);
            assert_eq!(our.code, their.code, "{tag} {args:?} exit code");
            assert_eq!(our.code, 1, "{tag} {args:?} must fail like the reference");
            if args.contains(&"doctor") && !args.contains(&"--json") {
                // doctor also runs the network update check (known
                // deviation): compare only the failure line
                assert!(
                    our.stdout.contains("failed schema validation"),
                    "{tag} plain doctor reports the failure"
                );
            } else if args.contains(&"doctor") {
                // json doctor: compare the schema-validation check and
                // the fail count (the upgrade check is network-bound)
                let ours_json: serde_json::Value =
                    serde_json::from_str(&our.stdout).expect("our json");
                let theirs_json: serde_json::Value =
                    serde_json::from_str(&their.stdout).expect("their json");
                let pick = |value: &serde_json::Value| {
                    value["checks"]
                        .as_array()
                        .expect("checks")
                        .iter()
                        .find(|check| check["name"] == "schema-validation")
                        .cloned()
                        .expect("schema-validation check")
                };
                assert_eq!(
                    pick(&ours_json),
                    pick(&theirs_json),
                    "{tag} schema-validation check"
                );
                assert_eq!(
                    ours_json["summary"]["fail"], theirs_json["summary"]["fail"],
                    "{tag} fail count"
                );
            } else {
                assert_eq!(
                    normalize_dir(&our.stdout, &ours.0),
                    normalize_dir(&their.stdout, &theirs.0),
                    "{tag} {args:?} stdout"
                );
                // plain validate renders the details on STDERR — the
                // gap that hid the const-mix spacing regression
                assert_eq!(
                    normalize_dir(&our.stderr, &ours.0),
                    normalize_dir(&their.stderr, &theirs.0),
                    "{tag} {args:?} stderr"
                );
            }
        }
    }

    // A batch record without the common fields is ENRICHED before
    // validation (reference `processStdinRecords`), so it is accepted.
    let batch = "[{\"type\":\"guide\",\"name\":\"g\",\"description\":\"gd\"}]";
    let (ours, theirs) = twin_seeded("reqbatch", None, &[("alpha", "")], &[]);
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(dir.join("batch.json"), batch).expect("batch file");
    }
    let args = ["record", "alpha", "--batch", "batch.json", "--json"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, their.code);
    assert_eq!(normalize(&our.stdout), normalize(&their.stdout));
    let ours_store = read_store_file(&ours.0, "expertise/alpha.jsonl");
    let theirs_store = read_store_file(&theirs.0, "expertise/alpha.jsonl");
    assert_eq!(normalize_line(&ours_store), normalize_line(&theirs_store));
    assert!(
        ours_store.contains("\"classification\":\"tactical\""),
        "batch enrichment fills classification: {ours_store}"
    );

    // an explicit recorded_at/classification survive the enrichment
    // (`if (!("recorded_at" in record))` — the values must not be
    // overwritten; timestamps are masked by normalize, so assert the
    // literal string survives on BOTH sides)
    let fixed = "2019-01-02T03:04:05.678Z";
    let batch_kept = format!(
        "[{{\"type\":\"guide\",\"name\":\"g2\",\"description\":\"gd2\",\"recorded_at\":\"{fixed}\",\"classification\":\"foundational\"}}]"
    );
    let (ours, theirs) = twin_seeded("reqbatch2", None, &[("alpha", "")], &[]);
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(dir.join("batch.json"), &batch_kept).expect("batch file");
    }
    let args = ["record", "alpha", "--batch", "batch.json", "--json"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, their.code);
    assert_eq!(normalize(&our.stdout), normalize(&their.stdout));
    let ours_store = read_store_file(&ours.0, "expertise/alpha.jsonl");
    let theirs_store = read_store_file(&theirs.0, "expertise/alpha.jsonl");
    assert!(
        ours_store.contains(fixed),
        "our enrichment must keep the explicit recorded_at: {ours_store}"
    );
    assert!(
        theirs_store.contains(fixed),
        "reference keeps the explicit recorded_at: {theirs_store}"
    );
    assert!(
        ours_store.contains("\"classification\":\"foundational\""),
        "our enrichment must keep the explicit classification: {ours_store}"
    );
    assert_eq!(normalize_line(&ours_store), normalize_line(&theirs_store));

    // Batch FAILURE surfaces: the entry carries the sub-errors without
    // a prefix, a type hint only for registered types, and non-object
    // items read `must be object`.
    let failing = [
        ("notype", "[{\"name\":\"n\",\"description\":\"d\"}]"),
        ("typed", "[{\"type\":\"pattern\",\"name\":\"n\"}]"),
        ("notobj", "[\"oops\"]"),
    ];
    for (index, (tag, batch)) in failing.iter().enumerate() {
        let (ours, theirs) = twin_seeded(&format!("reqfail{index}"), None, &[("alpha", "")], &[]);
        for dir in [&ours.0, &theirs.0] {
            std::fs::write(dir.join("batch.json"), batch).expect("batch file");
        }
        for args in [&["record", "alpha", "--batch", "batch.json"] as &[&str], &[
            "--json",
            "record",
            "alpha",
            "--batch",
            "batch.json",
        ]] {
            let our = run_in(&ours.0, Path::new(mulch_bin()), args);
            let their = run_in(&theirs.0, &ml, args);
            assert_eq!(our.code, their.code, "{tag} {args:?} exit");
            assert_eq!(
                normalize_dir(&our.stdout, &ours.0),
                normalize_dir(&their.stdout, &theirs.0),
                "{tag} {args:?} stdout"
            );
            assert_eq!(
                normalize_dir(&our.stderr, &ours.0),
                normalize_dir(&their.stderr, &theirs.0),
                "{tag} {args:?} stderr"
            );
        }
    }

    // `--files` on a type that does not declare it is DROPPED (the
    // reference collects required ∪ optional fields only)
    for (index, kind) in ["guide", "failure", "decision"].iter().enumerate() {
        let (ours, theirs) = twin_seeded(&format!("b8cafiles{index}"), None, &[("alpha", "")], &[]);
        let payload = match *kind {
            "guide" => vec!["guide", "--name", "g", "--description", "d"],
            "failure" => vec!["failure", "--description", "d", "--resolution", "r"],
            _ => vec!["decision", "--title", "t", "--rationale", "r"],
        };
        let mut args = vec!["record", "alpha", "--type"];
        args.extend(payload);
        args.extend(["--files", "a.ts"]);
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        let their = run_in(&theirs.0, &ml, &args);
        assert_eq!(our.code, their.code, "{kind} --files exit");
        assert_eq!(our.code, 0, "{kind} --files must succeed");
        assert_eq!(our.stdout, their.stdout, "{kind} --files stdout");
        let ours_store = read_store_file(&ours.0, "expertise/alpha.jsonl");
        let theirs_store = read_store_file(&theirs.0, "expertise/alpha.jsonl");
        assert!(
            !ours_store.contains("files"),
            "{kind} must drop --files: {ours_store}"
        );
        assert_eq!(normalize_line(&ours_store), normalize_line(&theirs_store));
    }

    // Flag-path json failure: the envelope carries the validate-style
    // text (not the plain rendering).
    let (ours, theirs) = twin_seeded("reqflagjson", None, &[("alpha", "")], &[]);
    let args = [
        "--json",
        "record",
        "alpha",
        "--type",
        "pattern",
        "--name",
        "n",
        "--description",
        "d",
        "--relates-to",
        "bogus",
    ];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, their.code);
    assert_eq!(our.stderr, their.stderr, "flag-path json envelope");
}

// ---- sprint 14 (mulch-b8ca): full subschema model ----

#[test]
fn schema_suberrors_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    // Each case was probe-fitted against ml 0.10.7: the branch reports
    // its FIRST failure in required -> additionalProperties ->
    // properties(declaration order) order.
    let cases: [(&str, &str); 9] = [
        (
            "additional",
            "{\"type\":\"guide\",\"name\":\"g\",\"description\":\"d\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"bogus\":1}\n",
        ),
        (
            "files-on-guide",
            "{\"type\":\"guide\",\"name\":\"g\",\"description\":\"d\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"files\":[]}\n",
        ),
        (
            "classification-enum",
            "{\"type\":\"guide\",\"name\":\"g\",\"description\":\"d\",\"classification\":\"weird\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\"}\n",
        ),
        (
            "description-type",
            "{\"type\":\"guide\",\"name\":\"g\",\"description\":123,\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\"}\n",
        ),
        (
            "name-null",
            "{\"type\":\"guide\",\"name\":null,\"description\":\"d\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\"}\n",
        ),
        (
            "id-pattern",
            "{\"type\":\"guide\",\"name\":\"g\",\"description\":\"d\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"id\":\"XX-bad\"}\n",
        ),
        (
            "tags-type",
            "{\"type\":\"guide\",\"name\":\"g\",\"description\":\"d\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"tags\":\"nope\"}\n",
        ),
        (
            "relates-to-item",
            "{\"type\":\"pattern\",\"name\":\"p\",\"description\":\"d\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"relates_to\":[\"ok:mx-abcd12\",\"bad\"]}\n",
        ),
        (
            "status-enum",
            "{\"type\":\"guide\",\"name\":\"g\",\"description\":\"d\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"status\":\"weird\"}\n",
        ),
    ];
    for (index, (tag, corpus)) in cases.iter().enumerate() {
        for args in [&["validate"] as &[&str], &["--json", "validate"]] {
            let (ours, theirs) = twin_seeded(&format!("b8ca{index}"), None, &[("d", *corpus)], &[]);
            let our = run_in(&ours.0, Path::new(mulch_bin()), args);
            let their = run_in(&theirs.0, &ml, args);
            assert_eq!(our.code, their.code, "{tag} {args:?} exit");
            assert_eq!(our.code, 1, "{tag} {args:?} must fail like the reference");
            assert_eq!(
                normalize_dir(&our.stdout, &ours.0),
                normalize_dir(&their.stdout, &theirs.0),
                "{tag} {args:?} stdout"
            );
            assert_eq!(
                normalize_dir(&our.stderr, &ours.0),
                normalize_dir(&their.stderr, &theirs.0),
                "{tag} {args:?} stderr"
            );
        }
    }
}

// ---- sprint 11 (mulch-ca49): batch summary surfaces + compact rewrite ----

// ---- sprint 36 (mulch-230b): unknown-type validate surfaces ----

/// The unknown-type store: a string-id unknown record, a schema-valid
/// pattern (full oneOf set — the fixture rule), an id-less unknown
/// record, and an empty-string-id unknown record (the id corners).
const UNK_TYPES_CORPUS: &str = concat!(
    r#"{"type":"zzzcustom","name":"wtf","id":"mx-bbbb02","content":"x","classification":"foundational","recorded_at":"2020-01-01T00:00:00Z"}"#,
    '\n',
    r#"{"type":"pattern","name":"ok","description":"d","classification":"foundational","recorded_at":"2020-01-01T00:00:00Z"}"#,
    '\n',
    r#"{"type":"wtf","content":"noid"}"#,
    '\n',
    r#"{"type":"wtfempty","id":"","content":"x"}"#,
    '\n',
);

#[test]
fn validate_unknown_type_message_and_id_corners_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_seeded("unk230v", None, &[("dev", UNK_TYPES_CORPUS)], &[]);

    // plain: summary on stdout, the targeted details on stderr, rc 1
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["validate"]);
    let their = run_in(&theirs.0, &ml, &["validate"]);
    assert_eq!(our.code, their.code, "plain exit");
    assert_eq!(our.code, 1, "unknown types fail validate");
    assert_eq!(our.stdout, their.stdout, "plain stdout");
    assert_eq!(our.stderr, their.stderr, "plain stderr");
    // the law itself: string id carries (id=…), the empty string
    // carries (id=), a missing id carries no part
    assert_eq!(
        our.stderr,
        concat!(
            "dev:1 - Unknown record type \"zzzcustom\" (id=mx-bbbb02). Register it under custom_types in mulch.config.yaml or remove the record.\n",
            "dev:3 - Unknown record type \"wtf\". Register it under custom_types in mulch.config.yaml or remove the record.\n",
            "dev:4 - Unknown record type \"wtfempty\" (id=). Register it under custom_types in mulch.config.yaml or remove the record.\n",
        )
    );
    assert_eq!(our.stdout, "4 records validated, 3 errors found\n");

    // json: the envelope carries the same messages in errors[]
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "validate"]);
    let their = run_in(&theirs.0, &ml, &["--json", "validate"]);
    assert_eq!(our.code, their.code, "json exit");
    assert_eq!(our.code, 1, "json envelope fails");
    assert_eq!(
        normalize(&our.stdout),
        normalize(&their.stdout),
        "json envelope"
    );
}

#[test]
fn validate_allow_unknown_types_skips_unknown_records_entirely() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_seeded("unk230a", None, &[("dev", UNK_TYPES_CORPUS)], &[]);

    // plain: rc 0, records still counted, zero errors, nothing on stderr
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "validate",
        "--allow-unknown-types",
    ]);
    let their = run_in(&theirs.0, &ml, &["validate", "--allow-unknown-types"]);
    assert_eq!(our.code, their.code, "plain exit");
    assert_eq!(our.code, 0, "the flag reads through unknown types");
    assert_eq!(our.stdout, their.stdout, "plain stdout");
    assert_eq!(our.stdout, "4 records validated, 0 errors found\n");
    assert_eq!(our.stderr, their.stderr, "plain stderr");

    // json: the success envelope, empty errors array
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json",
        "validate",
        "--allow-unknown-types",
    ]);
    let their = run_in(&theirs.0, &ml, &[
        "--json",
        "validate",
        "--allow-unknown-types",
    ]);
    assert_eq!(our.code, their.code, "json exit");
    assert_eq!(our.code, 0, "json envelope succeeds");
    assert_eq!(
        normalize(&our.stdout),
        normalize(&their.stdout),
        "json envelope"
    );
}

#[test]
fn validate_unknown_record_with_legacy_outcome_reports_only_unknown() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    // The unknown check runs FIRST and continues (reference
    // validate.ts): a legacy singular outcome on an unknown-type
    // record never reaches the legacy warning.
    let corpus = concat!(
        r#"{"type":"wtf","content":"x","outcome":{"status":"success"}}"#,
        '\n',
        r#"{"type":"pattern","name":"ok","description":"d","classification":"foundational","recorded_at":"2020-01-01T00:00:00Z"}"#,
        '\n',
    );
    let (ours, theirs) = twin_seeded("unk230l", None, &[("dev", corpus)], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["validate"]);
    let their = run_in(&theirs.0, &ml, &["validate"]);
    assert_eq!(our.code, their.code, "exit");
    assert_eq!(our.code, 1, "the unknown record still errors");
    assert_eq!(our.stdout, their.stdout, "stdout");
    assert_eq!(our.stderr, their.stderr, "stderr");
    assert_eq!(
        our.stderr,
        "dev:1 - Unknown record type \"wtf\". Register it under custom_types in mulch.config.yaml or remove the record.\n",
        "the unknown error is the record's ONLY finding"
    );
    assert_eq!(our.stdout, "2 records validated, 1 errors found\n");
}
