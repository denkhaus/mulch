//! `custom_types` parity probes (mulch-b4bd): the merged registry is
//! invisible to builtin-only stores (the rest of the battery pins
//! that); these probes exercise the custom-aware surfaces against
//! the reference — batch create/upsert/content-hash ids, validate's
//! oneOf with custom branches, doctor's registry counts and custom
//! duplicates, the global config-error channel, and `--fix` over
//! invalid custom records.

use std::path::Path;

use super::support::*;

/// The probe config: a field-dedup custom (separate id_key) and a
/// content_hash custom (never-duplicate, empty id key).
const CUSTOM_CONFIG: &str = "version: '1'\n\
domains:\n  dev: {}\n\
custom_types:\n  snippet:\n    required: [name, code]\n    optional: [note]\n    dedup_key: name\n    id_key: name\n    summary: \"{name}: {code}\"\n  rant:\n    required: [body]\n    dedup_key: content_hash\n    summary: \"{body}\"\n\
governance:\n  max_entries: 100\n  warn_entries: 150\n  hard_limit: 200\n\
classification_defaults:\n  shelf_life:\n    tactical: 14\n    observational: 30\n";

/// The invalid config: missing dedup_key (the reference's first
/// failing validation rule).
const BROKEN_CONFIG: &str = "version: '1'\n\
domains:\n  dev: {}\n\
custom_types:\n  broken:\n    required: [name]\n\
governance:\n  max_entries: 100\n  warn_entries: 150\n  hard_limit: 200\n\
classification_defaults:\n  shelf_life:\n    tactical: 14\n    observational: 30\n";

/// Seeds a twin pair with the custom config and the shared batch file.
fn custom_twins(tag: &str) -> (TempDir, TempDir) {
    let ours = TempDir::new(&format!("{tag}-ours"));
    let theirs = TempDir::new(&format!("{tag}-theirs"));
    for dir in [&ours.0, &theirs.0] {
        write_seed_with_config(dir, CUSTOM_CONFIG, &[]);
        std::fs::write(
            dir.join("b.json"),
            "[\n \
             {\"type\":\"snippet\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-15T18:40:00.000Z\",\"name\":\"b1\",\"code\":\"x\"},\n \
             {\"type\":\"snippet\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-15T18:40:01.000Z\",\"name\":\"b1\",\"code\":\"y\"},\n \
             {\"type\":\"rant\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-15T18:40:02.000Z\",\"body\":\"bb\"}\n\
             ]\n",
        )
        .expect("batch file");
    }
    (ours, theirs)
}

/// Neutralizes the relative-time windows (``just now``/``2m ago``)
/// and the upgrade-check lines doctor/status sprinkle.
fn strip_time_and_upgrade(text: &str) -> String {
    text.lines()
        .filter(|line| {
            !line.contains("Update available")
                && !line.contains("Run `mulch upgrade`")
                && !line.contains("Native binary")
        })
        .map(|line| {
            let mut line = line.to_string();
            for prefix in ["updated ", "recorded "] {
                if let Some(at) = line.find(prefix)
                    && let Some(ago) = line[at..].find(" ago")
                {
                    line.replace_range(at + prefix.len()..at + ago + 4, "<T>");
                }
            }
            if line.contains("passed") && line.contains(',') {
                // summary counts differ by the upgrade warn
                return "<SUMMARY>".into();
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn custom_batch_create_upsert_content_hash_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = custom_twins("ct-batch");

    // First run: create 2 (snippet + rant), update 1 (within-batch
    // snippet upsert); second run: the same records again — the
    // snippet updates in place, the content-hash rant CREATES anew.
    for pass in 0..2 {
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &[
            "record", "dev", "--batch", "b.json",
        ]);
        let theirs_run = run_in(&theirs.0, &ml, &["record", "dev", "--batch", "b.json"]);
        assert_eq!(ours_run.code, theirs_run.code, "pass {pass} codes differ");
        assert_eq!(
            ours_run.stdout, theirs_run.stdout,
            "pass {pass} stdout differs"
        );
        assert_eq!(
            ours_run.stderr, theirs_run.stderr,
            "pass {pass} stderr differs"
        );
    }

    assert_same_file(&ours, &theirs, "expertise/dev.jsonl");

    // The json arm of the summary.
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record", "dev", "--batch", "b.json", "--json",
    ]);
    let theirs_run = run_in(&theirs.0, &ml, &[
        "record", "dev", "--batch", "b.json", "--json",
    ]);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);
    assert_eq!(ours_run.stderr, theirs_run.stderr);
}

#[test]
fn custom_validate_oneof_branches_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = custom_twins("ct-validate");
    // A record failing EVERY branch: builtin subs first, then the
    // custom branches' own first-missing fields, then the oneOf tail.
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(
            dir.join(".mulch/expertise/dev.jsonl"),
            "{\"type\":\"snippet\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-15T18:40:00.000Z\",\"code\":\"x\"}\n",
        )
        .expect("domain file");
    }

    for args in [&["validate"][..], &["validate", "--json"][..]] {
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), args);
        let theirs_run = run_in(&theirs.0, &ml, args);
        assert_eq!(ours_run.code, theirs_run.code, "{args:?} codes differ");
        assert_eq!(
            ours_run.stdout, theirs_run.stdout,
            "{args:?} stdout differs"
        );
        assert_eq!(
            ours_run.stderr, theirs_run.stderr,
            "{args:?} stderr differs"
        );
    }
}

#[test]
fn custom_doctor_registry_and_duplicates_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = custom_twins("ct-doctor");
    // Run the batch so counts exist, then append a same-name snippet
    // pair so the duplicates check fires over a custom dedup key.
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record", "dev", "--batch", "b.json",
    ]);
    let _ = run_in(&theirs.0, &ml, &["record", "dev", "--batch", "b.json"]);
    for dir in [&ours.0, &theirs.0] {
        let path = dir.join(".mulch/expertise/dev.jsonl");
        let mut body = std::fs::read_to_string(&path).expect("domain file");
        body.push_str("{\"type\":\"snippet\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-15T19:00:00.000Z\",\"name\":\"dup\",\"code\":\"a\",\"id\":\"mx-111111\"}\n");
        body.push_str("{\"type\":\"snippet\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-15T19:00:00.000Z\",\"name\":\"dup\",\"code\":\"b\",\"id\":\"mx-222222\"}\n");
        std::fs::write(&path, body).expect("append dup pair");
    }

    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["doctor"]);
    let theirs_run = run_in(&theirs.0, &ml, &["doctor"]);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(
        strip_time_and_upgrade(&ours_run.stdout),
        strip_time_and_upgrade(&theirs_run.stdout),
        "doctor report differs"
    );
    assert_eq!(ours_run.stderr, theirs_run.stderr);

    // The json arm carries the type-registry details (built-in vs
    // custom rows and counts).
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--json"]);
    let theirs_run = run_in(&theirs.0, &ml, &["doctor", "--json"]);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(
        normalize_doctor_json(&ours_run.stdout),
        normalize_doctor_json(&theirs_run.stdout),
        "doctor json differs"
    );
}

#[test]
fn custom_status_counts_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = custom_twins("ct-status");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record", "dev", "--batch", "b.json",
    ]);
    let _ = run_in(&theirs.0, &ml, &["record", "dev", "--batch", "b.json"]);

    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["status"]);
    let theirs_run = run_in(&theirs.0, &ml, &["status"]);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(
        strip_time_and_upgrade(&ours_run.stdout),
        strip_time_and_upgrade(&theirs_run.stdout),
        "status differs"
    );
}

#[test]
fn custom_invalid_config_error_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("ct-broken-ours");
    let theirs = TempDir::new("ct-broken-theirs");
    for dir in [&ours.0, &theirs.0] {
        write_seed_with_config(dir, BROKEN_CONFIG, &[]);
    }

    // Every store command fails the same global way — including
    // init (the reference builds its registry at CLI start); the
    // json envelope hardcodes command "init".
    let commands: [&[&str]; 6] = [
        &["status"],
        &["validate"],
        &["doctor"],
        &[
            "record",
            "dev",
            "--type",
            "pattern",
            "--name",
            "x",
            "--description",
            "y",
        ],
        &["init"],
        &["status", "--json"],
    ];
    for args in commands {
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), args);
        let theirs_run = run_in(&theirs.0, &ml, args);
        assert_eq!(ours_run.code, theirs_run.code, "{args:?} codes differ");
        assert_eq!(
            ours_run.stdout, theirs_run.stdout,
            "{args:?} stdout differs"
        );
        assert_eq!(
            ours_run.stderr, theirs_run.stderr,
            "{args:?} stderr differs"
        );
    }
}

#[test]
fn custom_doctor_fix_drops_invalid_custom_records() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = custom_twins("ct-fix");
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(
            dir.join(".mulch/expertise/dev.jsonl"),
            "{\"type\":\"snippet\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-15T18:40:00.000Z\",\"name\":\"ok\",\"code\":\"c\",\"id\":\"mx-333333\"}\n\
             {\"type\":\"snippet\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-15T18:40:01.000Z\",\"code\":\"no-name\"}\n\
             {\"type\":\"rant\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-15T18:40:02.000Z\",\"body\":\"b\"}\n",
        )
        .expect("domain file");
    }

    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["doctor", "--fix"]);
    let theirs_run = run_in(&theirs.0, &ml, &["doctor", "--fix"]);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(
        strip_time_and_upgrade(&ours_run.stdout),
        strip_time_and_upgrade(&theirs_run.stdout),
        "doctor --fix report differs"
    );
    assert_eq!(ours_run.stderr, theirs_run.stderr);
    assert_same_file(&ours, &theirs, "expertise/dev.jsonl");
}

#[test]
fn custom_move_and_delete_summaries_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = custom_twins("ct-move");
    // Fixed ids (the resolver matches the id field verbatim); the
    // summaries render the COMPILED template ("{name}: {code}").
    let fixture = "{\"type\":\"snippet\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-15T10:00:00.000Z\",\"name\":\"s1\",\"code\":\"print(1)\",\"id\":\"mx-aaaa01\"}\n{\
        \"type\":\"snippet\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-15T10:00:00.000Z\",\"name\":\"s2\",\"code\":\"x\",\"id\":\"mx-aaaa02\"}\n";
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(dir.join(".mulch/expertise/dev.jsonl"), fixture).expect("dev fixture");
        std::fs::write(dir.join(".mulch/expertise/tgt.jsonl"), "").expect("tgt fixture");
    }

    for args in [
        &["move", "dev", "mx-aaaa01", "tgt"][..],
        &["delete", "dev", "mx-aaaa02", "--dry-run"][..],
        &["delete", "dev", "mx-aaaa02"][..],
        &["delete", "dev", "mx-aaaa02", "--json"][..],
    ] {
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), args);
        let theirs_run = run_in(&theirs.0, &ml, args);
        assert_eq!(ours_run.code, theirs_run.code, "{args:?} codes differ");
        assert_eq!(
            ours_run.stdout, theirs_run.stdout,
            "{args:?} stdout differs"
        );
        assert_eq!(
            ours_run.stderr, theirs_run.stderr,
            "{args:?} stderr differs"
        );
    }
    assert_same_file(&ours, &theirs, "expertise/dev.jsonl");
    assert_same_file(&ours, &theirs, "expertise/tgt.jsonl");
}

#[test]
fn custom_batch_validation_hint_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = custom_twins("ct-hint");
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(
            dir.join("bad.json"),
            "[{\"type\":\"snippet\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-15T18:40:00.000Z\",\"code\":\"x\"}]\n",
        )
        .expect("bad batch");
    }

    for args in [
        &["record", "dev", "--batch", "bad.json"][..],
        &["record", "dev", "--batch", "bad.json", "--json"][..],
    ] {
        let ours_run = run_in(&ours.0, Path::new(mulch_bin()), args);
        let theirs_run = run_in(&theirs.0, &ml, args);
        assert_eq!(ours_run.code, theirs_run.code, "{args:?} codes differ");
        assert_eq!(
            ours_run.stdout, theirs_run.stdout,
            "{args:?} stdout differs"
        );
        assert_eq!(
            ours_run.stderr, theirs_run.stderr,
            "{args:?} stderr differs"
        );
    }
}
