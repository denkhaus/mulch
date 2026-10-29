//! `mulch validate` + schema/parse-layer pins (ADR-0008 edition: OUR
//! byte pins only; literals frozen from the parity era's twin runs).
//!
//! The suberror table pins the full joined oneOf blob per case; the
//! common-fields and json-doctor arms pin exit codes plus structural
//! facts (the wording law is the same one the suberror table pins).

use std::path::Path;

use serde_json::json;

use super::support::*;

/// The joined-message law: plain stderr renders `domain:line - Schema
/// validation failed:` then one indented item per line; the json
/// envelope joins the same items with `; ` into `errors[0].message`.
fn schema_items(domain: &str, line: usize, items: &[&str]) -> (String, String) {
    let mut plain = format!("{domain}:{line} - Schema validation failed:\n");
    for item in items {
        plain.push_str("  ");
        plain.push_str(item);
        plain.push('\n');
    }
    let joined = items.join("; ");
    let message = format!("Schema validation failed: {joined}");
    (plain, message)
}

#[test]
fn validate_clean_and_broken_pin_their_bytes() {
    let ours = TempDir::new("val-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "dev",
        "--type",
        "pattern",
        "--name",
        "p1",
        "--description",
        "dp1",
    ]);

    // clean
    let clean = run_in(&ours.0, Path::new(mulch_bin()), &["validate"]);
    assert_eq!(clean.code, 0);
    assert_eq!(clean.stderr, "");
    assert_eq!(clean.stdout, "1 records validated, 0 errors found\n");

    // broken: an invalid convention line the writer rejects stays off
    // disk, appended garbage makes line 2 invalid
    let file = ours.0.join(".mulch/expertise/dev.jsonl");
    let mut text = std::fs::read_to_string(&file).expect("domain file");
    text.push_str("this line is not json\n");
    std::fs::write(&file, text).expect("append garbage");

    let broken = run_in(&ours.0, Path::new(mulch_bin()), &["validate"]);
    assert_eq!(broken.code, 1);
    assert_eq!(broken.stdout, "2 records validated, 1 errors found\n");
    assert_eq!(broken.stderr, "dev:2 - Invalid JSON: failed to parse\n");
    let json = run_in(&ours.0, Path::new(mulch_bin()), &["validate", "--json"]);
    assert_eq!(json.code, 1);
    assert_eq!(
        normalize(&json.stdout),
        "{\n  \"success\": false,\n  \"command\": \"validate\",\n  \"valid\": false,\n  \
         \"totalRecords\": 2,\n  \"totalErrors\": 1,\n  \"totalWarnings\": 0,\n  \"errors\": [\n    \
         {\n      \"domain\": \"dev\",\n      \"line\": 2,\n      \"message\": \"Invalid JSON: \
         failed to parse\"\n    }\n  ],\n  \"warnings\": []\n}\n"
    );
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

/// A pattern record missing its payload: the full oneOf blob, plain
/// and json, plus the doctor detail line.
#[test]
fn schema_invalid_strings_pin_the_full_blob() {
    let ours = TempDir::new("schemainv-ours");
    let record = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T10:00:00.000Z\",\"name\":\"x\"}";
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    std::fs::write(
        ours.0.join(".mulch/expertise/dev.jsonl"),
        format!("{record}\n"),
    )
    .expect("write record");
    let cfg = ours.0.join(".mulch/mulch.config.yaml");
    let text = std::fs::read_to_string(&cfg).expect("config");
    std::fs::write(cfg, text.replace("domains: {}", "domains:\n  dev: {}"))
        .expect("register domain");

    let items = [
        " must have required property 'content'",
        " must have required property 'description'",
        " must have required property 'description'",
        " must have required property 'title'",
        " must have required property 'description'",
        " must have required property 'description'",
        " must match exactly one schema in oneOf",
    ];
    let (plain_expected, message) = schema_items("dev", 1, &items);

    // validate plain: summary on stdout, details on stderr
    let plain = run_in(&ours.0, Path::new(mulch_bin()), &["validate"]);
    assert_eq!(plain.code, 1);
    assert_eq!(plain.stdout, "1 records validated, 1 errors found\n");
    assert_eq!(plain.stderr, plain_expected);

    // validate json: envelope on stdout
    let json = run_in(&ours.0, Path::new(mulch_bin()), &["validate", "--json"]);
    assert_eq!(json.code, 1);
    assert_eq!(
        normalize(&json.stdout),
        format!(
            "{{\n  \"success\": false,\n  \"command\": \"validate\",\n  \"valid\": false,\n  \
             \"totalRecords\": 1,\n  \"totalErrors\": 1,\n  \"totalWarnings\": 0,\n  \
             \"errors\": [\n    {{\n      \"domain\": \"dev\",\n      \"line\": 1,\n      \
             \"message\": \"{}\"\n    }}\n  ],\n  \"warnings\": []\n}}\n",
            message.replace('"', "\\\"")
        )
    );

    // doctor plain: the schema-validation detail line
    let doc = run_in(&ours.0, Path::new(mulch_bin()), &["doctor"]);
    assert!(
        doc.stdout
            .contains("must have required property 'description'")
    );
}

/// Blank lines keep physical line numbers: findings at lines 3 and 4,
/// not 2 and 3.
#[test]
fn blank_lines_keep_physical_line_numbers() {
    let ours = TempDir::new("blank-ours");
    let valid = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T10:00:00.000Z\",\"name\":\"p\",\"description\":\"d\"}";
    let invalid = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T10:00:00.000Z\",\"name\":\"x\"}";
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    std::fs::write(
        ours.0.join(".mulch/expertise/dev.jsonl"),
        format!("\n{valid}\n{invalid}\nnot json\n"),
    )
    .expect("write lines");
    let cfg = ours.0.join(".mulch/mulch.config.yaml");
    let text = std::fs::read_to_string(&cfg).expect("config");
    std::fs::write(cfg, text.replace("domains: {}", "domains:\n  dev: {}"))
        .expect("register domain");

    let run = run_in(&ours.0, Path::new(mulch_bin()), &["validate"]);
    assert_eq!(run.code, 1);
    assert_eq!(run.stdout, "3 records validated, 2 errors found\n");
    // physical addressing: findings at lines 3 and 4, not 2 and 3
    assert!(run.stderr.contains("dev:3 - Schema validation failed:"));
    assert!(run.stderr.contains("dev:4 - Invalid JSON"));
    assert!(
        run.stderr
            .ends_with("dev:4 - Invalid JSON: failed to parse\n")
    );
}

#[test]
fn unknown_record_type_errors_pin_the_registry_text() {
    let seed = format!("{ALPHA_ONE}\n{UNKNOWN_TYPE_LINE}\n");
    let expected = "Error: Unknown record type \"zzzcustom\" at <DIR>/.mulch/expertise/alpha.jsonl:2 (id=mx-zzz111). Register it under custom_types in mulch.config.yaml, remove the record, or pass --allow-unknown-types to bypass.\n";
    let commands: [&[&str]; 3] = [
        &["delete", "alpha", "mx-1bb21d"],
        &["delete-domain", "alpha", "--yes"],
        &["move", "alpha", "mx-1bb21d", "beta"],
    ];
    for (index, args) in commands.iter().enumerate() {
        let ours = seeded_store(
            &format!("unk{index}"),
            None,
            &[
                ("alpha", seed.as_str()),
                ("beta", format!("{BETA_ONE_LINE}\n").as_str()),
            ],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 1, "{args:?} aborts");
        assert_eq!(our.stdout, "", "{args:?} nothing on stdout");
        assert_eq!(normalize_dir(&our.stderr, &ours.0), expected, "our stderr");
        assert_eq!(read_store_file(&ours.0, "expertise/alpha.jsonl"), seed);
    }

    // Without an id the `(id=…)` part is omitted.
    let id_less = "{\"type\":\"zzzcustom\",\"name\":\"Odd\"}";
    let expected = "Error: Unknown record type \"zzzcustom\" at <DIR>/.mulch/expertise/alpha.jsonl:1. Register it under custom_types in mulch.config.yaml, remove the record, or pass --allow-unknown-types to bypass.\n";
    let ours = seeded_store("unknoid", None, &[("alpha", id_less)], &[]);
    let args = ["delete", "alpha", "mx-1bb21d"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(our.code, 1);
    assert_eq!(normalize_dir(&our.stderr, &ours.0), expected, "our stderr");

    // `--allow-unknown-types` reads through the unknown line; a KNOWN
    // record can then be removed / moved while the unknown record
    // survives (stores and messages frozen).
    for (index, (args, stdout, alpha_after, beta_after)) in [
        (
            &["delete", "alpha", "mx-1bb21d", "--allow-unknown-types"] as &[&str],
            "✓ Deleted pattern mx-1bb21d from alpha: Alpha One\n",
            format!("{UNKNOWN_TYPE_LINE}\n"),
            format!("{BETA_ONE_LINE}\n"),
        ),
        (
            &[
                "move",
                "alpha",
                "mx-1bb21d",
                "beta",
                "--allow-unknown-types",
            ],
            "✓ Moved pattern mx-1bb21d from alpha → beta: Alpha One\n",
            format!("{UNKNOWN_TYPE_LINE}\n"),
            format!("{BETA_ONE_LINE}\n{ALPHA_ONE}\n"),
        ),
        (
            &[
                "delete",
                "alpha",
                "--all-except",
                "mx-zzz111",
                "--allow-unknown-types",
            ],
            "Deleted pattern mx-1bb21d from alpha: Alpha One\n",
            format!("{UNKNOWN_TYPE_LINE}\n"),
            format!("{BETA_ONE_LINE}\n"),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let ours = seeded_store(
            &format!("unkallow{index}"),
            None,
            &[
                ("alpha", seed.as_str()),
                ("beta", format!("{BETA_ONE_LINE}\n").as_str()),
            ],
            &[],
        );
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 0, "{args:?}: {}", our.stderr);
        assert_eq!(our.stdout, stdout, "{args:?}");
        assert_eq!(
            read_store_file(&ours.0, "expertise/alpha.jsonl"),
            alpha_after
        );
        assert_eq!(read_store_file(&ours.0, "expertise/beta.jsonl"), beta_after);
    }
}

/// Missing common fields fail every surface (hand-written stores: the
/// `record` fixture would always carry classification + recorded_at).
/// The wording law is the suberror table's; these pin the codes, the
/// doctor failure line, and the batch enrichment behavior.
#[test]
fn required_common_fields_fail_every_surface() {
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
        let ours = seeded_store(&format!("req{index}"), None, &[("alpha", *corpus)], &[]);
        for args in [
            &["validate"] as &[&str],
            &["--json", "validate"],
            &["--json", "doctor"],
            &["doctor"],
        ] {
            let our = run_in(&ours.0, Path::new(mulch_bin()), args);
            assert_eq!(our.code, 1, "{tag} {args:?} must fail");
            if args.contains(&"doctor") && !args.contains(&"--json") {
                assert!(
                    our.stdout.contains("failed schema validation"),
                    "{tag} plain doctor reports the failure"
                );
            } else if args.contains(&"doctor") {
                // json doctor: the schema-validation check fails
                let value: serde_json::Value = serde_json::from_str(&our.stdout).expect("our json");
                let check = value["checks"]
                    .as_array()
                    .expect("checks")
                    .iter()
                    .find(|check| check["name"] == "schema-validation")
                    .expect("schema-validation check");
                assert_eq!(check["status"], json!("fail"), "{tag}");
                assert!(value["summary"]["fail"].as_u64().unwrap_or(0) >= 1, "{tag}");
            } else if args.contains(&"--json") {
                let value: serde_json::Value = serde_json::from_str(&our.stdout).expect("our json");
                assert_eq!(value["valid"], json!(false), "{tag}");
                assert_eq!(value["totalErrors"], json!(1), "{tag}");
            } else {
                assert!(
                    our.stderr.contains("alpha:1 - Schema validation failed:"),
                    "{tag} plain validate details: {}",
                    our.stderr
                );
            }
        }
    }

    // A batch record without the common fields is ENRICHED before
    // validation, so it is accepted.
    let batch = "[{\"type\":\"guide\",\"name\":\"g\",\"description\":\"gd\"}]";
    let ours = seeded_store("reqbatch", None, &[("alpha", "")], &[]);
    std::fs::write(ours.0.join("batch.json"), batch).expect("batch file");
    let args = ["record", "alpha", "--batch", "batch.json", "--json"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(our.code, 0, "{}", our.stderr);
    let store = read_store_file(&ours.0, "expertise/alpha.jsonl");
    assert!(
        store.contains("\"classification\":\"tactical\""),
        "batch enrichment fills classification: {store}"
    );

    // an explicit recorded_at/classification survive the enrichment
    let fixed = "2019-01-02T03:04:05.678Z";
    let batch_kept = format!(
        "[{{\"type\":\"guide\",\"name\":\"g2\",\"description\":\"gd2\",\"recorded_at\":\"{fixed}\",\"classification\":\"foundational\"}}]"
    );
    let ours = seeded_store("reqbatch2", None, &[("alpha", "")], &[]);
    std::fs::write(ours.0.join("batch.json"), &batch_kept).expect("batch file");
    let args = ["record", "alpha", "--batch", "batch.json", "--json"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(our.code, 0, "{}", our.stderr);
    let store = read_store_file(&ours.0, "expertise/alpha.jsonl");
    assert!(
        store.contains(fixed),
        "enrichment must keep the explicit recorded_at: {store}"
    );
    assert!(
        store.contains("\"classification\":\"foundational\""),
        "enrichment must keep the explicit classification: {store}"
    );

    // Batch FAILURE surfaces: the entry carries the sub-errors
    // without a prefix, a type hint only for registered types, and
    // non-object items read `must be object` (frozen).
    let failing = [
        (
            "notype",
            "[{\"name\":\"n\",\"description\":\"d\"}]",
            concat!(
                "Validation errors:\n",
                "  Record 0:  must have required property 'type';  must have required property \
                 'type';  must have required property 'type';  must have required property \
                 'type';  must have required property 'type';  must have required property \
                 'type';  must match exactly one schema in oneOf\n",
            ),
        ),
        (
            "typed",
            "[{\"type\":\"pattern\",\"name\":\"n\"}]",
            concat!(
                "Validation errors:\n",
                "  Record 0:  must have required property 'content';  must have required \
                 property 'description';  must have required property 'description';  must \
                 have required property 'title';  must have required property 'description';  \
                 must have required property 'description';  must match exactly one schema in \
                 oneOf. Hint: pattern records require: name, description\n",
            ),
        ),
        (
            "notobj",
            "[\"oops\"]",
            "Validation errors:\n  Record 0:  must be object\n",
        ),
    ];
    for (index, (tag, batch, expected)) in failing.iter().enumerate() {
        let ours = seeded_store(&format!("reqfail{index}"), None, &[("alpha", "")], &[]);
        std::fs::write(ours.0.join("batch.json"), batch).expect("batch file");
        let our = run_in(&ours.0, Path::new(mulch_bin()), &[
            "record",
            "alpha",
            "--batch",
            "batch.json",
        ]);
        assert_eq!(our.code, 1, "{tag}");
        assert_eq!(our.stderr, *expected, "{tag}");
        let json = run_in(&ours.0, Path::new(mulch_bin()), &[
            "--json",
            "record",
            "alpha",
            "--batch",
            "batch.json",
        ]);
        assert_eq!(json.code, 1, "{tag}");
        assert!(
            json.stderr.contains("Validation errors: Record 0:"),
            "{tag}: {}",
            json.stderr
        );
    }

    // `--files` on a type that does not declare it is DROPPED (the
    // writer collects required ∪ optional fields only)
    for (index, kind) in ["guide", "failure", "decision"].iter().enumerate() {
        let ours = seeded_store(&format!("b8cafiles{index}"), None, &[("alpha", "")], &[]);
        let payload = match *kind {
            "guide" => vec!["guide", "--name", "g", "--description", "d"],
            "failure" => vec!["failure", "--description", "d", "--resolution", "r"],
            _ => vec!["decision", "--title", "t", "--rationale", "r"],
        };
        let mut args = vec!["record", "alpha", "--type"];
        args.extend(payload);
        args.extend(["--files", "a.ts"]);
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(our.code, 0, "{kind} --files must succeed");
        let store = read_store_file(&ours.0, "expertise/alpha.jsonl");
        assert!(
            !store.contains("files"),
            "{kind} must drop --files: {store}"
        );
    }

    // Flag-path json failure: the envelope carries the validate-style
    // text (not the plain rendering).
    let ours = seeded_store("reqflagjson", None, &[("alpha", "")], &[]);
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
    assert_eq!(our.code, 1);
    assert!(
        normalize(&our.stderr).contains("/relates_to/0 must match pattern"),
        "{}",
        our.stderr
    );
}

// ---- sprint 14 (mulch-b8ca): full subschema model ----

/// The full subschema table: each case's joined oneOf blob, frozen.
#[test]
fn schema_suberrors_pin_the_full_blobs() {
    let cases: [(&str, &str, [&str; 7]); 9] = [
        (
            "additional",
            "{\"type\":\"guide\",\"name\":\"g\",\"description\":\"d\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"bogus\":1}\n",
            [
                " must have required property 'content'",
                " must NOT have additional properties",
                " must have required property 'resolution'",
                " must have required property 'title'",
                " must NOT have additional properties",
                " must NOT have additional properties",
                " must match exactly one schema in oneOf",
            ],
        ),
        (
            "files-on-guide",
            "{\"type\":\"guide\",\"name\":\"g\",\"description\":\"d\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"files\":[]}\n",
            [
                " must have required property 'content'",
                "/type must be equal to constant",
                " must have required property 'resolution'",
                " must have required property 'title'",
                "/type must be equal to constant",
                " must NOT have additional properties",
                " must match exactly one schema in oneOf",
            ],
        ),
        (
            "classification-enum",
            "{\"type\":\"guide\",\"name\":\"g\",\"description\":\"d\",\"classification\":\"weird\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\"}\n",
            [
                " must have required property 'content'",
                "/classification must be equal to one of the allowed values",
                " must have required property 'resolution'",
                " must have required property 'title'",
                "/classification must be equal to one of the allowed values",
                "/classification must be equal to one of the allowed values",
                " must match exactly one schema in oneOf",
            ],
        ),
        (
            "description-type",
            "{\"type\":\"guide\",\"name\":\"g\",\"description\":123,\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\"}\n",
            [
                " must have required property 'content'",
                "/type must be equal to constant",
                " must have required property 'resolution'",
                " must have required property 'title'",
                "/type must be equal to constant",
                "/description must be string",
                " must match exactly one schema in oneOf",
            ],
        ),
        (
            "name-null",
            "{\"type\":\"guide\",\"name\":null,\"description\":\"d\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\"}\n",
            [
                " must have required property 'content'",
                "/type must be equal to constant",
                " must have required property 'resolution'",
                " must have required property 'title'",
                "/type must be equal to constant",
                "/name must be string",
                " must match exactly one schema in oneOf",
            ],
        ),
        (
            "id-pattern",
            "{\"type\":\"guide\",\"name\":\"g\",\"description\":\"d\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"id\":\"XX-bad\"}\n",
            [
                " must have required property 'content'",
                "/id must match pattern \"^mx-[0-9a-f]{4,8}$\"",
                " must have required property 'resolution'",
                " must have required property 'title'",
                "/id must match pattern \"^mx-[0-9a-f]{4,8}$\"",
                "/id must match pattern \"^mx-[0-9a-f]{4,8}$\"",
                " must match exactly one schema in oneOf",
            ],
        ),
        (
            "tags-type",
            "{\"type\":\"guide\",\"name\":\"g\",\"description\":\"d\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"tags\":\"nope\"}\n",
            [
                " must have required property 'content'",
                "/tags must be array",
                " must have required property 'resolution'",
                " must have required property 'title'",
                "/tags must be array",
                "/tags must be array",
                " must match exactly one schema in oneOf",
            ],
        ),
        (
            "relates-to-item",
            "{\"type\":\"pattern\",\"name\":\"p\",\"description\":\"d\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"relates_to\":[\"ok:mx-abcd12\",\"bad\"]}\n",
            [
                " must have required property 'content'",
                "/relates_to/1 must match pattern \"^([a-z0-9-]+:)?mx-[0-9a-f]{4,8}$\"",
                " must have required property 'resolution'",
                " must have required property 'title'",
                "/relates_to/1 must match pattern \"^([a-z0-9-]+:)?mx-[0-9a-f]{4,8}$\"",
                "/relates_to/1 must match pattern \"^([a-z0-9-]+:)?mx-[0-9a-f]{4,8}$\"",
                " must match exactly one schema in oneOf",
            ],
        ),
        (
            "status-enum",
            "{\"type\":\"guide\",\"name\":\"g\",\"description\":\"d\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"status\":\"weird\"}\n",
            [
                " must have required property 'content'",
                "/status must be equal to one of the allowed values",
                " must have required property 'resolution'",
                " must have required property 'title'",
                "/status must be equal to one of the allowed values",
                "/status must be equal to one of the allowed values",
                " must match exactly one schema in oneOf",
            ],
        ),
    ];
    for (index, (tag, corpus, items)) in cases.iter().enumerate() {
        let (plain_expected, message) = schema_items("d", 1, items);
        for args in [&["validate"] as &[&str], &["--json", "validate"]] {
            let ours = seeded_store(&format!("b8ca{index}"), None, &[("d", corpus)], &[]);
            let our = run_in(&ours.0, Path::new(mulch_bin()), args);
            assert_eq!(our.code, 1, "{tag} {args:?} must fail");
            if args[0] == "--json" {
                let value: serde_json::Value = serde_json::from_str(&our.stdout).expect("json");
                assert_eq!(value["errors"][0]["message"], json!(message), "{tag}");
            } else {
                assert_eq!(our.stdout, "1 records validated, 1 errors found\n", "{tag}");
                assert_eq!(our.stderr, plain_expected, "{tag}");
            }
        }
    }
}

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
fn validate_unknown_type_message_and_id_corners() {
    let ours = seeded_store("unk230v", None, &[("dev", UNK_TYPES_CORPUS)], &[]);

    // plain: summary on stdout, the targeted details on stderr, rc 1
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["validate"]);
    assert_eq!(our.code, 1, "unknown types fail validate");
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
    assert_eq!(our.code, 1, "json envelope fails");
    let value: serde_json::Value = serde_json::from_str(&our.stdout).expect("json");
    assert_eq!(value["totalErrors"], json!(3));
    assert_eq!(value["totalRecords"], json!(4));
    assert_eq!(
        value["errors"][0]["message"],
        "Unknown record type \"zzzcustom\" (id=mx-bbbb02). Register it under custom_types in \
         mulch.config.yaml or remove the record."
    );
}

#[test]
fn validate_allow_unknown_types_skips_unknown_records_entirely() {
    let ours = seeded_store("unk230a", None, &[("dev", UNK_TYPES_CORPUS)], &[]);

    // plain: rc 0, records still counted, zero errors, nothing on stderr
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "validate",
        "--allow-unknown-types",
    ]);
    assert_eq!(our.code, 0, "the flag reads through unknown types");
    assert_eq!(our.stdout, "4 records validated, 0 errors found\n");
    assert_eq!(our.stderr, "");

    // json: the success envelope, empty errors array
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json",
        "validate",
        "--allow-unknown-types",
    ]);
    assert_eq!(our.code, 0, "json envelope succeeds");
    let value: serde_json::Value = serde_json::from_str(&our.stdout).expect("json");
    assert_eq!(value["valid"], json!(true));
    assert_eq!(value["errors"], serde_json::json!([]));
}

#[test]
fn validate_unknown_record_with_legacy_outcome_reports_only_unknown() {
    // The unknown check runs FIRST and continues: a legacy singular
    // outcome on an unknown-type record never reaches the legacy
    // warning.
    let corpus = concat!(
        r#"{"type":"wtf","content":"x","outcome":{"status":"success"}}"#,
        '\n',
        r#"{"type":"pattern","name":"ok","description":"d","classification":"foundational","recorded_at":"2020-01-01T00:00:00Z"}"#,
        '\n',
    );
    let ours = seeded_store("unk230l", None, &[("dev", corpus)], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["validate"]);
    assert_eq!(our.code, 1, "the unknown record still errors");
    assert_eq!(
        our.stderr,
        "dev:1 - Unknown record type \"wtf\". Register it under custom_types in mulch.config.yaml or remove the record.\n",
        "the unknown error is the record's ONLY finding"
    );
    assert_eq!(our.stdout, "2 records validated, 1 errors found\n");
}
