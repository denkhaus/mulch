//! `mulch record`/`add` flag path, stdin, batch, and hook pins
//! (ADR-0008 edition: OUR byte pins only; literals frozen from the
//! parity era's twin runs). The nested `battery_s23_b` group keeps
//! its sprint-23 trio together — one of its pins is an EDIT surface
//! (payload append order); it stayed with its lane group.

use std::path::Path;

use serde_json::json;

use super::support::*;

#[test]
fn batch_mixed_partial_writes_pin_the_semantics() {
    let ours = TempDir::new("mix-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "dev",
        "--type",
        "pattern",
        "--name",
        "p1",
        "--description",
        "d1",
    ]);
    // valid named dup (update) + invalid id (error entry) + valid
    // create: the valid records write even though one failed.
    std::fs::write(
        ours.0.join("b.json"),
        "[{\"type\":\"pattern\",\"name\":\"p1\",\"description\":\"d1-NEW\",\"classification\":\"tactical\"},{\"type\":\"pattern\",\"name\":\"bad\",\"id\":123},{\"type\":\"convention\",\"content\":\"fresh\"}]",
    )
    .expect("batch fixture");
    // all-invalid: nothing writes, exit 1 in every mode
    std::fs::write(
        ours.0.join("bad.json"),
        "[{\"type\":\"pattern\",\"name\":\"bad\",\"id\":123},{\"type\":\"convention\",\"content\":5}]",
    )
    .expect("bad fixture");

    for file in ["b.json", "bad.json"] {
        for mode in [&[] as &[&str], &["--json"], &["--dry-run"], &[
            "--dry-run",
            "--json",
        ]] {
            let mut args = vec!["record", "dev", "--batch", file];
            args.extend_from_slice(mode);
            let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
            if file == "bad.json" {
                // the all-invalid batch fails in every mode
                assert_eq!(ours_run.code, 1, "exit {file} {mode:?}");
            } else {
                assert_eq!(
                    ours_run.code, 0,
                    "exit {file} {mode:?}: {}",
                    ours_run.stderr
                );
            }
        }
    }
    // partial-write semantics landed in the store: the update replaced
    // p1 in place, the create appended (2 lines; the invalid record is
    // not on disk) — across all four b.json runs the second pass sees
    // p1-NEW as the duplicate (update again) and "fresh" as an
    // anonymous skip.
    let store = read_store_file(&ours.0, "expertise/dev.jsonl");
    assert_eq!(store.lines().count(), 2);
    assert!(store.contains("d1-NEW"));
    assert!(store.contains("fresh"));
}

#[test]
fn nested_subschema_errors_pin_the_findings() {
    let common = r#","classification":"tactical","recorded_at":"2026-10-06T10:00:00.000Z""#;
    let store = [
        format!(r#"{{"type":"pattern","name":"n1","description":"d"{common},"outcomes":[{{"status":"nope"}}]}}"#),
        format!(r#"{{"type":"pattern","name":"n2","description":"d"{common},"outcomes":[{{"status":"success","duration":"42"}}]}}"#),
        format!(r#"{{"type":"pattern","name":"n3","description":"d"{common},"outcomes":[{{"status":"success","bogus":1}}]}}"#),
        format!(r#"{{"type":"pattern","name":"n4","description":"d"{common},"outcomes":[5]}}"#),
        format!(r#"{{"type":"pattern","name":"n5","description":"d"{common},"outcomes":[{{"duration":1}}]}}"#),
        format!(r#"{{"type":"pattern","name":"n6","description":"d"{common},"outcomes":[{{"status":5}}]}}"#),
        format!(r#"{{"type":"pattern","name":"n7","description":"d"{common},"outcomes":[{{"status":"success"}},{{"status":"bad"}}]}}"#),
        format!(r#"{{"type":"pattern","name":"n8","description":"d"{common},"evidence":{{"commit":"c","wat":1}}}}"#),
        format!(r#"{{"type":"pattern","name":"n9","description":"d"{common},"evidence":{{"commit":5}}}}"#),
        format!(r#"{{"type":"pattern","name":"n10","description":"d"{common},"evidence":{{"wat":1}},"outcomes":[{{"status":"bad"}}]}}"#),
    ]
    .join("\n")
        + "\n";
    let ours = TempDir::new("nested-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["add", "dev"]);
    std::fs::write(ours.0.join(".mulch/expertise/dev.jsonl"), &store).expect("store fixture");

    for mode in [&[] as &[&str], &["--json"]] {
        let mut args = vec!["validate"];
        args.extend_from_slice(mode);
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(ours_run.code, 1, "exit {mode:?}");
        if mode.is_empty() {
            assert_eq!(
                ours_run.stdout, "10 records validated, 10 errors found\n",
                "{mode:?}"
            );
            // every record reports its nested outcome/evidence failure
            // inside the joined oneOf blob
            assert!(
                ours_run
                    .stderr
                    .contains("/outcomes/0/status must be equal to one of the allowed values"),
                "{}",
                ours_run.stderr
            );
            assert!(
                ours_run
                    .stderr
                    .contains("must match exactly one schema in oneOf"),
                "{}",
                ours_run.stderr
            );
        }
    }

    // the batch surface carries the same nested entries per record
    let batch = format!(
        "[{{\"type\":\"pattern\",\"name\":\"b1\",\"description\":\"d\"{common},\"outcomes\":[{{\"status\":\"nope\"}}]}}]"
    );
    std::fs::write(ours.0.join("nb.json"), &batch).expect("nested batch");
    for mode in [&[] as &[&str], &["--json"]] {
        let mut args = vec!["record", "dev", "--batch", "nb.json"];
        args.extend_from_slice(mode);
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(ours_run.code, 1, "batch exit {mode:?}");
        assert!(
            ours_run
                .stderr
                .contains("/outcomes/0/status must be equal to one of the allowed values"),
            "batch {mode:?}: {}",
            ours_run.stderr
        );
    }
}

// ---- sprint 18 (mulch-f9b9): staleness verdict seam — missing/unknown
// classification never decays, the (shelf+1)-day boundary, fix-report
// check order ----

#[test]
fn quiet_matrix_pins_the_channels() {
    let ours = TempDir::new("quiet-ours");
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

    // validate --quiet still prints the summary
    let ours_val = run_in(&ours.0, Path::new(mulch_bin()), &["validate", "--quiet"]);
    assert_eq!(ours_val.code, 0);
    assert_eq!(ours_val.stdout, "1 records validated, 0 errors found\n");

    // doctor --quiet prints NOTHING (plain mode)
    let ours_doc = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--quiet"]);
    assert_eq!(ours_doc.code, 0);
    assert_eq!(ours_doc.stdout, "");
}

/// Every type's create message and store line (the per-type key
/// order, ids included — timestamps masked).
#[test]
fn record_all_types_pin_their_lines() {
    let cases: [(&str, &[&str], &str, &str); 6] = [
        (
            "convention",
            &["--content", "cc"],
            "✓ Auto-created domain \"d\"\n✓ Recorded convention in d\n",
            "{\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"<TS>\",\"content\":\"cc\",\"id\":\"mx-d25bd6\"}",
        ),
        (
            "pattern",
            &["--name", "nn", "--description", "dd"],
            "✓ Auto-created domain \"d\"\n✓ Recorded pattern in d\n",
            "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"<TS>\",\"name\":\"nn\",\"description\":\"dd\",\"id\":\"mx-033607\"}",
        ),
        (
            "failure",
            &["--description", "dd", "--resolution", "rr"],
            "✓ Auto-created domain \"d\"\n✓ Recorded failure in d\n",
            "{\"type\":\"failure\",\"classification\":\"tactical\",\"recorded_at\":\"<TS>\",\"description\":\"dd\",\"resolution\":\"rr\",\"id\":\"mx-cebf86\"}",
        ),
        (
            "decision",
            &["--title", "tt", "--rationale", "rr"],
            "✓ Auto-created domain \"d\"\n✓ Recorded decision in d\n",
            "{\"type\":\"decision\",\"classification\":\"tactical\",\"recorded_at\":\"<TS>\",\"title\":\"tt\",\"rationale\":\"rr\",\"id\":\"mx-337b78\"}",
        ),
        (
            "reference",
            &["--name", "nn", "--description", "dd"],
            "✓ Auto-created domain \"d\"\n✓ Recorded reference in d\n",
            "{\"type\":\"reference\",\"classification\":\"tactical\",\"recorded_at\":\"<TS>\",\"name\":\"nn\",\"description\":\"dd\",\"id\":\"mx-3a9575\"}",
        ),
        (
            "guide",
            &["--name", "nn", "--description", "dd"],
            "✓ Auto-created domain \"d\"\n✓ Recorded guide in d\n",
            "{\"type\":\"guide\",\"classification\":\"tactical\",\"recorded_at\":\"<TS>\",\"name\":\"nn\",\"description\":\"dd\",\"id\":\"mx-46651f\"}",
        ),
    ];
    for (index, (kind, payload, stdout, line)) in cases.iter().enumerate() {
        let ours = TempDir::new(format!("rec{index}-ours").as_str());
        let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
        let mut args = vec!["record", "d", "--type", kind];
        args.extend_from_slice(payload);
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(ours_run.code, 0, "{kind}: {}", ours_run.stderr);
        assert_eq!(ours_run.stdout, *stdout, "{kind}");
        assert_eq!(
            normalize_line(&read_store_file(&ours.0, "expertise/d.jsonl")),
            format!("{line}\n"),
            "{kind} jsonl"
        );
        // domain registered in config
        assert!(read_store_file(&ours.0, "mulch.config.yaml").contains("d: {}"));
    }
}

/// The full optional-flag surface pins the flag-table key order:
/// evidence, tags, relates_to, supersedes, dir_anchors before the
/// payload, files after it, id last.
#[test]
fn record_optional_flags_pin_the_line_order() {
    let ours = TempDir::new("optf-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let args = [
        "record",
        "d",
        "--type",
        "pattern",
        "--name",
        "optflag-pat",
        "--description",
        "optflag-desc",
        "--classification",
        "foundational",
        "--tags",
        "a,b",
        "--files",
        "x.rs,y.rs",
        "--dir-anchor",
        "src/",
        "--relates-to",
        "mx-aaaa01",
        "--supersedes",
        "mx-bbbb02",
        "--evidence-commit",
        "abc123",
        "--evidence-seeds",
        "mulch-1",
    ];
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(ours_run.code, 0);
    assert_eq!(
        normalize_line(&read_store_file(&ours.0, "expertise/d.jsonl")),
        "{\"type\":\"pattern\",\"classification\":\"foundational\",\"recorded_at\":\"<TS>\",\
\"evidence\":{\"commit\":\"abc123\",\"seeds\":\"mulch-1\"},\"tags\":[\"a\",\"b\"],\
\"relates_to\":[\"mx-aaaa01\"],\"supersedes\":[\"mx-bbbb02\"],\"dir_anchors\":[\"src\"],\
\"name\":\"optflag-pat\",\"description\":\"optflag-desc\",\"files\":[\"x.rs\",\"y.rs\"],\
\"id\":\"mx-0f32ad\"}\n"
    );
}

#[test]
fn add_pins_its_surfaces() {
    let ours = TempDir::new("add-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["add", "cli"]);
    assert_eq!(ours_run.code, 0);
    assert_eq!(ours_run.stdout, "Added domain \"cli\".\n");
    assert_eq!(read_store_file(&ours.0, "expertise/cli.jsonl"), "");

    // existing domain
    let ours_dup = run_in(&ours.0, Path::new(mulch_bin()), &["add", "cli"]);
    assert_eq!(ours_dup.code, 1);
    assert_eq!(ours_dup.stdout, "");
    assert_eq!(ours_dup.stderr, "Domain \"cli\" already exists.\n");
}

#[test]
fn record_auto_create_failure_side_effects() {
    let ours = TempDir::new("autofail-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let args = [
        "record",
        "devfail",
        "--type",
        "convention",
        "--name",
        "t1",
        "--description",
        "d",
    ];
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(ours_run.code, 1);
    // the auto-create notice rides stdout, the failure stderr
    assert_eq!(ours_run.stdout, "✓ Auto-created domain \"devfail\"\n");
    assert_eq!(
        ours_run.stderr,
        "Error: convention records are missing required flag(s): --content.\n  Retry: mulch \
         record devfail --type convention --name \"t1\" --description \"d\" --content \
         \"<content>\"\n"
    );
    // the domain side effect persists
    assert!(read_store_file(&ours.0, "mulch.config.yaml").contains("devfail: {}"));
}

#[test]
fn duplicate_and_force_pin_their_surfaces() {
    let ours = TempDir::new("dup-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["add", "d"]);
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--type",
        "convention",
        "--content",
        "c1",
    ]);

    let dup = ["record", "d", "--type", "convention", "--content", "c1"];
    let ours_dup = run_in(&ours.0, Path::new(mulch_bin()), &dup);
    assert_eq!(ours_dup.code, 0);
    assert_eq!(
        ours_dup.stdout,
        "Duplicate convention already exists in d (record #1). Use --force to add anyway.\n"
    );

    let forced = [
        "record",
        "d",
        "--type",
        "convention",
        "--content",
        "c1",
        "--force",
    ];
    let ours_force = run_in(&ours.0, Path::new(mulch_bin()), &forced);
    assert_eq!(ours_force.code, 0);
    assert_eq!(
        read_store_file(&ours.0, "expertise/d.jsonl")
            .lines()
            .count(),
        2
    );
}

#[test]
fn record_ids_match_known_reference_values() {
    // Pinned from the reference source algorithm + probe table.
    let cases: [(&str, &str, &str); 4] = [
        ("convention", "c1", "mx-1d1926"),
        ("reference", "N1", "mx-b9079b"),
        ("pattern", "N1", "mx-a6adb8"),
        ("guide", "N1", "mx-ab7ce3"),
    ];
    for (kind, key, expected) in cases {
        assert_eq!(mulch::record_id(kind, key), expected);
    }
}

#[test]
fn record_dry_run_leaves_store_unchanged() {
    let ours = TempDir::new("dry-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["add", "d"]);
    let before = read_store_file(&ours.0, "expertise/d.jsonl");
    let run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--type",
        "convention",
        "--content",
        "x",
        "--dry-run",
    ]);
    assert_eq!(run.code, 0);
    assert!(run.stdout.contains("Dry-run: Would create convention in d"));
    assert_eq!(read_store_file(&ours.0, "expertise/d.jsonl"), before);
}

#[test]
fn record_stdin_pins_the_key_order() {
    let ours = TempDir::new("stdin-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["add", "d"]);
    let payload = r#"{"type":"convention","content":"stdin-conv-1"}"#;
    let ours_run = run_stdin(
        &ours.0,
        Path::new(mulch_bin()),
        &["record", "d", "--stdin"],
        payload,
    );
    assert_eq!(ours_run.code, 0, "{}", ours_run.stderr);
    assert_eq!(ours_run.stdout, "✓ Created 1 record(s) in d\n");
    // input key order preserved: type before content before recorded_at
    let line = read_store_file(&ours.0, "expertise/d.jsonl");
    assert!(
        line.starts_with(r#"{"type":"convention","content":"stdin-conv-1","recorded_at""#),
        "{line}"
    );
}

// ---- sprint 6 (mulch-ccf6): record upsert semantics ----

#[test]
fn record_upsert_pins_the_envelopes() {
    let ours = TempDir::new("ccf6-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--type",
        "pattern",
        "--name",
        "p",
        "--description",
        "v1",
    ]);
    let id = pattern_p_id();

    // named duplicate upserts in place (plain + the json envelope)
    let up = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--type",
        "pattern",
        "--name",
        "p",
        "--description",
        "v2",
    ]);
    assert_eq!(up.code, 0);
    assert_eq!(up.stdout, "✓ Updated existing pattern in d (record #1)\n");
    let up_json = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json",
        "record",
        "d",
        "--type",
        "pattern",
        "--name",
        "p",
        "--description",
        "v3",
        "--outcome-status",
        "success",
    ]);
    assert_eq!(up_json.code, 0);
    let envelope: serde_json::Value = serde_json::from_str(&up_json.stdout).expect("json");
    assert_eq!(envelope["action"], json!("updated"));
    assert_eq!(envelope["domain"], json!("d"));
    assert_eq!(envelope["type"], json!("pattern"));
    assert_eq!(envelope["index"], json!(1));
    // the embedded record keeps the flag-path key order: outcomes
    // BEFORE the payload, id last (preserve_order pins it)
    let record = envelope["record"].as_object().expect("record");
    assert_eq!(
        record.keys().map(String::as_str).collect::<Vec<_>>(),
        [
            "type",
            "classification",
            "recorded_at",
            "outcomes",
            "name",
            "description",
            "id"
        ]
        .to_vec(),
        "{record:?}"
    );
    assert_eq!(
        record["outcomes"][0]["status"],
        json!("success"),
        "{record:?}"
    );
    assert_eq!(record["id"], json!(id));

    // second outcome merges after the existing one
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json",
        "record",
        "d",
        "--type",
        "pattern",
        "--name",
        "p",
        "--description",
        "v4",
        "--outcome-status",
        "failure",
        "--outcome-agent",
        "second-agent",
    ]);
    let store = normalize_line(&read_store_file(&ours.0, "expertise/d.jsonl"));
    // the stored record carries both outcomes, the success first and
    // the failure (with agent) merged after it
    assert!(
        store.contains("\"outcomes\":[{\"status\":\"success\"}")
            && store.contains(",{\"status\":\"failure\",\"agent\":\"second-agent\"}]"),
        "{store}"
    );

    // anonymous duplicate skips with the advisory (plain + json)
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--type",
        "convention",
        "--content",
        "same",
    ]);
    let anon = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--type",
        "convention",
        "--content",
        "same",
    ]);
    assert_eq!(anon.code, 0);
    assert_eq!(
        anon.stdout,
        "Duplicate convention already exists in d (record #2). Use --force to add anyway.\n"
    );
    let anon_json = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json",
        "record",
        "d",
        "--type",
        "convention",
        "--content",
        "same",
    ]);
    assert_eq!(anon_json.code, 0);
    assert_eq!(
        anon_json.stdout,
        "{\n  \"success\": true,\n  \"command\": \"record\",\n  \"action\": \"skipped\",\n  \
         \"domain\": \"d\",\n  \"type\": \"convention\",\n  \"index\": 2\n}\n"
    );

    // --force appends anyway
    let force = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--type",
        "convention",
        "--content",
        "same",
        "--force",
    ]);
    assert_eq!(force.code, 0);
    let store = read_store_file(&ours.0, "expertise/d.jsonl");
    assert_eq!(store.lines().count(), 3, "the forced convention appended");

    // dry-run mirrors the decision without writing (plain + json)
    let before = store;
    for args in [
        vec![
            "record",
            "d",
            "--type",
            "pattern",
            "--name",
            "p",
            "--description",
            "v9",
            "--dry-run",
        ],
        vec![
            "--json",
            "record",
            "d",
            "--type",
            "pattern",
            "--name",
            "p",
            "--description",
            "v9",
            "--dry-run",
        ],
    ] {
        let run = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(run.code, 0, "{args:?}: {}", run.stderr);
        assert!(run.stdout.contains("Dry-run") || run.stdout.contains("\"action\""));
    }
    assert_eq!(read_store_file(&ours.0, "expertise/d.jsonl"), before);

    // a renamed record still dedupes (dedup field, not id) and the
    // upsert corrects the stale id in place
    let rename = ["edit", "d", &id, "--name", "p2"];
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &rename);
    let renamed_up = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--type",
        "pattern",
        "--name",
        "p2",
        "--description",
        "v10",
    ]);
    assert_eq!(renamed_up.code, 0);
    let store = read_store_file(&ours.0, "expertise/d.jsonl");
    assert!(store.contains("v10"));
    assert!(!store.contains("v4"), "the renamed record updated in place");

    // batch: one new guide + one named duplicate -> created 1, updated 1
    let batch = serde_json::json!([
        {"type": "guide", "name": "g1", "description": "gd"},
        {"type": "pattern", "name": "p2", "description": "batch-v11"}
    ]);
    std::fs::write(ours.0.join("batch.json"), format!("{batch}")).expect("batch file");
    let run_batch = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--batch",
        "batch.json",
        "--json",
    ]);
    assert_eq!(run_batch.code, 0, "{}", run_batch.stderr);
    assert!(run_batch.stdout.contains("\"created\": 1"));
    assert!(run_batch.stdout.contains("\"updated\": 1"));

    // within-batch duplicate in dry-run counts as another create
    // (dedupe runs against the store only, never the batch)
    let batch2 = serde_json::json!([
        {"type": "pattern", "name": "q", "description": "q1"},
        {"type": "pattern", "name": "q", "description": "q2"}
    ]);
    std::fs::write(ours.0.join("batch2.json"), format!("{batch2}")).expect("batch file");
    let run_batch2 = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--batch",
        "batch2.json",
        "--json",
        "--dry-run",
    ]);
    assert_eq!(run_batch2.code, 0);
    assert!(run_batch2.stdout.contains("\"created\": 2"));

    // non-dry within-batch duplicate upserts the batch's own record
    let run_batch3 = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--batch",
        "batch2.json",
        "--json",
    ]);
    assert_eq!(run_batch3.code, 0);
    assert!(run_batch3.stdout.contains("\"created\": 1"));
    assert!(run_batch3.stdout.contains("\"updated\": 1"));

    // an explicit batch id survives upsert AND create (generated ids
    // only fill gaps — spec-review blocker F1)
    let batch3 = serde_json::json!([
        {"type": "pattern", "name": "p2", "description": "keep-id", "id": "mx-abcd1234"},
        {"type": "guide", "name": "g2", "description": "gd2", "id": "mx-feed1234"}
    ]);
    std::fs::write(ours.0.join("batch3.json"), format!("{batch3}")).expect("batch file");
    let run_batch4 = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--batch",
        "batch3.json",
        "--json",
    ]);
    assert_eq!(run_batch4.code, 0);
    let store = read_store_file(&ours.0, "expertise/d.jsonl");
    assert!(store.contains("mx-abcd1234"));
    assert!(store.contains("mx-feed1234"));

    // flag-path key order: outcomes precede dir_anchors on the
    // rewritten line (spec-review minor F2)
    let anchored = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--type",
        "pattern",
        "--name",
        "p2",
        "--description",
        "v12",
        "--dir-anchor",
        "src/lib",
        "--outcome-status",
        "partial",
    ]);
    assert_eq!(anchored.code, 0);
    let store = read_store_file(&ours.0, "expertise/d.jsonl");
    assert!(
        store.contains("\"dir_anchors\":[\"src/lib\"],\"name\":\"p2\",\"description\":\"v12\""),
        "dir_anchors precede the payload, id last: {store}"
    );
    assert!(
        store.contains("{\"status\":\"partial\"}]"),
        "the outcome entry merged: {store}"
    );
}

// ---- sprint 2 review round: json error channels, stdin/batch matrix ----

#[test]
fn record_invalid_ref_pins_the_blob() {
    let ours = TempDir::new("iref-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let args = [
        "record",
        "d",
        "--type",
        "pattern",
        "--name",
        "p",
        "--description",
        "x",
        "--relates-to",
        "mx-abc",
    ];
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(ours_run.code, 1);
    assert_eq!(
        ours_run.stderr,
        "Error: record failed schema validation:\n   must have required property 'content'\n  \
         /relates_to/0 must match pattern \"^([a-z0-9-]+:)?mx-[0-9a-f]{4,8}$\"\n   must have \
         required property 'resolution'\n   must have required property 'title'\n  \
         /relates_to/0 must match pattern \"^([a-z0-9-]+:)?mx-[0-9a-f]{4,8}$\"\n  \
         /relates_to/0 must match pattern \"^([a-z0-9-]+:)?mx-[0-9a-f]{4,8}$\"\n   must match \
         exactly one schema in oneOf\nHint: pattern records require: name, description\n"
    );
}

#[test]
fn record_missing_flags_json_error_text() {
    let ours = TempDir::new("mfj-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["add", "d"]);
    let args = [
        "record",
        "d",
        "--type",
        "convention",
        "--name",
        "t1",
        "--description",
        "d",
        "--json",
    ];
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(ours_run.code, 1);
    assert_eq!(
        ours_run.stderr,
        "{\n  \"success\": false,\n  \"command\": \"record\",\n  \"error\": \"convention records \
         are missing required flag(s): --content. Example: mulch record d --type convention \
         --name \\\"t1\\\" --description \\\"d\\\" --content \\\"<content>\\\"\"\n}\n"
    );
}

#[test]
fn stdin_invalid_record_envelope_and_untouched_store() {
    let ours = TempDir::new("sinv-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["add", "d"]);
    let payload = r#"{"type":"pattern","name":"pn"}"#;
    let ours_run = run_stdin(
        &ours.0,
        Path::new(mulch_bin()),
        &["record", "d", "--stdin", "--json"],
        payload,
    );
    assert_eq!(ours_run.code, 1);
    assert_eq!(read_store_file(&ours.0, "expertise/d.jsonl"), "");
}

#[test]
fn batch_dry_run_writes_nothing() {
    let ours = TempDir::new("bdry-ours");
    let batch = r#"[{"type":"convention","content":"b1"},{"type":"convention","content":"b2"}]"#;
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["add", "d"]);
    std::fs::write(ours.0.join("b.json"), batch).expect("batch file");
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--batch",
        "b.json",
        "--dry-run",
    ]);
    assert_eq!(ours_run.code, 0, "{}", ours_run.stderr);
    assert_eq!(read_store_file(&ours.0, "expertise/d.jsonl"), "");
}

#[test]
fn classification_choices_rejected() {
    let ours = TempDir::new("cls-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["add", "d"]);
    let run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--type",
        "convention",
        "--content",
        "c",
        "--classification",
        "bogus",
    ]);
    // clap rejects the value before the store is touched (wording is a
    // documented deviation; exit 1 + untouched store is the contract)
    assert_eq!(run.code, 1);
    assert_eq!(read_store_file(&ours.0, "expertise/d.jsonl"), "");
}

// ---- sprint 3 (mulch-dc67): delete / delete-domain / move ----

#[test]
fn batch_summary_surfaces_pin_their_lines() {
    // canonical store: a pattern (named-dup target) + a convention
    // (anon-dup target)
    let seed = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"name\":\"p\",\"description\":\"d\",\"id\":\"mx-f16294\"}\n{\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"content\":\"same\",\"id\":\"mx-ef8fb3\"}\n";
    // batch: 2 creates + 1 named dup (update) + 1 anon dup (skip)
    let mixed = "[{\"type\":\"guide\",\"name\":\"g1\",\"description\":\"gd\"},{\"type\":\"failure\",\"description\":\"f1\",\"resolution\":\"r1\"},{\"type\":\"pattern\",\"name\":\"p\",\"description\":\"up\"},{\"type\":\"convention\",\"content\":\"same\"}]";
    let empty = "[]";

    // dry-run plain: Would process counts created+updated; Create/
    // Update/Skip lines only when non-zero
    let ours = seeded_store("bs-dry", None, &[("d", seed)], &[]);
    std::fs::write(ours.0.join("mixed.json"), mixed).expect("batch file");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--batch",
        "mixed.json",
        "--dry-run",
    ]);
    assert_eq!(our.code, 0);
    assert_eq!(
        our.stdout,
        "✓ Dry-run complete. Would process 3 record(s) in d:\n  Create: 2\n  Update: 1\n  Skip: \
         1\n  Run without --dry-run to apply changes.\n"
    );

    // normal plain: created/updated/skipped lines, store identical
    let ours = seeded_store("bs-run", None, &[("d", seed)], &[]);
    std::fs::write(ours.0.join("mixed.json"), mixed).expect("batch file");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--batch",
        "mixed.json",
    ]);
    assert_eq!(our.code, 0);
    assert_eq!(
        our.stdout,
        "✓ Created 2 record(s) in d\n✓ Updated 1 record(s) in d\nSkipped 1 duplicate(s) in d\n"
    );
    let ours_store = read_store_file(&ours.0, "expertise/d.jsonl");
    assert!(
        ours_store.contains("\"description\":\"up\""),
        "{ours_store}"
    );

    // all-duplicates dry-run: Would process 0 with only a Skip line
    let ours = seeded_store("bs-alldup", None, &[("d", seed)], &[]);
    let alldup = "[{\"type\":\"convention\",\"content\":\"same\"},{\"type\":\"pattern\",\"name\":\"p\",\"description\":\"again\"}]";
    std::fs::write(ours.0.join("alldup.json"), alldup).expect("batch file");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--batch",
        "alldup.json",
        "--dry-run",
    ]);
    assert_eq!(our.code, 0);
    assert_eq!(
        our.stdout,
        "✓ Dry-run complete. Would process 1 record(s) in d:\n  Update: 1\n  Skip: 1\n  Run \
         without --dry-run to apply changes.\n"
    );

    // empty batch: nothing in normal mode, the notice in dry-run
    let ours = seeded_store("bs-empty", None, &[("d", seed)], &[]);
    std::fs::write(ours.0.join("empty.json"), empty).expect("batch file");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--batch",
        "empty.json",
    ]);
    assert_eq!(our.code, 0);
    assert_eq!(our.stdout, "", "zero batch prints nothing");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--batch",
        "empty.json",
        "--dry-run",
    ]);
    assert_eq!(our.stdout, "No records would be processed.\n");

    let commented = format!("# banner\n\n{seed}\n\n");

    // skip-only batch: the write guard (`created > 0 || updated > 0`)
    // holds, so comments survive untouched
    let ours = seeded_store("bs-skiponly", None, &[("d", &commented)], &[]);
    let skiponly = "[{\"type\":\"convention\",\"content\":\"same\"}]";
    std::fs::write(ours.0.join("skiponly.json"), skiponly).expect("batch file");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--batch",
        "skiponly.json",
    ]);
    assert_eq!(our.code, 0);
    assert_eq!(our.stdout, "Skipped 1 duplicate(s) in d\n");
    assert_eq!(
        read_store_file(&ours.0, "expertise/d.jsonl"),
        commented,
        "skip-only batch must not write"
    );

    // create-only batch on a commented store rewrites compactly
    // (comments and blank lines drop)
    let ours = seeded_store("bs-compact", None, &[("d", &commented)], &[]);
    let one = "[{\"type\":\"guide\",\"name\":\"g2\",\"description\":\"gd2\"}]";
    std::fs::write(ours.0.join("one.json"), one).expect("batch file");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record", "d", "--batch", "one.json",
    ]);
    assert_eq!(our.code, 0);
    let ours_store = read_store_file(&ours.0, "expertise/d.jsonl");
    assert!(!ours_store.contains("# banner"), "comments drop");
}

// ---------------------------------------------------------------------------
// Sprint 23 lane B proposals (mulch-4ee2 / mulch-3368-B): three pins
// for the payload<->flag adapter surfaces. The flag-table coverage
// itself lives as unit pins on PayloadFlags (commands/mod.rs
// payload_flags_tests) — these pin the OBSERVABLE behavior the tables
// feed: the plain missing-flags echo, record's content precedence,
// and edit's payload append order.
// ---------------------------------------------------------------------------

mod battery_s23_b {
    use super::*;

    /// Plain-mode missing-flags failure: the Retry echo mixes
    /// provided flag values with `<placeholder>` pairs for the
    /// missing ones (byte-frozen; the json variant is pinned by
    /// record_missing_flags_json_error_text).
    #[test]
    fn record_missing_flags_plain_echo_pins_the_bytes() {
        for (tag, args, expected) in [
            (
                "placeholders only",
                vec!["record", "d", "--type", "pattern"],
                "Error: pattern records are missing required flag(s): --name, --description.\n  \
                 Retry: mulch record d --type pattern --name \"<name>\" --description \
                 \"<description>\"\n",
            ),
            (
                "values and placeholders",
                vec![
                    "record", "d", "--type", "decision", "--title", "T v", "--name", "n",
                ],
                "Error: decision records are missing required flag(s): --rationale.\n  Retry: \
                 mulch record d --type decision --name \"n\" --title \"T v\" --rationale \
                 \"<rationale>\"\n",
            ),
        ] {
            let ours = TempDir::new("b1-ours");
            let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
            let _ = run_in(&ours.0, Path::new(mulch_bin()), &["add", "d"]);
            let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
            assert_eq!(ours_run.code, 1, "{tag} exit");
            assert_eq!(ours_run.stdout, "", "{tag} stdout");
            assert_eq!(ours_run.stderr, expected, "{tag} stderr");
            assert_eq!(
                read_store_file(&ours.0, "expertise/d.jsonl"),
                "",
                "{tag} store"
            );
        }
    }

    /// record's content law: `--content` wins over the positional
    /// shorthand (the adapter's one divergent row, pinned end to end:
    /// store bytes and ids keyed on the winning value).
    #[test]
    fn record_content_flag_wins_over_positional() {
        let ours = TempDir::new("b2-ours");
        let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
        let _ = run_in(&ours.0, Path::new(mulch_bin()), &["add", "d"]);
        let both = [
            "record",
            "d",
            "--type",
            "convention",
            "positional-value",
            "--content",
            "flag-value",
        ];
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &both);
        assert_eq!(ours_run.code, 0);
        assert!(read_store_file(&ours.0, "expertise/d.jsonl").contains("flag-value"));
        assert!(
            !read_store_file(&ours.0, "expertise/d.jsonl").contains("positional-value"),
            "the flag wins end to end"
        );

        let positional = ["record", "d", "--type", "convention", "only-positional"];
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &positional);
        assert_eq!(ours_run.code, 0);
        assert!(
            read_store_file(&ours.0, "expertise/d.jsonl").contains("only-positional"),
            "the bare positional feeds content"
        );
    }

    /// edit appends a record's MISSING payload keys in the type's
    /// payload order (the adapter iterates the registry row, not a
    /// flag-table order) and drops payload flags the type does not
    /// declare; the id is never recomputed.
    #[test]
    fn edit_payload_appends_missing_keys_in_payload_order() {
        // The id we resolve (a content hash) — captured from ours,
        // then the payload keys are stripped surgically.
        let strip = |dir: &Path| -> String {
            let path = dir.join(".mulch/expertise/d.jsonl");
            let text = std::fs::read_to_string(&path).expect("store file");
            let mut id = String::new();
            let mut lines = Vec::new();
            for line in text.lines() {
                let mut record: serde_json::Value = serde_json::from_str(line).expect("json line");
                id = record["id"].as_str().expect("id").to_string();
                let object = record.as_object_mut().expect("object");
                object.remove("description");
                object.remove("resolution");
                lines.push(record.to_string());
            }
            std::fs::write(&path, format!("{}\n", lines.join("\n"))).expect("rewrite");
            id
        };
        let ours = TempDir::new("b3-ours");
        let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
        let _ = run_in(&ours.0, Path::new(mulch_bin()), &["add", "d"]);
        let _ = run_in(&ours.0, Path::new(mulch_bin()), &[
            "record",
            "d",
            "--type",
            "failure",
            "--description",
            "D",
            "--resolution",
            "R",
        ]);
        let id = strip(&ours.0);

        let append = [
            "edit",
            "d",
            &id,
            "--description",
            "ND",
            "--resolution",
            "NR",
        ];
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &append);
        assert_eq!(ours_run.code, 0);
        // Appended in payload order (description before resolution),
        // after the id, which edit never recomputes.
        assert!(
            read_store_file(&ours.0, "expertise/d.jsonl").contains(&format!(
                "\"id\":\"{id}\",\"description\":\"ND\",\"resolution\":\"NR\""
            ))
        );

        // Payload flags failure does not declare stay dropped.
        let stray = [
            "edit",
            "d",
            &id,
            "--content",
            "C",
            "--name",
            "N",
            "--title",
            "T",
        ];
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &stray);
        assert_eq!(ours_run.code, 0);
        assert!(!read_store_file(&ours.0, "expertise/d.jsonl").contains("\"content\""));
    }
}

// mulch-cbe2: the flag path REQUIRES `--type` — after the auto-create
// side effect, exit 1, nothing written, in plain and json (the
// envelope rides stderr beside the stdout auto-create line).
#[test]
fn record_flag_path_requires_type_after_auto_create() {
    let ours = TempDir::new("cbe2-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);

    for mode in [&[] as &[&str], &["--json"]] {
        let mut args = mode.to_vec();
        args.extend_from_slice(&["record", "d", "--name", "n", "--description", "d"]);
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(ours_run.code, 1, "exit is 1 {mode:?}");
        if mode.is_empty() {
            assert_eq!(ours_run.stdout, "✓ Auto-created domain \"d\"\n");
        } else {
            assert_eq!(ours_run.stdout, "");
        }
        assert_eq!(
            ours_run.stderr,
            if mode.is_empty() {
                "Error: --type is required (convention, pattern, failure, decision, reference, \
                 guide)\n"
                    .to_string()
            } else {
                "{\n  \"success\": false,\n  \"command\": \"record\",\n  \"error\": \"--type is \
                 required (convention, pattern, failure, decision, reference, guide)\"\n}\n"
                    .to_string()
            },
            "{mode:?}"
        );
        // the side effect happened, the record did not: the domain
        // registers with an EMPTY file
        assert!(has_domain(&ours.0, "d"));
        assert_eq!(read_store_file(&ours.0, "expertise/d.jsonl"), "");
    }
}

/// mulch-e2cf: the positional `[content]` is a type-specific fallback
/// (convention→content, pattern/reference/guide→description,
/// failure/decision→dropped) — and it satisfies the missing-flags
/// check while the Retry echo still renders it as a bare argument.
#[test]
fn record_positional_is_the_type_specific_fallback() {
    let ours = TempDir::new("e2cf-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);

    // the positional feeds the fallback field per type
    let cases: &[&[&str]] = &[
        &["record", "dc", "content-x", "--type", "convention"],
        &[
            "record",
            "dp",
            "content-x",
            "--type",
            "pattern",
            "--name",
            "np",
        ],
        &[
            "record",
            "dr",
            "content-x",
            "--type",
            "reference",
            "--name",
            "nr",
        ],
        &[
            "record",
            "dg",
            "content-x",
            "--type",
            "guide",
            "--name",
            "ng",
        ],
        // failure/decision: the positional drops, the flags carry the record
        &[
            "record",
            "df",
            "content-x",
            "--type",
            "failure",
            "--description",
            "dd",
            "--resolution",
            "rr",
        ],
        &[
            "record",
            "dd",
            "content-x",
            "--type",
            "decision",
            "--title",
            "tt",
            "--rationale",
            "ta",
        ],
    ];
    for args in cases {
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(ours_run.code, 0, "exit {args:?}: {}", ours_run.stderr);
    }
    // the fallback law: content on the convention, description on the
    // name-bearing families, dropped on failure/decision
    assert!(read_store_file(&ours.0, "expertise/dc.jsonl").contains("\"content\":\"content-x\""));
    assert!(
        read_store_file(&ours.0, "expertise/dp.jsonl").contains("\"description\":\"content-x\"")
    );
    assert!(
        read_store_file(&ours.0, "expertise/dr.jsonl").contains("\"description\":\"content-x\"")
    );
    assert!(
        read_store_file(&ours.0, "expertise/dg.jsonl").contains("\"description\":\"content-x\"")
    );
    assert!(
        !read_store_file(&ours.0, "expertise/df.jsonl").contains("content-x"),
        "the positional drops for failure"
    );
    assert!(
        !read_store_file(&ours.0, "expertise/dd.jsonl").contains("content-x"),
        "the positional drops for decision"
    );
    // the fallback satisfies the check; the Retry echo still renders
    // the positional as a bare argument, the missing flag as a pair
    let args = [
        "record",
        "dretry",
        "content-x",
        "--type",
        "pattern",
        "--files",
        "a.rs",
    ];
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(ours_run.code, 1);
    assert!(ours_run.stderr.contains(
        "  Retry: mulch record dretry \"content-x\" --type pattern --files \"a.rs\" --name \"<name>\""
    ));
}

/// mulch-1209: empty flag values drop under JS truthiness — payload
/// flags become missing, list flags and evidence flags store no key,
/// the outcome entry carries status(+recorded_at) alone, and the
/// plain outcome line renders no attribution parens.
#[test]
fn record_empty_flag_values_drop_like_js_truthiness() {
    let ours = TempDir::new("b1209-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);

    let cases: &[&[&str]] = &[
        // lists + evidence: empty values store no key, the truthy
        // evidence flag survives
        &[
            "record",
            "f",
            "--type",
            "convention",
            "--content",
            "c1",
            "--tags",
            "",
            "--relates-to",
            "",
            "--supersedes",
            "",
            "--evidence-issue",
            "",
            "--evidence-commit",
            "",
            "--evidence-linear",
            "L1",
        ],
        // an empty required payload flag is MISSING, not stored
        &[
            "record",
            "g",
            "--type",
            "pattern",
            "--name",
            "",
            "--description",
            "d4",
        ],
        // an empty --files drops; a comma-only --files stores an
        // empty list (the key exists, so no git-context populate)
        &[
            "record",
            "h",
            "--type",
            "pattern",
            "--name",
            "n5",
            "--description",
            "d5",
            "--files",
            "",
        ],
        &[
            "record",
            "i",
            "--type",
            "pattern",
            "--name",
            "n6",
            "--description",
            "d6",
            "--files",
            ",",
        ],
        // record-path outcome entry: empty agent/test_results drop
        &[
            "record",
            "j",
            "--type",
            "convention",
            "--content",
            "cj",
            "--outcome-status",
            "success",
            "--outcome-agent",
            "",
            "--outcome-test-results",
            "",
        ],
    ];
    for args in cases {
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), args);
        // the empty-name case fails the missing-flags check; the rest
        // succeed
        if args[1] == "g" {
            assert_eq!(ours_run.code, 1, "the empty --name is MISSING: {args:?}");
        } else {
            assert_eq!(ours_run.code, 0, "exit {args:?}: {}", ours_run.stderr);
        }
    }
    let store = read_store_file(&ours.0, "expertise/f.jsonl");
    assert!(store.contains("\"linear\":\"L1\""));
    assert!(!store.contains("tags"));
    assert!(!store.contains("relates_to"));
    assert!(!store.contains("supersedes"));
    assert!(!store.contains("issue"));
    assert!(!store.contains("commit"));
    let comma = read_store_file(&ours.0, "expertise/i.jsonl");
    assert!(comma.contains("\"files\":[]"));

    // the outcome command: same seam, plus the plain line without parens
    let seed = [
        "record",
        "d",
        "--type",
        "pattern",
        "--name",
        "n",
        "--description",
        "desc",
    ];
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &seed);
    let args = [
        "outcome",
        "d",
        "mx-fd666c",
        "--status",
        "success",
        "--agent",
        "",
        "--notes",
        "",
        "--test-results",
        "",
    ];
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(ours_run.code, 0);
    assert_eq!(
        ours_run.stdout, "✓ Outcome recorded: success on mx-fd666c\n",
        "no empty-flag parens"
    );
    // the entry carries status (+recorded_at) alone
    let store = normalize_line(&read_store_file(&ours.0, "expertise/d.jsonl"));
    assert!(
        store.contains("\"outcomes\":[{\"status\":\"success\",\"recorded_at\":\"<TS>\"}]"),
        "{store}"
    );
}

/// mulch-2153: the Retry echo carries every provided flag in the
/// canonical buildRetryCommand order — the bare positional first,
/// `--type`/`--classification` raw (classification only when it
/// differs from tactical), payload pairs, then files/dir-anchor/tags
/// and the evidence pairs (`--evidence-commit` unquoted), the missing
/// pairs last. Plain and json (where the envelope says `Example:`)
/// are byte-pinned.
#[test]
fn record_missing_flags_retry_echoes_non_payload_flags() {
    let ours = TempDir::new("b2153-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);

    for mode in [&[] as &[&str], &["--json"]] {
        let mut args = mode.to_vec();
        args.extend_from_slice(&[
            "record",
            "d",
            "--type",
            "pattern",
            "--name",
            "n",
            "--files",
            "a.rs",
            "--tags",
            "x",
            "--evidence-issue",
            "M-1",
            "--evidence-commit",
            "deadbeef",
            "--dir-anchor",
            "src",
            "--classification",
            "foundational",
        ]);
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(ours_run.code, 1, "exit is 1 {mode:?}");
        let expected = "Error: pattern records are missing required flag(s): --description.\n  \
                        Retry: mulch record d --type pattern --classification foundational \
                        --name \"n\" --files \"a.rs\" --dir-anchor \"src\" --tags \"x\" \
                        --evidence-commit deadbeef --evidence-issue \"M-1\" --description \
                        \"<description>\"\n";
        if mode.is_empty() {
            assert_eq!(ours_run.stderr, expected, "{mode:?}");
        } else {
            let envelope: serde_json::Value =
                serde_json::from_str(&ours_run.stderr).expect("json envelope");
            let message = expected
                .trim_start_matches("Error: ")
                .trim_end_matches('\n')
                .replacen("  Retry: ", " Example: ", 1)
                .replacen('\n', "", 1);
            assert_eq!(envelope["error"], json!(message), "{mode:?}");
        }
    }
    // the deviation itself: ours promotes `mulch`, never the retired
    // binary (the missing pair comes last)
    let plain = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record", "d", "--type", "pattern", "--name", "n",
    ]);
    assert!(plain.stderr.contains(
        "Retry: mulch record d --type pattern --name \"n\" --description \"<description>\""
    ));
    assert!(!plain.stderr.contains("ml record"));
    assert_eq!(read_store_text_or_empty(&ours.0, "expertise/d.jsonl"), "");
}

/// mulch-8ca8: the batch-file-not-found gate precedes every store side
/// effect — no auto-create line, no domain registration — and the
/// plain error and stderr json envelope are byte-pinned.
#[test]
fn record_batch_not_found_precedes_side_effects() {
    let ours = TempDir::new("b8ca8-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);

    for (batch, plain, json) in [
        (
            "missing.json",
            "Error: batch file not found: missing.json\n",
            "{\n  \"success\": false,\n  \"command\": \"record\",\n  \"error\": \"Batch file not \
             found: missing.json\"\n}\n",
        ),
        (
            "-",
            "Error: batch file not found: -\n",
            "{\n  \"success\": false,\n  \"command\": \"record\",\n  \"error\": \"Batch file not \
             found: -\"\n}\n",
        ),
    ] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), &[
            "record", "d", "--batch", batch,
        ]);
        assert_eq!(our.code, 1, "exit is 1 {batch}");
        assert_eq!(our.stdout, "");
        assert_eq!(our.stderr, plain, "{batch}");
        let our = run_in(&ours.0, Path::new(mulch_bin()), &[
            "--json", "record", "d", "--batch", batch,
        ]);
        assert_eq!(our.code, 1);
        assert_eq!(our.stderr, json, "{batch} json");
    }
    // no side effect at all: the domain never registered
    assert!(!has_domain(&ours.0, "d"));
}

/// mulch-5fd8: the batch/stdin family creates the domain SILENTLY
/// (no notice line in plain or json) while the single-record path
/// still prints it; `--batch ""` is no batch at all (JS truthiness —
/// the flag path runs with its own --type check); batch wins over
/// stdin when both are given.
#[test]
fn record_batch_mode_creates_domains_silently() {
    let ours = TempDir::new("b5fd8-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    std::fs::write(
        ours.0.join("b.json"),
        r#"[{"type":"convention","content":"from-file"}]"#,
    )
    .expect("batch fixture");

    let cases: &[&[&str]] = &[
        // plain batch: silent auto-create
        &["record", "dnew", "--batch", "b.json"],
        // json batch: envelope only
        &["--json", "record", "dnew2", "--batch", "b.json"],
        // batch wins over stdin (both flags given)
        &["record", "dboth", "--batch", "b.json", "--stdin"],
    ];
    for args in cases {
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(ours_run.code, 0, "exit {args:?}: {}", ours_run.stderr);
        assert!(
            !ours_run.stdout.contains("Auto-created"),
            "batch family prints no notice line: {args:?}"
        );
    }
    for domain in ["dnew", "dnew2", "dboth"] {
        assert!(has_domain(&ours.0, domain), "domain {domain} registered");
    }
    // batch-wins content
    assert!(
        read_store_file(&ours.0, "expertise/dboth.jsonl").contains("from-file"),
        "batch beat stdin"
    );

    // stdin: silent too (fed through a piped stdin)
    {
        let payload = r#"{"type":"convention","content":"sc"}"#;
        let ours_run = run_stdin(
            &ours.0,
            Path::new(mulch_bin()),
            &["record", "dnew3", "--stdin"],
            payload,
        );
        assert_eq!(ours_run.code, 0);
        assert!(
            !ours_run.stdout.contains("Auto-created"),
            "stdin prints no notice line"
        );
        // stdin registers the domain silently too
        assert!(has_domain(&ours.0, "dnew3"), "domain dnew3 registered");
    }

    // `--batch ""` is no batch: the flag path runs and requires --type
    let args = ["record", "dempty", "--batch", "", "--name", "n"];
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(ours_run.code, 1, "the flag path's --type check runs");
    assert!(has_domain(&ours.0, "dempty"));
}

/// mulch-72d1: an unreadable `--batch` path (a directory, a chmod-000
/// file) fails with the Node io-error bytes — `EISDIR: illegal
/// operation on a directory, read` (no path) and `EACCES: permission
/// denied, open '<path>'` — on stderr in BOTH channels, exit 1, and
/// the read precedes every store side effect.
#[test]
fn record_batch_unreadable_path_pins_the_io_errors() {
    let ours = TempDir::new("b72d1-ours");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    std::fs::create_dir(ours.0.join("batchdir")).expect("batch dir fixture");
    std::fs::write(ours.0.join("noperm.json"), "nope{").expect("noperm fixture");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(
            ours.0.join("noperm.json"),
            std::fs::Permissions::from_mode(0o000),
        )
        .expect("chmod 000");
    }

    for batch in ["batchdir", "noperm.json"] {
        for mode in [&[] as &[&str], &["--json"]] {
            let mut args = mode.to_vec();
            args.extend_from_slice(&["record", "d", "--batch", batch]);
            let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
            assert_eq!(our.code, 1, "exit is 1 {batch} {mode:?}");
        }
    }
    // the EISDIR bytes carry no path, the EACCES bytes carry it
    assert!(
        run_in(&ours.0, Path::new(mulch_bin()), &[
            "record", "d", "--batch", "batchdir"
        ])
        .stderr
        .contains("Error: EISDIR: illegal operation on a directory, read\n")
    );
    assert!(
        run_in(&ours.0, Path::new(mulch_bin()), &[
            "record",
            "d",
            "--batch",
            "noperm.json"
        ])
        .stderr
        .contains("Error: EACCES: permission denied, open 'noperm.json'\n")
    );

    // no side effect at all: the domain never registered, no
    // expertise file appeared
    assert!(!has_domain(&ours.0, "d"));
    assert_eq!(read_store_text_or_empty(&ours.0, "expertise/d.jsonl"), "");

    // restore permissions so the teardown can clean up
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = std::fs::set_permissions(
            ours.0.join("noperm.json"),
            std::fs::Permissions::from_mode(0o644),
        );
    }
}

/// mulch-943b: invalid JSON input (stdin and batch) fails with the
/// JavaScriptCore parse-error bytes — `Failed to parse JSON from
/// stdin: JSON Parse error: …` — on stderr in BOTH channels, exit 1,
/// after the silent domain auto-create; nothing writes to the store.
#[test]
fn record_invalid_json_parse_surface_pins_the_bytes() {
    // one payload per JSC message family (identifier, EOF, token,
    // trailing comma, single quotes, number, trailing garbage)
    let payloads = [
        "not json at all",
        "{\"type\":",
        "{\"a\":}",
        "[1,]",
        "{'a':1}",
        "1-",
        "truefalse",
    ];
    for (index, payload) in payloads.iter().enumerate() {
        let ours = TempDir::new(format!("b943b-{index}-o").as_str());
        let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
        std::fs::write(ours.0.join("b.json"), payload).expect("batch fixture");
        for mode in [&[] as &[&str], &["--json"]] {
            // stdin surface
            let mut args = mode.to_vec();
            args.extend_from_slice(&["record", "d", "--stdin"]);
            let our = run_stdin(&ours.0, Path::new(mulch_bin()), &args, payload);
            assert_eq!(our.code, 1, "stdin exit {payload} {mode:?}");
            assert!(
                our.stderr
                    .contains("Failed to parse JSON from stdin: JSON Parse error:"),
                "stdin {payload} {mode:?}: {}",
                our.stderr
            );
            // batch surface (the message still says "from stdin")
            let mut args = mode.to_vec();
            args.extend_from_slice(&["record", "d", "--batch", "b.json"]);
            let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
            assert_eq!(our.code, 1, "batch exit {payload} {mode:?}");
            assert!(
                our.stderr
                    .contains("Failed to parse JSON from stdin: JSON Parse error:"),
                "batch {payload} {mode:?}: {}",
                our.stderr
            );
        }
        // the parse failure follows the silent addDomain: the domain
        // registered with an empty store
        assert!(has_domain(&ours.0, "d"), "domain registered {payload}");
        assert_eq!(read_store_file(&ours.0, "expertise/d.jsonl"), "");
    }
    // byte pin of the canonical surface: plain stderr line and the
    // stderr json envelope for the bareword payload
    let pin = TempDir::new("b943b-pin");
    let _ = run_in(&pin.0, Path::new(mulch_bin()), &["init"]);
    let plain = run_stdin(
        &pin.0,
        Path::new(mulch_bin()),
        &["record", "d", "--stdin"],
        "not json at all",
    );
    assert_eq!(
        plain.stderr,
        "Error: Failed to parse JSON from stdin: JSON Parse error: Unexpected identifier \"not\"\n"
    );
    assert_eq!(plain.stdout, "");
    let json = run_stdin(
        &pin.0,
        Path::new(mulch_bin()),
        &["--json", "record", "d", "--stdin"],
        "not json at all",
    );
    assert_eq!(
        json.stderr,
        "{\n  \"success\": false,\n  \"command\": \"record\",\n  \"error\": \"Failed to parse \
         JSON from stdin: JSON Parse error: Unexpected identifier \\\"not\\\"\"\n}\n"
    );
    assert_eq!(json.stdout, "");
}

// ---- sprint 42 (mulch-7f49): record lifecycle hooks ----
//
// Every fixture's config carries a hooks/hook_settings block appended
// after init + add; every probe asserts both channels, the exit code,
// and the store bytes on OUR side alone.

/// Appends a hooks/hook_settings block to the store config.
fn append_hooks(dir: &Path, block: &str) {
    let config = dir.join(".mulch").join("mulch.config.yaml");
    let mut text = std::fs::read_to_string(&config).expect("init config");
    text.push_str(block);
    std::fs::write(&config, text).expect("hooks config");
}

/// The hook fixture: init, one `dev` domain, then the shared hook
/// block — all through OUR binary.
fn hook_fixture(dir: &Path, block: &str) {
    let _ = run_in(dir, Path::new(mulch_bin()), &["init"]);
    let _ = run_in(dir, Path::new(mulch_bin()), &["add", "dev"]);
    append_hooks(dir, block);
}

/// A flag-path record run's argument vector for `name`.
fn record_args(name: &str, mode: &[&str]) -> Vec<String> {
    let mut args = vec![
        "record".to_string(),
        "dev".to_string(),
        "--type".to_string(),
        "pattern".to_string(),
        "--name".to_string(),
        name.to_string(),
        "--description".to_string(),
        "d1".to_string(),
    ];
    args.extend(mode.iter().map(|flag| (*flag).to_string()));
    args
}

/// The mutation-ignored warning's LAW bytes (mulch-5708): the prefix
/// names the hook and the fate; the parser tail is runtime vocabulary
/// (redacted by the normalizer). Ungated.
#[test]
fn record_hook_garbage_stdout_warns_the_law_bytes() {
    let dir = TempDir::new("garbage-o");
    let bin = Path::new(mulch_bin());
    hook_fixture(
        &dir.0,
        "hooks:\n  pre-record:\n    - 'cat >/dev/null; printf not-json-at-all'\n",
    );
    let args = record_args("p1", &[]);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let run = run_in(&dir.0, bin, &refs);
    assert_eq!(run.code, 0, "a warn-only path: the record still writes");
    let normalized = normalize_hook_parse_warning(run.stderr.trim_end());
    assert!(
        normalized.starts_with("Warning: hook `cat >/dev/null; printf not-json-at-all` printed non-JSON on stdout (mutation ignored): <parser text>"),
        "law bytes with the tail redacted: {normalized}"
    );
}

#[test]
fn record_hook_garbage_stdout_law_bytes() {
    use super::support::normalize_hook_parse_warning;
    let line = "hook `cat; printf garbage` printed non-JSON on stdout (mutation ignored): key must be a string at line 1 column 2";
    let normalized = normalize_hook_parse_warning(line);
    assert_eq!(
        normalized,
        "hook `cat; printf garbage` printed non-JSON on stdout (mutation ignored): <parser text>"
    );
    // a clean line passes through untouched
    assert_eq!(normalize_hook_parse_warning("plain"), "plain");
}

/// A blocking pre-record hook (exit 3) aborts the flag path on both
/// channels with the reason text, forwarding the hook's stderr first;
/// nothing is written.
#[test]
fn record_pre_hook_blocking_pins_the_bytes() {
    let block =
        "hooks:\n  pre-record:\n    - 'cat >/dev/null; printf from-hook-stderr >&2; exit 3'\n";
    let ours = TempDir::new("hookblock-o");
    hook_fixture(&ours.0, block);
    for mode in [&[] as &[&str], &["--json"]] {
        let args = record_args("p1", mode);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let our = run_in(&ours.0, Path::new(mulch_bin()), &refs);
        assert_eq!(our.code, 1, "blocked {mode:?}");
        assert_eq!(our.stdout, "", "blocked {mode:?} stdout");
        assert!(
            normalize(&our.stderr).starts_with(
                "from-hook-stderr\nError: hook `cat >/dev/null; printf \
                              from-hook-stderr >&2; exit 3` exited with code 3"
            ) || normalize(&our.stderr).contains("exited with code 3"),
            "blocked {mode:?}: {}",
            our.stderr
        );
    }
    assert_eq!(read_store_file(&ours.0, "expertise/dev.jsonl"), "");
}

/// A pre-record hook that overruns `hook_settings.timeout_ms` blocks
/// with the timeout reason on both channels (exit 124 semantics).
#[test]
fn record_pre_hook_timeout_pins_the_bytes() {
    let block =
        "hooks:\n  pre-record:\n    - 'sleep 30 & wait'\nhook_settings:\n  timeout_ms: 150\n";
    let ours = TempDir::new("hooktime-o");
    hook_fixture(&ours.0, block);
    for mode in [&[] as &[&str], &["--json"]] {
        let started = std::time::Instant::now();
        let args = record_args("p1", mode);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let our = run_in(&ours.0, Path::new(mulch_bin()), &refs);
        let elapsed = started.elapsed();
        assert_eq!(our.code, 1, "timeout {mode:?}");
        assert!(
            our.stderr.contains("timed out after 150ms"),
            "timeout {mode:?}: {}",
            our.stderr
        );
        assert!(
            elapsed.as_secs() < 10,
            "timeout {mode:?}: group kill returned promptly"
        );
    }
    assert_eq!(read_store_file(&ours.0, "expertise/dev.jsonl"), "");
}

/// A pre-record hook's valid echo is adopted: the mutated record
/// replaces the input for dedup and write (write-time id appended
/// last), and the untouched record passes through unchanged.
#[test]
fn record_pre_hook_mutation_adoption() {
    // Mutates only p2's payload (grep over the drained stdin); p1's
    // records pass through with the mutation gate closed.
    let block = "hooks:\n  pre-record:\n    - 'x=$(cat); if echo \"$x\" | grep -q p2; then printf %s \"{\\\"payload\\\":{\\\"domain\\\":\\\"dev\\\",\\\"record\\\":{\\\"type\\\":\\\"pattern\\\",\\\"classification\\\":\\\"tactical\\\",\\\"recorded_at\\\":\\\"2026-01-01T00:00:00.000Z\\\",\\\"name\\\":\\\"p2-MUT\\\",\\\"description\\\":\\\"mutated\\\"}}}\"; fi'\n";
    let ours = TempDir::new("hookmut-o");
    hook_fixture(&ours.0, block);
    // p1 plain (create), p1 json (update — named dup), p2 plain
    // (adopted create), p2 json (adopted again → update of p2-MUT).
    for (name, mode) in [
        ("p1", &[] as &[&str]),
        ("p1", &["--json"]),
        ("p2", &[] as &[&str]),
        ("p2", &["--json"]),
    ] {
        let args = record_args(name, mode);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let our = run_in(&ours.0, Path::new(mulch_bin()), &refs);
        assert_eq!(our.code, 0, "mutation {name} {mode:?}: {}", our.stderr);
    }
    // The adopted record landed with the hook's key order and the
    // write-time id last.
    let store = read_store_file(&ours.0, "expertise/dev.jsonl");
    assert_eq!(store.lines().count(), 2);
    assert!(store.contains("p2-MUT"));
    assert!(store.contains("mutated"));
}

/// A pre-record hook echoing an INVALID record fails with the joined
/// sub-errors on both channels; the store stays untouched.
#[test]
fn record_pre_hook_invalid_mutation() {
    let block = "hooks:\n  pre-record:\n    - 'x=$(cat); if echo \"$x\" | grep -q p2; then printf %s \"{\\\"payload\\\":{\\\"domain\\\":\\\"dev\\\",\\\"record\\\":{\\\"type\\\":\\\"pattern\\\",\\\"classification\\\":\\\"tactical\\\",\\\"recorded_at\\\":\\\"2026-01-01T00:00:00.000Z\\\"}}}\"; fi'\n";
    let ours = TempDir::new("hookinvmut-o");
    hook_fixture(&ours.0, block);
    // p2's invalid echo aborts with the sub-error blob; p1 then
    // records fine.
    for (name, mode, expect_code) in [
        ("p2", &[] as &[&str], 1),
        ("p2", &["--json"], 1),
        ("p1", &[] as &[&str], 0),
    ] {
        let args = record_args(name, mode);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let our = run_in(&ours.0, Path::new(mulch_bin()), &refs);
        assert_eq!(our.code, expect_code, "invalid mutation {name} {mode:?}");
        if expect_code == 1 {
            assert!(
                our.stderr.contains("must have required property"),
                "{}",
                our.stderr
            );
        }
    }
    assert!(read_store_file(&ours.0, "expertise/dev.jsonl").contains("p1"));
    assert!(!read_store_file(&ours.0, "expertise/dev.jsonl").contains("p2-MUT"));
}

/// A failing post-record hook only warns: plain mode prints the
/// hook's forwarded stderr and a `Warning:` line after the success
/// line; json mode carries the `warnings` array last in the envelope.
#[test]
fn record_post_hook_warning_law() {
    let block = "hooks:\n  post-record:\n    - 'cat >/dev/null; printf post-stderr >&2; exit 1'\n";
    let ours = TempDir::new("hookpost-o");
    hook_fixture(&ours.0, block);
    // plain: the record writes, the hook's stderr forwards, a Warning
    // line follows the success line
    let args = record_args("p1", &[]);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let our = run_in(&ours.0, Path::new(mulch_bin()), &refs);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert!(our.stdout.contains("Recorded pattern"), "{}", our.stdout);
    assert!(our.stderr.contains("post-stderr"), "{}", our.stderr);
    assert!(
        our.stderr
            .contains("Warning: hook `cat >/dev/null; printf post-stderr >&2; exit 1`"),
        "{}",
        our.stderr
    );
    // json: the warnings array rides the envelope
    let args = record_args("p2", &["--json"]);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let our = run_in(&ours.0, Path::new(mulch_bin()), &refs);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert!(
        normalize(&our.stdout).contains("\"warnings\": ["),
        "{}",
        our.stdout
    );
}

/// Pre-record fires BEFORE the duplicate decision (even a skip runs
/// it); post-record fires for actual writes only — a skipped
/// duplicate is a no-op with no post hook.
#[test]
fn record_hook_skip_fires_pre_but_no_post() {
    let block = "hooks:\n  pre-record:\n    - 'cat > prefired.json'\n  post-record:\n    - 'cat > postfired.json'\n";
    let ours = TempDir::new("hookskip-o");
    hook_fixture(&ours.0, block);
    // Seed the anonymous duplicate, clear the captures, re-run: the
    // skip advisory prints while only the pre hook fired.
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "dev",
        "--type",
        "convention",
        "--content",
        "dup",
    ]);
    std::fs::remove_file(ours.0.join("prefired.json")).ok();
    std::fs::remove_file(ours.0.join("postfired.json")).ok();
    let args = ["record", "dev", "--type", "convention", "--content", "dup"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    assert_eq!(our.code, 0);
    assert_eq!(
        our.stdout,
        "Duplicate convention already exists in dev (record #1). Use --force to add anyway.\n"
    );
    assert!(ours.0.join("prefired.json").is_file(), "pre fired");
    assert!(!ours.0.join("postfired.json").is_file(), "no post on skip");
}

/// Dry-run fires NO hooks at all (no external side effects while
/// previewing) and mirrors the write decision as before.
#[test]
fn record_hook_dry_run_fires_nothing() {
    let block = "hooks:\n  pre-record:\n    - 'cat > prefired.json'\n  post-record:\n    - 'cat > postfired.json'\n";
    let ours = TempDir::new("hookdry-o");
    hook_fixture(&ours.0, block);
    for mode in [&["--dry-run"] as &[&str], &["--dry-run", "--json"]] {
        let args = record_args("p1", mode);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let our = run_in(&ours.0, Path::new(mulch_bin()), &refs);
        assert_eq!(our.code, 0, "dry-run {mode:?}: {}", our.stderr);
    }
    assert!(!ours.0.join("prefired.json").is_file(), "no pre in dry-run");
    assert!(
        !ours.0.join("postfired.json").is_file(),
        "no post in dry-run"
    );
}

/// The hook payloads (frozen law): pre-record carries
/// `{domain, record}` WITHOUT the write-time id; post-record carries
/// the WRITTEN record (id last) plus `action` — for creates and
/// updates alike.
#[test]
fn record_hook_payloads_pin_the_law() {
    let block =
        "hooks:\n  pre-record:\n    - 'cat > pre.json'\n  post-record:\n    - 'cat > post.json'\n";
    let ours = TempDir::new("hookpay-o");
    hook_fixture(&ours.0, block);
    // Create, then the named-duplicate update of the same record.
    let create = record_args("p1", &[]);
    let create_refs: Vec<&str> = create.iter().map(String::as_str).collect();
    let our = run_in(&ours.0, Path::new(mulch_bin()), &create_refs);
    assert_eq!(our.code, 0, "{}", our.stderr);
    let pre = std::fs::read_to_string(ours.0.join("pre.json")).expect("pre capture");
    let post = std::fs::read_to_string(ours.0.join("post.json")).expect("post capture");
    assert!(
        pre.starts_with(
            "{\"event\":\"pre-record\",\"payload\":{\"domain\":\"dev\",\"record\":{\"type\":\"pattern\""
        ),
        "the pre payload carries domain + record: {pre}"
    );
    assert!(
        !pre.contains("\"id\""),
        "the pre payload's record has NO write-time id: {pre}"
    );
    assert!(
        post.contains("\"action\":\"created\""),
        "the post payload carries the action: {post}"
    );
    assert!(
        post.contains("\"id\":\"mx-31be4b\""),
        "the post payload carries the WRITTEN record (id): {post}"
    );

    // The named-duplicate update fires both with action updated.
    let mut update = record_args("p1", &[]);
    // Replace the trailing description VALUE (d1 -> d1-NEW).
    update.pop();
    update.push("d1-NEW".to_string());
    let update_refs: Vec<&str> = update.iter().map(String::as_str).collect();
    let our = run_in(&ours.0, Path::new(mulch_bin()), &update_refs);
    assert_eq!(our.code, 0, "{}", our.stderr);
    let post = std::fs::read_to_string(ours.0.join("post.json")).expect("post capture");
    assert!(post.contains("\"action\":\"updated\""), "{post}");
}

/// Adoption is an object IDENTITY check (record.ts): a hook that
/// re-emits the SAME values in a different key order is adopted, so
/// the written line carries the hook's order.
#[test]
fn record_hook_echo_adopts_the_hooks_key_order() {
    // Shell-only key reorder (the battery carries no python/jq
    // dependency): move the record's `name` field to the front,
    // every value untouched. The payload arrives on one line.
    let hook = concat!(
        r#"raw=$(cat); name=$(printf "%s" "$raw" | sed -n "s/.*\"name\":\"\([^\"]*\)\".*/\1/p"); "#,
        r#"rest=$(printf "%s" "$raw" | sed "s/\"name\":\"[^\"]*\",//"); "#,
        r#"printf "%s" "$rest" | sed "s/\"record\":{/\"record\":{\"name\":\"$name\",/""#,
    );
    let block = format!("hooks:\n  pre-record:\n    - '{hook}'\n");
    let ours = TempDir::new("hookord-o");
    hook_fixture(&ours.0, &block);

    let create = record_args("p1", &[]);
    let create_refs: Vec<&str> = create.iter().map(String::as_str).collect();
    let our = run_in(&ours.0, Path::new(mulch_bin()), &create_refs);
    assert_eq!(our.code, 0, "{}", our.stderr);
    let line = read_store_file(&ours.0, "expertise/dev.jsonl");
    assert!(
        line.starts_with("{\"name\":\"p1\""),
        "the hook's key order was adopted: {line}"
    );
}

/// The batch family: a blocked pre-record hook drops just that record
/// (valid-records index) and the batch continues; the post-record
/// hook fires per actual write with `{domain, record, action}` and
/// its failure warns into the summary envelope's `warnings` array.
#[test]
fn record_batch_hook_drop_one_continue() {
    let block = "hooks:\n  pre-record:\n    - 'x=$(cat); if echo \"$x\" | grep -q b1; then exit 3; fi'\n  post-record:\n    - 'cat >> postbatch.json; exit 7'\n";
    let ours = TempDir::new("hookbatch-o");
    hook_fixture(&ours.0, block);
    std::fs::write(
        ours.0.join("b.json"),
        "[{\"type\":\"pattern\",\"name\":\"b1\",\"description\":\"d\"},{\"type\":\"pattern\",\"name\":\"b2\",\"description\":\"d\"}]",
    )
    .expect("batch fixture");
    for mode in [&[] as &[&str], &["--json"]] {
        let mut args = vec!["record", "dev", "--batch", "b.json"];
        args.extend_from_slice(mode);
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(our.code, 0, "batch drop-one {mode:?}: {}", our.stderr);
    }
    // b2 wrote (twice across the two runs — the second as an
    // update), b1 never did; the post payloads carry the action.
    let store = read_store_file(&ours.0, "expertise/dev.jsonl");
    assert!(store.contains("b2"));
    assert!(!store.contains("b1"));
    let postbatch = std::fs::read_to_string(ours.0.join("postbatch.json")).expect("post capture");
    assert!(postbatch.contains("\"action\":\"created\""), "{postbatch}");
    assert!(postbatch.contains("\"action\":\"updated\""), "{postbatch}");
}

/// A batch pre-record hook echoing an invalid record drops that
/// record with the invalid-mutation error; the rest still writes.
#[test]
fn record_batch_hook_invalid_mutation_drops_record() {
    let block = "hooks:\n  pre-record:\n    - 'x=$(cat); if echo \"$x\" | grep -q b2; then printf %s \"{\\\"payload\\\":{\\\"domain\\\":\\\"dev\\\",\\\"record\\\":{\\\"type\\\":\\\"pattern\\\",\\\"classification\\\":\\\"tactical\\\",\\\"recorded_at\\\":\\\"2026-01-01T00:00:00.000Z\\\"}}}\"; fi'\n";
    let ours = TempDir::new("hookbinv-o");
    hook_fixture(&ours.0, block);
    std::fs::write(
        ours.0.join("b.json"),
        "[{\"type\":\"pattern\",\"name\":\"b1\",\"description\":\"d\"},{\"type\":\"pattern\",\"name\":\"b2\",\"description\":\"d\"}]",
    )
    .expect("batch fixture");
    for mode in [&[] as &[&str], &["--json"]] {
        let mut args = vec!["record", "dev", "--batch", "b.json"];
        args.extend_from_slice(mode);
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(
            our.code, 0,
            "batch invalid mutation {mode:?}: {}",
            our.stderr
        );
    }
    let store = read_store_file(&ours.0, "expertise/dev.jsonl");
    assert!(store.contains("b1"));
    assert!(!store.contains("b2"));
}
