//! `mulch record`/`add` flag path, stdin, batch, and echo parity probes.
//!
//! Moved verbatim from the one-file battery (mulch-3e7c); sprint
//! banners stay with their probes. The nested `battery_s23_b` group
//! keeps its sprint-23 trio together — one of its pins is an EDIT
//! surface (payload append order); it stayed with its lane group.

use std::path::Path;

use super::support::*;

#[test]
fn batch_mixed_partial_writes_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("mix-ours");
    let theirs = TempDir::new("mix-theirs");
    let fixture = |dir: &Path| {
        let _ = run_in(dir, &ml, &["init"]);
        let _ = run_in(dir, &ml, &[
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
            dir.join("b.json"),
            "[{\"type\":\"pattern\",\"name\":\"p1\",\"description\":\"d1-NEW\",\"classification\":\"tactical\"},{\"type\":\"pattern\",\"name\":\"bad\",\"id\":123},{\"type\":\"convention\",\"content\":\"fresh\"}]",
        )
        .expect("batch fixture");
        // all-invalid: nothing writes, exit 1 in every mode
        std::fs::write(
            dir.join("bad.json"),
            "[{\"type\":\"pattern\",\"name\":\"bad\",\"id\":123},{\"type\":\"convention\",\"content\":5}]",
        )
        .expect("bad fixture");
    };
    fixture(&ours.0);
    fixture(&theirs.0);

    for file in ["b.json", "bad.json"] {
        for mode in [&[] as &[&str], &["--json"], &["--dry-run"], &[
            "--dry-run",
            "--json",
        ]] {
            let mut args = vec!["record", "dev", "--batch", file];
            args.extend_from_slice(mode);
            let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
            let theirs_run = run_in(&theirs.0, &ml, &args);
            assert_eq!(ours_run.code, theirs_run.code, "exit {file} {mode:?}");
            assert_eq!(ours_run.stdout, theirs_run.stdout, "stdout {file} {mode:?}");
            assert_eq!(ours_run.stderr, theirs_run.stderr, "stderr {file} {mode:?}");
        }
    }
    // partial-write semantics landed in the store: the update replaced
    // p1 in place, the create appended (2 lines; the invalid record is
    // not on disk) — across all four b.json runs the second pass sees
    // p1-NEW as the duplicate (update again) and "fresh" as an
    // anonymous skip.
    assert_eq!(
        normalize(&read_store_file(&ours.0, "expertise/dev.jsonl")),
        normalize(&read_store_file(&theirs.0, "expertise/dev.jsonl"))
    );
    let store = read_store_file(&ours.0, "expertise/dev.jsonl");
    assert_eq!(store.lines().count(), 2);
    assert!(store.contains("d1-NEW"));
    assert!(store.contains("fresh"));
}

#[test]
fn nested_subschema_errors_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("nested-ours");
    let theirs = TempDir::new("nested-theirs");
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
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let _ = run_in(dir, &ml, &["add", "dev"]);
        std::fs::write(dir.join(".mulch/expertise/dev.jsonl"), &store).expect("store fixture");
    }

    for mode in [&[] as &[&str], &["--json"]] {
        let mut args = vec!["validate"];
        args.extend_from_slice(mode);
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
        let theirs_run = run_in(&theirs.0, &ml, &args);
        assert_eq!(ours_run.code, theirs_run.code, "exit {mode:?}");
        assert_eq!(ours_run.stdout, theirs_run.stdout, "stdout {mode:?}");
        assert_eq!(ours_run.stderr, theirs_run.stderr, "stderr {mode:?}");
    }

    // the batch surface carries the same nested entries per record
    let batch = format!(
        "[{{\"type\":\"pattern\",\"name\":\"b1\",\"description\":\"d\"{common},\"outcomes\":[{{\"status\":\"nope\"}}]}}]"
    );
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(dir.join("nb.json"), &batch).expect("nested batch");
    }
    for mode in [&[] as &[&str], &["--json"]] {
        let mut args = vec!["record", "dev", "--batch", "nb.json"];
        args.extend_from_slice(mode);
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
        let theirs_run = run_in(&theirs.0, &ml, &args);
        assert_eq!(ours_run.code, theirs_run.code, "batch exit {mode:?}");
        assert_eq!(ours_run.stdout, theirs_run.stdout, "batch stdout {mode:?}");
        assert_eq!(ours_run.stderr, theirs_run.stderr, "batch stderr {mode:?}");
    }
}

// ---- sprint 18 (mulch-f9b9): staleness verdict seam — missing/unknown
// classification never decays, the (shelf+1)-day boundary, fix-report
// check order ----

#[test]
fn quiet_matrix_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("quiet-ours");
    let theirs = TempDir::new("quiet-theirs");
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

    // validate --quiet still prints the summary
    let ours_val = run_in(&ours.0, Path::new(mulch_bin()), &["validate", "--quiet"]);
    let theirs_val = run_in(&theirs.0, &ml, &["validate", "--quiet"]);
    assert_eq!(ours_val.code, theirs_val.code);
    assert_eq!(ours_val.stdout, theirs_val.stdout);

    // doctor --quiet prints NOTHING (plain mode)
    let ours_doc = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--quiet"]);
    let theirs_doc = run_in(&theirs.0, &ml, &["doctor", "--quiet"]);
    assert_eq!(ours_doc.code, theirs_doc.code);
    assert_eq!(ours_doc.stdout, theirs_doc.stdout);
    assert_eq!(ours_doc.stdout, "");
}

#[test]
fn record_all_types_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let cases: [(&str, &[&str]); 6] = [
        ("convention", &["--content", "cc"]),
        ("pattern", &["--name", "nn", "--description", "dd"]),
        ("failure", &["--description", "dd", "--resolution", "rr"]),
        ("decision", &["--title", "tt", "--rationale", "rr"]),
        ("reference", &["--name", "nn", "--description", "dd"]),
        ("guide", &["--name", "nn", "--description", "dd"]),
    ];
    for (index, (kind, payload)) in cases.iter().enumerate() {
        let ours = TempDir::new(format!("rec{index}-ours").as_str());
        let theirs = TempDir::new(format!("rec{index}-theirs").as_str());
        for dir in [&ours.0, &theirs.0] {
            let _ = run_in(dir, &ml, &["init"]);
        }
        let mut args = vec!["record", "d", "--type", kind];
        args.extend_from_slice(payload);
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
        let theirs_run = run_in(&theirs.0, &ml, &args);
        assert_eq!(ours_run.code, theirs_run.code, "{kind} exit");
        assert_eq!(ours_run.stdout, theirs_run.stdout, "{kind} stdout");
        assert_eq!(
            normalize_line(&read_store_file(&ours.0, "expertise/d.jsonl")),
            normalize_line(&read_store_file(&theirs.0, "expertise/d.jsonl")),
            "{kind} jsonl"
        );
        // domain registered in config on both sides
        assert!(read_store_file(&ours.0, "mulch.config.yaml").contains("d: {}"));
    }
}

#[test]
fn record_optional_flags_line_matches() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("optf-ours");
    let theirs = TempDir::new("optf-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
    }
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
    let theirs_run = run_in(&theirs.0, &ml, &args);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);
    assert_eq!(
        normalize_line(&read_store_file(&ours.0, "expertise/d.jsonl")),
        normalize_line(&read_store_file(&theirs.0, "expertise/d.jsonl"))
    );
}

#[test]
fn add_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("add-ours");
    let theirs = TempDir::new("add-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
    }
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["add", "cli"]);
    let theirs_run = run_in(&theirs.0, &ml, &["add", "cli"]);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);
    assert_eq!(
        read_store_file(&ours.0, "mulch.config.yaml"),
        read_store_file(&theirs.0, "mulch.config.yaml")
    );
    assert_eq!(read_store_file(&ours.0, "expertise/cli.jsonl"), "");

    // existing domain
    let ours_dup = run_in(&ours.0, Path::new(mulch_bin()), &["add", "cli"]);
    let theirs_dup = run_in(&theirs.0, &ml, &["add", "cli"]);
    assert_eq!(ours_dup.code, theirs_dup.code);
    assert_eq!(ours_dup.stdout, theirs_dup.stdout);
    assert_eq!(ours_dup.stderr, theirs_dup.stderr);
}

#[test]
fn record_auto_create_failure_side_effects_match() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("autofail-ours");
    let theirs = TempDir::new("autofail-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
    }
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
    let theirs_run = run_in(&theirs.0, &ml, &args);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);
    // the Retry echo names this binary (README DEVIATIONS) —
    // normalized, pinned by record_echo_promotes_mulch
    assert_eq!(echo_ours_to_reference(&ours_run.stderr), theirs_run.stderr);
    // the domain side effect persists on both sides
    assert!(read_store_file(&ours.0, "mulch.config.yaml").contains("devfail: {}"));
    assert!(read_store_file(&theirs.0, "mulch.config.yaml").contains("devfail: {}"));
}

#[test]
fn duplicate_and_force_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("dup-ours");
    let theirs = TempDir::new("dup-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let _ = run_in(dir, &ml, &["add", "d"]);
        let _ = run_in(dir, &ml, &[
            "record",
            "d",
            "--type",
            "convention",
            "--content",
            "c1",
        ]);
    }
    let dup = ["record", "d", "--type", "convention", "--content", "c1"];
    let ours_dup = run_in(&ours.0, Path::new(mulch_bin()), &dup);
    let theirs_dup = run_in(&theirs.0, &ml, &dup);
    assert_eq!(ours_dup.code, theirs_dup.code);
    assert_eq!(ours_dup.stdout, theirs_dup.stdout);

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
    let theirs_force = run_in(&theirs.0, &ml, &forced);
    assert_eq!(ours_force.code, theirs_force.code);
    assert_eq!(ours_force.stdout, theirs_force.stdout);
    assert_eq!(
        normalize_line(&read_store_file(&ours.0, "expertise/d.jsonl")),
        normalize_line(&read_store_file(&theirs.0, "expertise/d.jsonl"))
    );
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
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("dry-ours");
    let _ = run_in(&ours.0, &ml, &["init"]);
    let _ = run_in(&ours.0, &ml, &["add", "d"]);
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
fn record_stdin_matches_reference() {
    use std::io::Write as _;
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("stdin-ours");
    let theirs = TempDir::new("stdin-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let _ = run_in(dir, &ml, &["add", "d"]);
    }
    let payload = r#"{"type":"convention","content":"stdin-conv-1"}"#;
    for (dir, program) in [(&ours.0, Path::new(mulch_bin())), (&theirs.0, &ml)] {
        let mut child = std::process::Command::new(program)
            .args(["record", "d", "--stdin"])
            .current_dir(dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn");
        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(payload.as_bytes())
            .expect("write");
        let output = child.wait_with_output().expect("wait");
        assert!(output.status.success());
    }
    assert_eq!(
        normalize_line(&read_store_file(&ours.0, "expertise/d.jsonl")),
        normalize_line(&read_store_file(&theirs.0, "expertise/d.jsonl"))
    );
    // input key order preserved: type before content before recorded_at
    let line = read_store_file(&ours.0, "expertise/d.jsonl");
    assert!(line.starts_with(r#"{"type":"convention","content":"stdin-conv-1","recorded_at""#));
}

// ---- sprint 6 (mulch-ccf6): record upsert semantics ----

#[test]
fn record_upsert_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("ccf6-ours");
    let theirs = TempDir::new("ccf6-theirs");
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
            "v1",
        ]);
    }
    let id = pattern_p_id();
    let compare = |ours: (&Run, String), theirs: (&Run, String)| {
        assert_eq!(ours.0.code, theirs.0.code);
        assert_eq!(normalize(&ours.0.stdout), normalize(&theirs.0.stdout));
        assert_eq!(normalize(&ours.0.stderr), normalize(&theirs.0.stderr));
        assert_eq!(
            normalize_line(&ours.1),
            normalize_line(&theirs.1),
            "store diverged"
        );
    };

    // named duplicate upserts in place
    let up = [
        "record",
        "d",
        "--type",
        "pattern",
        "--name",
        "p",
        "--description",
        "v2",
    ];
    compare(
        (
            &run_in(&ours.0, Path::new(mulch_bin()), &up),
            read_store_file(&ours.0, "expertise/d.jsonl"),
        ),
        (
            &run_in(&theirs.0, &ml, &up),
            read_store_file(&theirs.0, "expertise/d.jsonl"),
        ),
    );

    // json updated envelope + outcomes carried into the upsert
    let up_outcome = [
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
    ];
    compare(
        (
            &run_in(&ours.0, Path::new(mulch_bin()), &up_outcome),
            read_store_file(&ours.0, "expertise/d.jsonl"),
        ),
        (
            &run_in(&theirs.0, &ml, &up_outcome),
            read_store_file(&theirs.0, "expertise/d.jsonl"),
        ),
    );

    // second outcome merges after the existing one
    let up_outcome2 = [
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
    ];
    compare(
        (
            &run_in(&ours.0, Path::new(mulch_bin()), &up_outcome2),
            read_store_file(&ours.0, "expertise/d.jsonl"),
        ),
        (
            &run_in(&theirs.0, &ml, &up_outcome2),
            read_store_file(&theirs.0, "expertise/d.jsonl"),
        ),
    );

    // anonymous duplicate skips with the advisory
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--type",
        "convention",
        "--content",
        "same",
    ]);
    let _ = run_in(&theirs.0, &ml, &[
        "record",
        "d",
        "--type",
        "convention",
        "--content",
        "same",
    ]);
    let anon = ["record", "d", "--type", "convention", "--content", "same"];
    compare(
        (
            &run_in(&ours.0, Path::new(mulch_bin()), &anon),
            read_store_file(&ours.0, "expertise/d.jsonl"),
        ),
        (
            &run_in(&theirs.0, &ml, &anon),
            read_store_file(&theirs.0, "expertise/d.jsonl"),
        ),
    );

    // json skipped envelope carries the record number
    let anon_json = [
        "--json",
        "record",
        "d",
        "--type",
        "convention",
        "--content",
        "same",
    ];
    compare(
        (
            &run_in(&ours.0, Path::new(mulch_bin()), &anon_json),
            read_store_file(&ours.0, "expertise/d.jsonl"),
        ),
        (
            &run_in(&theirs.0, &ml, &anon_json),
            read_store_file(&theirs.0, "expertise/d.jsonl"),
        ),
    );

    // --force appends anyway
    let force = [
        "record",
        "d",
        "--type",
        "convention",
        "--content",
        "same",
        "--force",
    ];
    compare(
        (
            &run_in(&ours.0, Path::new(mulch_bin()), &force),
            read_store_file(&ours.0, "expertise/d.jsonl"),
        ),
        (
            &run_in(&theirs.0, &ml, &force),
            read_store_file(&theirs.0, "expertise/d.jsonl"),
        ),
    );

    // dry-run mirrors the decision without writing (plain + json)
    let dry_named = [
        "record",
        "d",
        "--type",
        "pattern",
        "--name",
        "p",
        "--description",
        "v9",
        "--dry-run",
    ];
    compare(
        (
            &run_in(&ours.0, Path::new(mulch_bin()), &dry_named),
            read_store_file(&ours.0, "expertise/d.jsonl"),
        ),
        (
            &run_in(&theirs.0, &ml, &dry_named),
            read_store_file(&theirs.0, "expertise/d.jsonl"),
        ),
    );
    let dry_json = [
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
    ];
    compare(
        (
            &run_in(&ours.0, Path::new(mulch_bin()), &dry_json),
            read_store_file(&ours.0, "expertise/d.jsonl"),
        ),
        (
            &run_in(&theirs.0, &ml, &dry_json),
            read_store_file(&theirs.0, "expertise/d.jsonl"),
        ),
    );

    // a renamed record still dedupes (dedup field, not id) and the
    // upsert corrects the stale id in place
    let rename = ["edit", "d", &id, "--name", "p2"];
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &rename);
    let _ = run_in(&theirs.0, &ml, &rename);
    let renamed_up = [
        "record",
        "d",
        "--type",
        "pattern",
        "--name",
        "p2",
        "--description",
        "v10",
    ];
    compare(
        (
            &run_in(&ours.0, Path::new(mulch_bin()), &renamed_up),
            read_store_file(&ours.0, "expertise/d.jsonl"),
        ),
        (
            &run_in(&theirs.0, &ml, &renamed_up),
            read_store_file(&theirs.0, "expertise/d.jsonl"),
        ),
    );

    // batch: one new guide + one named duplicate -> created 1, updated 1
    let batch = serde_json::json!([
        {"type": "guide", "name": "g1", "description": "gd"},
        {"type": "pattern", "name": "p2", "description": "batch-v11"}
    ]);
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(dir.join("batch.json"), format!("{batch}")).expect("batch file");
    }
    let run_batch = ["record", "d", "--batch", "batch.json", "--json"];
    compare(
        (
            &run_in(&ours.0, Path::new(mulch_bin()), &run_batch),
            read_store_file(&ours.0, "expertise/d.jsonl"),
        ),
        (
            &run_in(&theirs.0, &ml, &run_batch),
            read_store_file(&theirs.0, "expertise/d.jsonl"),
        ),
    );

    // within-batch duplicate in dry-run counts as another create
    // (dedupe runs against the store only, never the batch)
    let batch2 = serde_json::json!([
        {"type": "pattern", "name": "q", "description": "q1"},
        {"type": "pattern", "name": "q", "description": "q2"}
    ]);
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(dir.join("batch2.json"), format!("{batch2}")).expect("batch file");
    }
    let run_batch2 = [
        "record",
        "d",
        "--batch",
        "batch2.json",
        "--json",
        "--dry-run",
    ];
    compare(
        (
            &run_in(&ours.0, Path::new(mulch_bin()), &run_batch2),
            read_store_file(&ours.0, "expertise/d.jsonl"),
        ),
        (
            &run_in(&theirs.0, &ml, &run_batch2),
            read_store_file(&theirs.0, "expertise/d.jsonl"),
        ),
    );

    // non-dry within-batch duplicate upserts the batch's own record
    let run_batch3 = ["record", "d", "--batch", "batch2.json", "--json"];
    compare(
        (
            &run_in(&ours.0, Path::new(mulch_bin()), &run_batch3),
            read_store_file(&ours.0, "expertise/d.jsonl"),
        ),
        (
            &run_in(&theirs.0, &ml, &run_batch3),
            read_store_file(&theirs.0, "expertise/d.jsonl"),
        ),
    );

    // an explicit batch id survives upsert AND create (generated ids
    // only fill gaps — spec-review blocker F1)
    let batch3 = serde_json::json!([
        {"type": "pattern", "name": "p2", "description": "keep-id", "id": "mx-abcd1234"},
        {"type": "guide", "name": "g2", "description": "gd2", "id": "mx-feed1234"}
    ]);
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(dir.join("batch3.json"), format!("{batch3}")).expect("batch file");
    }
    let run_batch4 = ["record", "d", "--batch", "batch3.json", "--json"];
    compare(
        (
            &run_in(&ours.0, Path::new(mulch_bin()), &run_batch4),
            read_store_file(&ours.0, "expertise/d.jsonl"),
        ),
        (
            &run_in(&theirs.0, &ml, &run_batch4),
            read_store_file(&theirs.0, "expertise/d.jsonl"),
        ),
    );

    // flag-path key order: outcomes precede dir_anchors on the
    // rewritten line (spec-review minor F2)
    let anchored = [
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
    ];
    compare(
        (
            &run_in(&ours.0, Path::new(mulch_bin()), &anchored),
            read_store_file(&ours.0, "expertise/d.jsonl"),
        ),
        (
            &run_in(&theirs.0, &ml, &anchored),
            read_store_file(&theirs.0, "expertise/d.jsonl"),
        ),
    );
}

// ---- sprint 2 review round: json error channels, stdin/batch matrix ----

#[test]
fn record_invalid_ref_matches_reference_blob() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("iref-ours");
    let theirs = TempDir::new("iref-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
    }
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
    let theirs_run = run_in(&theirs.0, &ml, &args);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);
    assert_eq!(ours_run.stderr, theirs_run.stderr);
}

#[test]
fn record_missing_flags_json_error_text() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("mfj-ours");
    let theirs = TempDir::new("mfj-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let _ = run_in(dir, &ml, &["add", "d"]);
    }
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
    let theirs_run = run_in(&theirs.0, &ml, &args);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);
    assert_eq!(echo_ours_to_reference(&ours_run.stderr), theirs_run.stderr);
}

#[test]
fn stdin_invalid_record_envelope_and_untouched_store() {
    use std::io::Write as _;
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("sinv-ours");
    let theirs = TempDir::new("sinv-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let _ = run_in(dir, &ml, &["add", "d"]);
    }
    let payload = r#"{"type":"pattern","name":"pn"}"#;
    for (dir, program) in [(&ours.0, Path::new(mulch_bin())), (&theirs.0, &ml)] {
        let mut child = std::process::Command::new(program)
            .args(["record", "d", "--stdin", "--json"])
            .current_dir(dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn");
        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(payload.as_bytes())
            .expect("write");
        let output = child.wait_with_output().expect("wait");
        assert_eq!(output.status.code(), Some(1));
    }
    assert_eq!(read_store_file(&ours.0, "expertise/d.jsonl"), "");
    assert_eq!(read_store_file(&theirs.0, "expertise/d.jsonl"), "");
}

#[test]
fn batch_dry_run_writes_nothing() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("bdry-ours");
    let theirs = TempDir::new("bdry-theirs");
    let batch = r#"[{"type":"convention","content":"b1"},{"type":"convention","content":"b2"}]"#;
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        let _ = run_in(dir, &ml, &["add", "d"]);
        std::fs::write(dir.join("b.json"), batch).expect("batch file");
    }
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--batch",
        "b.json",
        "--dry-run",
    ]);
    let theirs_run = run_in(&theirs.0, &ml, &[
        "record",
        "d",
        "--batch",
        "b.json",
        "--dry-run",
    ]);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);
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
fn batch_summary_surfaces_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    // canonical store: a pattern (named-dup target) + a convention
    // (anon-dup target)
    let seed = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"name\":\"p\",\"description\":\"d\",\"id\":\"mx-f16294\"}\n{\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-04T08:00:00.000Z\",\"content\":\"same\",\"id\":\"mx-ef8fb3\"}\n";
    // batch: 2 creates + 1 named dup (update) + 1 anon dup (skip)
    let mixed = "[{\"type\":\"guide\",\"name\":\"g1\",\"description\":\"gd\"},{\"type\":\"failure\",\"description\":\"f1\",\"resolution\":\"r1\"},{\"type\":\"pattern\",\"name\":\"p\",\"description\":\"up\"},{\"type\":\"convention\",\"content\":\"same\"}]";
    let empty = "[]";

    // dry-run plain: Would process counts created+updated; Create/
    // Update/Skip lines only when non-zero
    let (ours, theirs) = twin_seeded("bs-dry", None, &[("d", seed)], &[]);
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(dir.join("mixed.json"), mixed).expect("batch file");
    }
    let args = ["record", "d", "--batch", "mixed.json", "--dry-run"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, their.code);
    assert_eq!(our.stdout, their.stdout, "dry-run summary lines");
    assert_eq!(
        our.stdout,
        "✓ Dry-run complete. Would process 3 record(s) in d:\n  Create: 2\n  Update: 1\n  Skip: 1\n  Run without --dry-run to apply changes.\n"
    );

    // normal plain: created/updated/skipped lines, store identical
    let (ours, theirs) = twin_seeded("bs-run", None, &[("d", seed)], &[]);
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(dir.join("mixed.json"), mixed).expect("batch file");
    }
    let args = ["record", "d", "--batch", "mixed.json"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_in(&theirs.0, &ml, &args);
    assert_eq!(our.code, their.code);
    assert_eq!(our.stdout, their.stdout, "normal summary lines");
    assert_eq!(
        our.stdout,
        "✓ Created 2 record(s) in d\n✓ Updated 1 record(s) in d\nSkipped 1 duplicate(s) in d\n"
    );
    assert_eq!(
        normalize_line(&read_store_file(&ours.0, "expertise/d.jsonl")),
        normalize_line(&read_store_file(&theirs.0, "expertise/d.jsonl"))
    );

    // all-duplicates dry-run: Would process 0 with only a Skip line
    let (ours, theirs) = twin_seeded("bs-alldup", None, &[("d", seed)], &[]);
    let alldup = "[{\"type\":\"convention\",\"content\":\"same\"},{\"type\":\"pattern\",\"name\":\"p\",\"description\":\"again\"}]";
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(dir.join("alldup.json"), alldup).expect("batch file");
    }
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--batch",
        "alldup.json",
        "--dry-run",
    ]);
    let their = run_in(&theirs.0, &ml, &[
        "record",
        "d",
        "--batch",
        "alldup.json",
        "--dry-run",
    ]);
    assert_eq!(our.code, their.code);
    assert_eq!(our.stdout, their.stdout, "all-duplicates dry-run");
    assert_eq!(
        our.stdout,
        "✓ Dry-run complete. Would process 1 record(s) in d:\n  Update: 1\n  Skip: 1\n  Run without --dry-run to apply changes.\n"
    );

    // empty batch: nothing in normal mode, the notice in dry-run
    let (ours, theirs) = twin_seeded("bs-empty", None, &[("d", seed)], &[]);
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(dir.join("empty.json"), empty).expect("batch file");
    }
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--batch",
        "empty.json",
    ]);
    let their = run_in(&theirs.0, &ml, &["record", "d", "--batch", "empty.json"]);
    assert_eq!(our.code, their.code);
    assert_eq!(our.stdout, their.stdout);
    assert_eq!(our.stdout, "", "zero batch prints nothing");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--batch",
        "empty.json",
        "--dry-run",
    ]);
    let their = run_in(&theirs.0, &ml, &[
        "record",
        "d",
        "--batch",
        "empty.json",
        "--dry-run",
    ]);
    assert_eq!(our.stdout, their.stdout);
    assert_eq!(our.stdout, "No records would be processed.\n");

    let commented = format!("# banner\n\n{seed}\n\n");

    // skip-only batch: the reference does NOT write (guard
    // `created > 0 || updated > 0`), so comments survive untouched
    let (ours, theirs) = twin_seeded("bs-skiponly", None, &[("d", &commented)], &[]);
    let skiponly = "[{\"type\":\"convention\",\"content\":\"same\"}]";
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(dir.join("skiponly.json"), skiponly).expect("batch file");
    }
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record",
        "d",
        "--batch",
        "skiponly.json",
    ]);
    let their = run_in(&theirs.0, &ml, &["record", "d", "--batch", "skiponly.json"]);
    assert_eq!(our.code, their.code);
    assert_eq!(our.stdout, their.stdout, "skip-only summary");
    assert_eq!(our.stdout, "Skipped 1 duplicate(s) in d\n");
    assert_eq!(
        read_store_file(&ours.0, "expertise/d.jsonl"),
        commented,
        "skip-only batch must not write"
    );

    // create-only batch on a commented store rewrites compactly on
    // both sides (comments and blank lines drop)
    let (ours, theirs) = twin_seeded("bs-compact", None, &[("d", &commented)], &[]);
    let one = "[{\"type\":\"guide\",\"name\":\"g2\",\"description\":\"gd2\"}]";
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(dir.join("one.json"), one).expect("batch file");
    }
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record", "d", "--batch", "one.json",
    ]);
    let their = run_in(&theirs.0, &ml, &["record", "d", "--batch", "one.json"]);
    assert_eq!(our.code, their.code);
    assert_eq!(our.stdout, their.stdout);
    let ours_store = read_store_file(&ours.0, "expertise/d.jsonl");
    let theirs_store = read_store_file(&theirs.0, "expertise/d.jsonl");
    assert!(!ours_store.contains("# banner"), "comments drop");
    assert_eq!(
        normalize_line(&ours_store),
        normalize_line(&theirs_store),
        "compact rewrite identical"
    );
}

// ---------------------------------------------------------------------------
// Sprint 23 lane B proposals (mulch-4ee2 / mulch-3368-B): three
// differential pins for the payload<->flag adapter surfaces. The
// flag-table coverage itself lives as unit pins on PayloadFlags
// (commands/mod.rs payload_flags_tests) — these pin the OBSERVABLE
// differential behavior the tables feed: the plain missing-flags
// echo, record's content precedence, and edit's payload append order.
// ---------------------------------------------------------------------------

mod battery_s23_b {
    use super::*;

    /// Plain-mode missing-flags failure: the Retry echo mixes
    /// provided flag values with `<placeholder>` pairs for the
    /// missing ones (the json variant is pinned by
    /// record_missing_flags_json_error_text; this pins the plain
    /// channel, values and placeholders side by side).
    #[test]
    fn record_missing_flags_plain_echo_matches_reference() {
        let Some(ml) = reference_ml() else {
            eprintln!("skipped: no ml on PATH");
            return;
        };
        for (tag, args) in [
            ("placeholders only", vec![
                "record", "d", "--type", "pattern",
            ]),
            ("values and placeholders", vec![
                "record", "d", "--type", "decision", "--title", "T v", "--name", "n",
            ]),
        ] {
            let ours = TempDir::new("b1-ours");
            let theirs = TempDir::new("b1-theirs");
            for dir in [&ours.0, &theirs.0] {
                let _ = run_in(dir, &ml, &["init"]);
                let _ = run_in(dir, &ml, &["add", "d"]);
            }
            let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
            let theirs_run = run_in(&theirs.0, &ml, &args);
            assert_eq!(ours_run.code, theirs_run.code, "{tag} exit");
            assert_eq!(ours_run.stdout, theirs_run.stdout, "{tag} stdout");
            assert_eq!(
                echo_ours_to_reference(&ours_run.stderr),
                theirs_run.stderr,
                "{tag} stderr (echo names this binary — README DEVIATIONS)"
            );
            assert_eq!(
                read_store_file(&ours.0, "expertise/d.jsonl"),
                read_store_file(&theirs.0, "expertise/d.jsonl"),
                "{tag} store"
            );
        }
    }

    /// record's content law: `--content` wins over the positional
    /// shorthand (the adapter's one divergent row, pinned end to end:
    /// store bytes and ids keyed on the winning value).
    #[test]
    fn record_content_flag_wins_over_positional_matches_reference() {
        let Some(ml) = reference_ml() else {
            eprintln!("skipped: no ml on PATH");
            return;
        };
        let ours = TempDir::new("b2-ours");
        let theirs = TempDir::new("b2-theirs");
        for dir in [&ours.0, &theirs.0] {
            let _ = run_in(dir, &ml, &["init"]);
            let _ = run_in(dir, &ml, &["add", "d"]);
        }
        let args: [&str; 6] = [
            "record",
            "d",
            "--type",
            "convention",
            "positional-value",
            "--content",
        ];
        // (append the value separately so both sides share args)
        let both = {
            let mut v = args.to_vec();
            v.push("flag-value");
            v
        };
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &both);
        let theirs_run = run_in(&theirs.0, &ml, &both);
        assert_eq!(ours_run.code, theirs_run.code);
        assert_eq!(ours_run.stdout, theirs_run.stdout);
        assert_eq!(ours_run.stderr, theirs_run.stderr);
        assert_eq!(
            normalize(&read_store_file(&ours.0, "expertise/d.jsonl")),
            normalize(&read_store_file(&theirs.0, "expertise/d.jsonl")),
        );
        assert!(read_store_file(&ours.0, "expertise/d.jsonl").contains("flag-value"));

        let positional = ["record", "d", "--type", "convention", "only-positional"];
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &positional);
        let theirs_run = run_in(&theirs.0, &ml, &positional);
        assert_eq!(ours_run.code, theirs_run.code);
        assert!(read_store_file(&theirs.0, "expertise/d.jsonl").contains("only-positional"));
        assert_eq!(
            normalize(&read_store_file(&ours.0, "expertise/d.jsonl")),
            normalize(&read_store_file(&theirs.0, "expertise/d.jsonl")),
        );
    }

    /// edit appends a record's MISSING payload keys in the type's
    /// payload order (the adapter iterates the registry row, not a
    /// flag-table order) and drops payload flags the type does not
    /// declare; the id is never recomputed.
    #[test]
    fn edit_payload_appends_missing_keys_in_payload_order_matches_reference() {
        let Some(ml) = reference_ml() else {
            eprintln!("skipped: no ml on PATH");
            return;
        };
        // The id both sides resolve (ids are content hashes, equal in
        // both stores) — captured from ours, then the payload keys are
        // stripped surgically in both.
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
        let theirs = TempDir::new("b3-theirs");
        for dir in [&ours.0, &theirs.0] {
            let _ = run_in(dir, &ml, &["init"]);
            let _ = run_in(dir, &ml, &["add", "d"]);
            let _ = run_in(dir, &ml, &[
                "record",
                "d",
                "--type",
                "failure",
                "--description",
                "D",
                "--resolution",
                "R",
            ]);
        }
        let id = strip(&ours.0);
        let _ = strip(&theirs.0);

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
        let theirs_run = run_in(&theirs.0, &ml, &append);
        assert_eq!(ours_run.code, theirs_run.code);
        assert_eq!(ours_run.stdout, theirs_run.stdout);
        assert_eq!(
            normalize(&read_store_file(&ours.0, "expertise/d.jsonl")),
            normalize(&read_store_file(&theirs.0, "expertise/d.jsonl")),
        );
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
        let theirs_run = run_in(&theirs.0, &ml, &stray);
        assert_eq!(ours_run.code, theirs_run.code);
        assert_eq!(
            normalize(&read_store_file(&ours.0, "expertise/d.jsonl")),
            normalize(&read_store_file(&theirs.0, "expertise/d.jsonl")),
        );
        assert!(!read_store_file(&ours.0, "expertise/d.jsonl").contains("\"content\""));
    }
}

// mulch-dd36 battery: doctor's governance check computes over the
// effective thresholds — overruns (fail at hard_limit), warn-only
// stores keep exit 0, partial governance blocks backfill per field.

/// mulch-cbe2: the flag path REQUIRES `--type` — after the
/// auto-create side effect, exit 1, nothing written, in plain and
/// json (the envelope rides stderr beside the stdout auto-create
/// line).
#[test]
fn record_flag_path_requires_type_after_auto_create() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("cbe2-ours");
    let theirs = TempDir::new("cbe2-theirs");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let _ = run_in(&theirs.0, &ml, &["init"]);

    for mode in [&[] as &[&str], &["--json"]] {
        let mut args = mode.to_vec();
        args.extend_from_slice(&["record", "d", "--name", "n", "--description", "d"]);
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
        let theirs_run = run_in(&theirs.0, &ml, &args);
        assert_eq!(ours_run.code, 1, "exit is 1 {mode:?}");
        assert_eq!(ours_run.code, theirs_run.code, "exit differs {mode:?}");
        assert_eq!(
            ours_run.stdout, theirs_run.stdout,
            "stdout differs {mode:?}"
        );
        assert_eq!(
            ours_run.stderr, theirs_run.stderr,
            "stderr differs {mode:?}"
        );
        assert!(ours_run.stderr.contains(
            "--type is required (convention, pattern, failure, decision, reference, guide)"
        ));
        // the side effect happened, the record did not: the domain
        // file registers as an EMPTY line container in both
        assert!(has_domain(&ours.0, "d"));
        assert!(has_domain(&theirs.0, "d"));
        assert_eq!(read_store_file(&ours.0, "expertise/d.jsonl"), "");
        assert_eq!(read_store_file(&theirs.0, "expertise/d.jsonl"), "");
    }
}

/// mulch-e2cf: the positional `[content]` is a type-specific fallback
/// (convention→content, pattern/reference/guide→description,
/// failure/decision→dropped) — and it satisfies the missing-flags
/// check while the Retry echo still renders it as a bare argument.
#[test]
fn record_positional_is_the_type_specific_fallback() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("e2cf-ours");
    let theirs = TempDir::new("e2cf-theirs");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let _ = run_in(&theirs.0, &ml, &["init"]);

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
        let theirs_run = run_in(&theirs.0, &ml, args);
        assert_eq!(ours_run.code, theirs_run.code, "exit {args:?}");
        assert_eq!(ours_run.stdout, theirs_run.stdout, "stdout {args:?}");
        assert_eq!(ours_run.stderr, theirs_run.stderr, "stderr {args:?}");
    }
    for domain in ["dc", "dp", "dr", "dg", "df", "dd"] {
        assert_eq!(
            normalize(&read_store_file(
                &ours.0,
                &format!("expertise/{domain}.jsonl")
            )),
            normalize(&read_store_file(
                &theirs.0,
                &format!("expertise/{domain}.jsonl")
            )),
            "store {domain} differs"
        );
    }
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
    let theirs_run = run_in(&theirs.0, &ml, &args);
    assert_eq!(ours_run.code, theirs_run.code, "exit retry");
    assert_eq!(
        echo_ours_to_reference(&ours_run.stderr),
        theirs_run.stderr,
        "stderr retry (echo names this binary)"
    );
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
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("b1209-ours");
    let theirs = TempDir::new("b1209-theirs");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let _ = run_in(&theirs.0, &ml, &["init"]);

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
        let theirs_run = run_in(&theirs.0, &ml, args);
        assert_eq!(ours_run.code, theirs_run.code, "exit {args:?}");
        assert_eq!(ours_run.stdout, theirs_run.stdout, "stdout {args:?}");
        // the empty-name case's Retry echo names this binary (README
        // DEVIATIONS) — normalized here, pinned by
        // record_echo_promotes_mulch
        assert_eq!(
            echo_ours_to_reference(&ours_run.stderr),
            theirs_run.stderr,
            "stderr {args:?}"
        );
    }
    for domain in ["f", "g", "h", "i", "j"] {
        assert_eq!(
            normalize(&read_store_file(
                &ours.0,
                &format!("expertise/{domain}.jsonl")
            )),
            normalize(&read_store_file(
                &theirs.0,
                &format!("expertise/{domain}.jsonl")
            )),
            "store {domain} differs"
        );
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
    let _ = run_in(&theirs.0, &ml, &seed);
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
    let theirs_run = run_in(&theirs.0, &ml, &args);
    assert_eq!(ours_run.code, theirs_run.code, "exit outcome");
    assert_eq!(ours_run.stdout, theirs_run.stdout, "stdout outcome");
    assert_eq!(ours_run.stderr, theirs_run.stderr, "stderr outcome");
    assert_eq!(
        ours_run.stdout, "✓ Outcome recorded: success on mx-fd666c\n",
        "no empty-flag parens"
    );
    assert_eq!(
        normalize(&read_store_file(&ours.0, "expertise/d.jsonl")),
        normalize(&read_store_file(&theirs.0, "expertise/d.jsonl")),
        "outcome store differs"
    );
}

/// mulch-2153: the Retry echo carries every provided flag in the
/// reference's buildRetryCommand order — the bare positional first,
/// `--type`/`--classification` raw (classification only when it
/// differs from tactical), payload pairs, then files/dir-anchor/tags
/// and the evidence pairs (`--evidence-commit` unquoted), the
/// missing pairs last. Plain and json (where the envelope says
/// `Example:`) are byte-pinned.
#[test]
fn record_missing_flags_retry_echoes_non_payload_flags() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("b2153-ours");
    let theirs = TempDir::new("b2153-theirs");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let _ = run_in(&theirs.0, &ml, &["init"]);

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
        let theirs_run = run_in(&theirs.0, &ml, &args);
        assert_eq!(ours_run.code, 1, "exit is 1 {mode:?}");
        assert_eq!(ours_run.code, theirs_run.code, "exit differs {mode:?}");
        assert_eq!(ours_run.stdout, theirs_run.stdout, "stdout {mode:?}");
        assert_eq!(
            echo_ours_to_reference(&ours_run.stderr),
            theirs_run.stderr,
            "stderr {mode:?} (echo names this binary)"
        );
    }
    // the deviation itself: ours promotes `mulch`, never the retired
    // binary (README DEVIATIONS; the missing pair comes last)
    let plain = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record", "d", "--type", "pattern", "--name", "n",
    ]);
    assert!(plain.stderr.contains(
        "Retry: mulch record d --type pattern --name \"n\" --description \"<description>\""
    ));
    assert!(!plain.stderr.contains("ml record"));
    assert_eq!(read_store_text_or_empty(&ours.0, "expertise/d.jsonl"), "");
}

/// mulch-8ca8: the batch-file-not-found gate precedes every store
/// side effect — no auto-create line, no domain registration — and
/// the plain error and stderr json envelope match byte-for-byte.
#[test]
fn record_batch_not_found_precedes_side_effects() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("b8ca8-ours");
    let theirs = TempDir::new("b8ca8-theirs");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let _ = run_in(&theirs.0, &ml, &["init"]);

    for mode in [&[] as &[&str], &["--json"]] {
        for batch in ["missing.json", "-"] {
            let mut args = mode.to_vec();
            args.extend_from_slice(&["record", "d", "--batch", batch]);
            let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
            let theirs_run = run_in(&theirs.0, &ml, &args);
            assert_eq!(ours_run.code, 1, "exit is 1 {batch} {mode:?}");
            assert_eq!(ours_run.code, theirs_run.code, "exit differs {batch}");
            assert_eq!(
                ours_run.stdout, theirs_run.stdout,
                "stdout {batch} {mode:?}"
            );
            assert_eq!(
                ours_run.stderr, theirs_run.stderr,
                "stderr {batch} {mode:?}"
            );
        }
    }
    // no side effect at all: the domain never registered
    assert!(!has_domain(&ours.0, "d"));
    assert!(!has_domain(&theirs.0, "d"));
}

/// mulch-5fd8: the batch/stdin family creates the domain SILENTLY
/// (no notice line in plain or json) while the single-record path
/// still prints it; `--batch ""` is no batch at all (JS truthiness —
/// the flag path runs with its own --type check); batch wins over
/// stdin when both are given.
#[test]
fn record_batch_mode_creates_domains_silently() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("b5fd8-ours");
    let theirs = TempDir::new("b5fd8-theirs");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let _ = run_in(&theirs.0, &ml, &["init"]);
    let fixture = |dir: &Path| {
        std::fs::write(
            dir.join("b.json"),
            r#"[{"type":"convention","content":"from-file"}]"#,
        )
        .expect("batch fixture");
    };
    fixture(&ours.0);
    fixture(&theirs.0);

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
        let theirs_run = run_in(&theirs.0, &ml, args);
        assert_eq!(ours_run.code, theirs_run.code, "exit {args:?}");
        assert_eq!(ours_run.stdout, theirs_run.stdout, "stdout {args:?}");
        assert_eq!(ours_run.stderr, theirs_run.stderr, "stderr {args:?}");
        assert!(
            !ours_run.stdout.contains("Auto-created"),
            "batch family prints no notice line: {args:?}"
        );
    }
    for domain in ["dnew", "dnew2", "dboth"] {
        assert!(has_domain(&ours.0, domain), "domain {domain} registered");
        assert!(
            has_domain(&theirs.0, domain),
            "reference registered {domain}"
        );
    }
    // batch-wins content and silent-creation bytes (timestamps are
    // stamped at write time, hence normalized)
    assert_eq!(
        normalize(&read_store_file(&ours.0, "expertise/dnew.jsonl")),
        normalize(&read_store_file(&theirs.0, "expertise/dnew.jsonl")),
        "dnew store differs"
    );
    assert!(read_store_file(&ours.0, "expertise/dboth.jsonl").contains("from-file"));

    // stdin: silent too (fed through a piped stdin)
    {
        let payload = r#"{"type":"convention","content":"sc"}"#;
        let ours_run = run_stdin(
            &ours.0,
            Path::new(mulch_bin()),
            &["record", "dnew3", "--stdin"],
            payload,
        );
        let theirs_run = run_stdin(&theirs.0, &ml, &["record", "dnew3", "--stdin"], payload);
        assert_eq!(ours_run.code, theirs_run.code, "exit stdin");
        assert_eq!(ours_run.stdout, theirs_run.stdout, "stdout stdin");
        assert_eq!(ours_run.stderr, theirs_run.stderr, "stderr stdin");
        assert!(
            !ours_run.stdout.contains("Auto-created"),
            "stdin prints no notice line"
        );
        // stdin registers the domain silently too
        assert!(has_domain(&ours.0, "dnew3"), "domain dnew3 registered");
        assert!(has_domain(&theirs.0, "dnew3"), "reference registered dnew3");
        assert_eq!(
            normalize(&read_store_file(&ours.0, "expertise/dnew3.jsonl")),
            normalize(&read_store_file(&theirs.0, "expertise/dnew3.jsonl")),
            "dnew3 store differs"
        );
    }

    // `--batch ""` is no batch: the flag path runs and requires --type
    let args = ["record", "dempty", "--batch", "", "--name", "n"];
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let theirs_run = run_in(&theirs.0, &ml, &args);
    assert_eq!(ours_run.code, theirs_run.code, "exit batch-empty");
    assert_eq!(ours_run.stdout, theirs_run.stdout, "stdout batch-empty");
    assert_eq!(ours_run.stderr, theirs_run.stderr, "stderr batch-empty");
    assert!(has_domain(&ours.0, "dempty"));
}

/// mulch-72d1: an unreadable `--batch` path (a directory, a
/// chmod-000 file) fails with the reference's Node io-error bytes —
/// `EISDIR: illegal operation on a directory, read` (no path) and
/// `EACCES: permission denied, open '<path>'` — on stderr in BOTH
/// channels, exit 1, and the read precedes every store side effect:
/// the domain never registers.
#[test]
fn record_batch_unreadable_path_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("b72d1-ours");
    let theirs = TempDir::new("b72d1-theirs");
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &["init"]);
        std::fs::create_dir(dir.join("batchdir")).expect("batch dir fixture");
    }
    let unreadable = |dir: &Path| {
        std::fs::write(dir.join("noperm.json"), "nope{").expect("noperm fixture");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(
                dir.join("noperm.json"),
                std::fs::Permissions::from_mode(0o000),
            )
            .expect("chmod 000");
        }
    };
    unreadable(&ours.0);
    unreadable(&theirs.0);

    for batch in ["batchdir", "noperm.json"] {
        for mode in [&[] as &[&str], &["--json"]] {
            let mut args = mode.to_vec();
            args.extend_from_slice(&["record", "d", "--batch", batch]);
            let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
            let their = run_in(&theirs.0, &ml, &args);
            assert_eq!(our.code, 1, "exit is 1 {batch} {mode:?}");
            assert_eq!(our.code, their.code, "exit differs {batch} {mode:?}");
            assert_eq!(our.stdout, their.stdout, "stdout {batch} {mode:?}");
            assert_eq!(our.stderr, their.stderr, "stderr {batch} {mode:?}");
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
    // expertise file appeared, the configs stay identical
    assert!(!has_domain(&ours.0, "d"));
    assert!(!has_domain(&theirs.0, "d"));
    assert_eq!(read_store_text_or_empty(&ours.0, "expertise/d.jsonl"), "");
    assert_eq!(read_store_text_or_empty(&theirs.0, "expertise/d.jsonl"), "");
    assert_eq!(
        read_store_file(&ours.0, "mulch.config.yaml"),
        read_store_file(&theirs.0, "mulch.config.yaml")
    );

    // restore permissions so the teardown can clean up everywhere
    for dir in [&ours.0, &theirs.0] {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let _ = std::fs::set_permissions(
                dir.join("noperm.json"),
                std::fs::Permissions::from_mode(0o644),
            );
        }
    }
}

/// mulch-943b: invalid JSON input (stdin and batch) fails with the
/// reference's JavaScriptCore parse-error bytes —
/// `Failed to parse JSON from stdin: JSON Parse error: …` — on
/// stderr in BOTH channels, exit 1, after the silent domain
/// auto-create; nothing writes to the store.
#[test]
fn record_invalid_json_parse_surface_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
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
        let theirs = TempDir::new(format!("b943b-{index}-t").as_str());
        for dir in [&ours.0, &theirs.0] {
            let _ = run_in(dir, &ml, &["init"]);
            std::fs::write(dir.join("b.json"), payload).expect("batch fixture");
        }
        for mode in [&[] as &[&str], &["--json"]] {
            // stdin surface
            let mut args = mode.to_vec();
            args.extend_from_slice(&["record", "d", "--stdin"]);
            let our = run_stdin(&ours.0, Path::new(mulch_bin()), &args, payload);
            let their = run_stdin(&theirs.0, &ml, &args, payload);
            assert_eq!(our.code, 1, "stdin exit {payload} {mode:?}");
            assert_eq!(
                our.code, their.code,
                "stdin exit differs {payload} {mode:?}"
            );
            assert_eq!(our.stdout, their.stdout, "stdin stdout {payload} {mode:?}");
            assert_eq!(our.stderr, their.stderr, "stdin stderr {payload} {mode:?}");
            // batch surface (the message still says "from stdin" —
            // the reference shares processStdinRecords)
            let mut args = mode.to_vec();
            args.extend_from_slice(&["record", "d", "--batch", "b.json"]);
            let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
            let their = run_in(&theirs.0, &ml, &args);
            assert_eq!(our.code, 1, "batch exit {payload} {mode:?}");
            assert_eq!(
                our.code, their.code,
                "batch exit differs {payload} {mode:?}"
            );
            assert_eq!(our.stdout, their.stdout, "batch stdout {payload} {mode:?}");
            assert_eq!(our.stderr, their.stderr, "batch stderr {payload} {mode:?}");
        }
        // the parse failure follows the reference's addDomain: the
        // domain registered silently with an empty store, nothing
        // else moved in either store
        assert!(has_domain(&ours.0, "d"), "domain registered {payload}");
        assert!(has_domain(&theirs.0, "d"), "reference registered {payload}");
        assert_eq!(read_store_file(&ours.0, "expertise/d.jsonl"), "");
        assert_eq!(read_store_file(&theirs.0, "expertise/d.jsonl"), "");
    }
    // byte pin of the canonical surface: plain stderr line and the
    // stderr json envelope for the bareword payload
    let pin = TempDir::new("b943b-pin");
    let _ = run_in(&pin.0, &ml, &["init"]);
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
        "{\n  \"success\": false,\n  \"command\": \"record\",\n  \"error\": \"Failed to parse JSON from stdin: JSON Parse error: Unexpected identifier \\\"not\\\"\"\n}\n"
    );
    assert_eq!(json.stdout, "");
}

// ---- sprint 42 (mulch-7f49): record lifecycle hooks ----
//
// Twin stores share one hooks/hook_settings block appended after
// `ml init` + `ml add dev`; every probe live-compares both channels,
// the exit code, and the store bytes. Two reference-side channels are
// kept out of the comparison on purpose: (a) the pino diagnostics
// line every failing hook logs on stderr (their log.ts header: "NOT
// the product-output channel") — silenced with MULCH_LOG_LEVEL=silent
// — and (b) the Bun EPIPE stack trace a hook leaks when it exits
// without draining stdin — every fixture hook reads stdin first.

/// Appends a hooks/hook_settings block to the store config (both
/// sides of a twin fixture share it verbatim).
fn append_hooks(dir: &Path, block: &str) {
    let config = dir.join(".mulch").join("mulch.config.yaml");
    let mut text = std::fs::read_to_string(&config).expect("init config");
    text.push_str(block);
    std::fs::write(&config, text).expect("hooks config");
}

/// The twin hook fixture: `ml init`, one `dev` domain, then the
/// shared hook block.
fn hook_fixture(dir: &Path, ml: &Path, block: &str) {
    let _ = run_in(dir, ml, &["init"]);
    let _ = run_in(dir, ml, &["add", "dev"]);
    append_hooks(dir, block);
}

/// Runs the reference with its pino diagnostics channel silenced:
/// failing hooks log a structured warn line on stderr that is not
/// product output, so byte parity targets the product channels only.
fn run_ref_quiet(dir: &Path, ml: &Path, args: &[&str]) -> Run {
    let output = std::process::Command::new(ml)
        .args(args)
        .env("MULCH_LOG_LEVEL", "silent")
        .current_dir(dir)
        .output()
        .expect("spawn reference");
    Run {
        code:   output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// Byte-compare one twin run pair (exit, stdout, stderr; timestamps
/// normalized — the twin fixtures record `now` microseconds apart).
fn assert_hook_pair(our: &Run, their: &Run, label: &str) {
    assert_eq!(
        our.code, their.code,
        "exit {label}\nour stderr: {:?}\ntheir stderr: {:?}",
        our.stderr, their.stderr
    );
    assert_eq!(
        normalize(&our.stdout),
        normalize(&their.stdout),
        "stdout {label}"
    );
    assert_eq!(
        normalize(&our.stderr),
        normalize(&their.stderr),
        "stderr {label}"
    );
}

/// Compares a hook-captured payload file between the twin stores
/// (missing files read empty — the dry-run/skip probes rely on that).
fn assert_capture_same(ours: &TempDir, theirs: &TempDir, file: &str) {
    let read = |dir: &TempDir| std::fs::read_to_string(dir.0.join(file)).unwrap_or_default();
    assert_eq!(
        normalize(&read(ours)),
        normalize(&read(theirs)),
        "{file} differs"
    );
}

/// Store-byte compare for runs that record `now`: timestamps mask,
/// everything else is byte law (the twin fixtures run microseconds
/// apart).
fn assert_hook_store_same(ours: &TempDir, theirs: &TempDir) {
    assert_eq!(
        normalize(&read_store_file(&ours.0, "expertise/dev.jsonl")),
        normalize(&read_store_file(&theirs.0, "expertise/dev.jsonl")),
        "dev.jsonl differs"
    );
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

/// A blocking pre-record hook (exit 3) aborts the flag path on both
/// channels with the reference's reason text, forwarding the hook's
/// stderr first; nothing is written.
#[test]
fn record_pre_hook_blocking_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let block =
        "hooks:\n  pre-record:\n    - 'cat >/dev/null; printf from-hook-stderr >&2; exit 3'\n";
    let ours = TempDir::new("hookblock-o");
    let theirs = TempDir::new("hookblock-t");
    hook_fixture(&ours.0, &ml, block);
    hook_fixture(&theirs.0, &ml, block);
    for mode in [&[] as &[&str], &["--json"]] {
        let args = record_args("p1", mode);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let our = run_in(&ours.0, Path::new(mulch_bin()), &refs);
        let their = run_ref_quiet(&theirs.0, &ml, &refs);
        assert_hook_pair(&our, &their, &format!("blocked {mode:?}"));
    }
    assert_same_file(&ours, &theirs, "expertise/dev.jsonl");
    assert_eq!(read_store_file(&ours.0, "expertise/dev.jsonl"), "");
}

/// A pre-record hook that overruns `hook_settings.timeout_ms` blocks
/// with the timeout reason on both channels (exit 124 semantics).
#[test]
fn record_pre_hook_timeout_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let block =
        "hooks:\n  pre-record:\n    - 'sleep 30 & wait'\nhook_settings:\n  timeout_ms: 150\n";
    let ours = TempDir::new("hooktime-o");
    let theirs = TempDir::new("hooktime-t");
    hook_fixture(&ours.0, &ml, block);
    hook_fixture(&theirs.0, &ml, block);
    for mode in [&[] as &[&str], &["--json"]] {
        let args = record_args("p1", mode);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let our = run_in(&ours.0, Path::new(mulch_bin()), &refs);
        let their = run_ref_quiet(&theirs.0, &ml, &refs);
        assert_hook_pair(&our, &their, &format!("timeout {mode:?}"));
    }
    assert_same_file(&ours, &theirs, "expertise/dev.jsonl");
    assert_eq!(read_store_file(&ours.0, "expertise/dev.jsonl"), "");
}

/// A pre-record hook's valid echo is adopted: the mutated record
/// replaces the input for dedup and write (write-time id appended
/// last), and the untouched record passes through unchanged.
#[test]
fn record_pre_hook_mutation_adoption_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    // Mutates only p2's payload (grep over the drained stdin); p1's
    // records pass through with the mutation gate closed.
    let block = "hooks:\n  pre-record:\n    - 'x=$(cat); if echo \"$x\" | grep -q p2; then printf %s \"{\\\"payload\\\":{\\\"domain\\\":\\\"dev\\\",\\\"record\\\":{\\\"type\\\":\\\"pattern\\\",\\\"classification\\\":\\\"tactical\\\",\\\"recorded_at\\\":\\\"2026-01-01T00:00:00.000Z\\\",\\\"name\\\":\\\"p2-MUT\\\",\\\"description\\\":\\\"mutated\\\"}}}\"; fi'\n";
    let ours = TempDir::new("hookmut-o");
    let theirs = TempDir::new("hookmut-t");
    hook_fixture(&ours.0, &ml, block);
    hook_fixture(&theirs.0, &ml, block);
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
        let their = run_ref_quiet(&theirs.0, &ml, &refs);
        assert_hook_pair(&our, &their, &format!("mutation {name} {mode:?}"));
    }
    assert_hook_store_same(&ours, &theirs);
    // The adopted record landed with the hook's key order and the
    // write-time id last.
    let store = read_store_file(&ours.0, "expertise/dev.jsonl");
    assert_eq!(store.lines().count(), 2);
    assert!(store.contains("p2-MUT"));
}

/// A pre-record hook echoing an INVALID record fails with the
/// reference's joined ajv sub-errors on both channels; the store
/// stays untouched.
#[test]
fn record_pre_hook_invalid_mutation_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let block = "hooks:\n  pre-record:\n    - 'x=$(cat); if echo \"$x\" | grep -q p2; then printf %s \"{\\\"payload\\\":{\\\"domain\\\":\\\"dev\\\",\\\"record\\\":{\\\"type\\\":\\\"pattern\\\",\\\"classification\\\":\\\"tactical\\\",\\\"recorded_at\\\":\\\"2026-01-01T00:00:00.000Z\\\"}}}\"; fi'\n";
    let ours = TempDir::new("hookinvmut-o");
    let theirs = TempDir::new("hookinvmut-t");
    hook_fixture(&ours.0, &ml, block);
    hook_fixture(&theirs.0, &ml, block);
    // p2's invalid echo aborts with the sub-error blob (probed:
    // ` must have required property 'content'; … ; must match
    // exactly one schema in oneOf`); p1 then records fine.
    for (name, mode) in [
        ("p2", &[] as &[&str]),
        ("p2", &["--json"]),
        ("p1", &[] as &[&str]),
    ] {
        let args = record_args(name, mode);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let our = run_in(&ours.0, Path::new(mulch_bin()), &refs);
        let their = run_ref_quiet(&theirs.0, &ml, &refs);
        assert_hook_pair(&our, &their, &format!("invalid mutation {name} {mode:?}"));
    }
    assert_hook_store_same(&ours, &theirs);
    assert!(read_store_file(&ours.0, "expertise/dev.jsonl").contains("p1"));
}

/// A failing post-record hook only warns: plain mode prints the
/// hook's forwarded stderr and a `Warning:` line after the success
/// line; json mode carries the `warnings` array last in the envelope.
#[test]
fn record_post_hook_warning_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let block = "hooks:\n  post-record:\n    - 'cat >/dev/null; printf post-stderr >&2; exit 1'\n";
    let ours = TempDir::new("hookpost-o");
    let theirs = TempDir::new("hookpost-t");
    hook_fixture(&ours.0, &ml, block);
    hook_fixture(&theirs.0, &ml, block);
    for mode in [&[] as &[&str], &["--json"]] {
        let args = record_args("p1", mode);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let our = run_in(&ours.0, Path::new(mulch_bin()), &refs);
        let their = run_ref_quiet(&theirs.0, &ml, &refs);
        assert_hook_pair(&our, &their, &format!("post warning {mode:?}"));
    }
    assert_hook_store_same(&ours, &theirs);
}

/// Pre-record fires BEFORE the duplicate decision (even a skip runs
/// it); post-record fires for actual writes only — a skipped
/// duplicate is a no-op with no post hook.
#[test]
fn record_hook_skip_fires_pre_but_no_post() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let block = "hooks:\n  pre-record:\n    - 'cat > prefired.json'\n  post-record:\n    - 'cat > postfired.json'\n";
    let ours = TempDir::new("hookskip-o");
    let theirs = TempDir::new("hookskip-t");
    hook_fixture(&ours.0, &ml, block);
    hook_fixture(&theirs.0, &ml, block);
    // Seed the anonymous duplicate, clear the captures, re-run: the
    // skip advisory prints while only the pre hook fired.
    for dir in [&ours.0, &theirs.0] {
        let _ = run_in(dir, &ml, &[
            "record",
            "dev",
            "--type",
            "convention",
            "--content",
            "dup",
        ]);
        std::fs::remove_file(dir.join("prefired.json")).ok();
        std::fs::remove_file(dir.join("postfired.json")).ok();
    }
    let args = ["record", "dev", "--type", "convention", "--content", "dup"];
    let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
    let their = run_ref_quiet(&theirs.0, &ml, &args);
    assert_hook_pair(&our, &their, "skip hooks");
    for dir in [&ours.0, &theirs.0] {
        assert!(dir.join("prefired.json").is_file(), "pre fired in {dir:?}");
        assert!(
            !dir.join("postfired.json").is_file(),
            "no post on skip in {dir:?}"
        );
    }
}

/// Dry-run fires NO hooks at all (no external side effects while
/// previewing) and mirrors the write decision as before.
#[test]
fn record_hook_dry_run_fires_nothing() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let block = "hooks:\n  pre-record:\n    - 'cat > prefired.json'\n  post-record:\n    - 'cat > postfired.json'\n";
    let ours = TempDir::new("hookdry-o");
    let theirs = TempDir::new("hookdry-t");
    hook_fixture(&ours.0, &ml, block);
    hook_fixture(&theirs.0, &ml, block);
    for mode in [&["--dry-run"] as &[&str], &["--dry-run", "--json"]] {
        let args = record_args("p1", mode);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let our = run_in(&ours.0, Path::new(mulch_bin()), &refs);
        let their = run_ref_quiet(&theirs.0, &ml, &refs);
        assert_hook_pair(&our, &their, &format!("dry-run {mode:?}"));
    }
    for dir in [&ours.0, &theirs.0] {
        assert!(
            !dir.join("prefired.json").is_file(),
            "no pre in dry-run {dir:?}"
        );
        assert!(
            !dir.join("postfired.json").is_file(),
            "no post in dry-run {dir:?}"
        );
    }
}

/// The hook payloads byte-pin against the reference: pre-record
/// carries `{domain, record}` WITHOUT the write-time id; post-record
/// carries the WRITTEN record (id last) plus `action` — for creates
/// and updates alike.
#[test]
fn record_hook_payloads_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let block =
        "hooks:\n  pre-record:\n    - 'cat > pre.json'\n  post-record:\n    - 'cat > post.json'\n";
    let ours = TempDir::new("hookpay-o");
    let theirs = TempDir::new("hookpay-t");
    hook_fixture(&ours.0, &ml, block);
    hook_fixture(&theirs.0, &ml, block);
    // Create, then the named-duplicate update of the same record.
    let create = record_args("p1", &[]);
    let create_refs: Vec<&str> = create.iter().map(String::as_str).collect();
    let our = run_in(&ours.0, Path::new(mulch_bin()), &create_refs);
    let their = run_ref_quiet(&theirs.0, &ml, &create_refs);
    assert_hook_pair(&our, &their, "payload create");
    assert_capture_same(&ours, &theirs, "pre.json");
    assert_capture_same(&ours, &theirs, "post.json");

    let mut update = record_args("p1", &[]);
    // Replace the trailing description VALUE (d1 -> d1-NEW).
    update.pop();
    update.push("d1-NEW".to_string());
    let update_refs: Vec<&str> = update.iter().map(String::as_str).collect();
    let our = run_in(&ours.0, Path::new(mulch_bin()), &update_refs);
    let their = run_ref_quiet(&theirs.0, &ml, &update_refs);
    assert_hook_pair(&our, &their, "payload update");
    assert_capture_same(&ours, &theirs, "pre.json");
    assert_capture_same(&ours, &theirs, "post.json");
    assert_hook_store_same(&ours, &theirs);
}

/// The batch family: a blocked pre-record hook drops just that record
/// (valid-records index) and the batch continues; the post-record
/// hook fires per actual write with `{domain, record, action}` and
/// its failure warns into the summary envelope's `warnings` array.
#[test]
fn record_batch_hook_drop_one_continue_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let block = "hooks:\n  pre-record:\n    - 'x=$(cat); if echo \"$x\" | grep -q b1; then exit 3; fi'\n  post-record:\n    - 'cat >> postbatch.json; exit 7'\n";
    let ours = TempDir::new("hookbatch-o");
    let theirs = TempDir::new("hookbatch-t");
    hook_fixture(&ours.0, &ml, block);
    hook_fixture(&theirs.0, &ml, block);
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(
            dir.join("b.json"),
            "[{\"type\":\"pattern\",\"name\":\"b1\",\"description\":\"d\"},{\"type\":\"pattern\",\"name\":\"b2\",\"description\":\"d\"}]",
        )
        .expect("batch fixture");
    }
    for mode in [&[] as &[&str], &["--json"]] {
        let mut args = vec!["record", "dev", "--batch", "b.json"];
        args.extend_from_slice(mode);
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        let their = run_ref_quiet(&theirs.0, &ml, &args);
        assert_hook_pair(&our, &their, &format!("batch drop-one {mode:?}"));
    }
    assert_hook_store_same(&ours, &theirs);
    // b2 wrote (twice across the two runs — the second as an
    // update), b1 never did; the captured post payloads agree.
    let store = read_store_file(&ours.0, "expertise/dev.jsonl");
    assert!(store.contains("b2"));
    assert!(!store.contains("b1"));
    assert_capture_same(&ours, &theirs, "postbatch.json");
}

/// A batch pre-record hook echoing an invalid record drops that
/// record with the invalid-mutation error; the rest still writes.
#[test]
fn record_batch_hook_invalid_mutation_drops_record() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let block = "hooks:\n  pre-record:\n    - 'x=$(cat); if echo \"$x\" | grep -q b2; then printf %s \"{\\\"payload\\\":{\\\"domain\\\":\\\"dev\\\",\\\"record\\\":{\\\"type\\\":\\\"pattern\\\",\\\"classification\\\":\\\"tactical\\\",\\\"recorded_at\\\":\\\"2026-01-01T00:00:00.000Z\\\"}}}\"; fi'\n";
    let ours = TempDir::new("hookbinv-o");
    let theirs = TempDir::new("hookbinv-t");
    hook_fixture(&ours.0, &ml, block);
    hook_fixture(&theirs.0, &ml, block);
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(
            dir.join("b.json"),
            "[{\"type\":\"pattern\",\"name\":\"b1\",\"description\":\"d\"},{\"type\":\"pattern\",\"name\":\"b2\",\"description\":\"d\"}]",
        )
        .expect("batch fixture");
    }
    for mode in [&[] as &[&str], &["--json"]] {
        let mut args = vec!["record", "dev", "--batch", "b.json"];
        args.extend_from_slice(mode);
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        let their = run_ref_quiet(&theirs.0, &ml, &args);
        assert_hook_pair(&our, &their, &format!("batch invalid mutation {mode:?}"));
    }
    assert_hook_store_same(&ours, &theirs);
    let store = read_store_file(&ours.0, "expertise/dev.jsonl");
    assert!(store.contains("b1"));
    assert!(!store.contains("b2"));
}
