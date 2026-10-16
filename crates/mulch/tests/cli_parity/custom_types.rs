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

// ---- sprint 31 (mulch-d524): the custom-type-aware flag path ----

/// The flag-path config: adds an extracts-files custom and an
/// underscored-field custom to the probe pair.
const FLAG_CONFIG: &str = "version: '1'\n\
domains:\n  dev: {}\n\
custom_types:\n  snippet:\n    required: [name, code]\n    optional: [note]\n    dedup_key: name\n    id_key: name\n    summary: \"{name}: {code}\"\n  sketchnote:\n    required: [title, code_name]\n    dedup_key: title\n    id_key: title\n    summary: \"{title}!\"\n  gitfile:\n    required: [name]\n    optional: [files]\n    dedup_key: name\n    id_key: name\n    extracts_files: true\n    summary: \"{name}\"\n\
governance:\n  max_entries: 100\n  warn_entries: 150\n  hard_limit: 200\n\
classification_defaults:\n  shelf_life:\n    tactical: 14\n    observational: 30\n";

/// Flag-path twins (no git: evidence and context files stay absent).
fn flag_twins(tag: &str) -> (TempDir, TempDir) {
    let ours = TempDir::new(&format!("{tag}-ours"));
    let theirs = TempDir::new(&format!("{tag}-theirs"));
    for dir in [&ours.0, &theirs.0] {
        write_seed_with_config(dir, FLAG_CONFIG, &[]);
    }
    (ours, theirs)
}

/// Git twins with ONE identical dirty state: the same committed
/// files, the same modification, the same staged addition — so
/// `getContextFiles` lists identical names on both sides (evidence
/// commit SHAs differ; the assertions mask them).
fn git_twins(tag: &str) -> (TempDir, TempDir) {
    let ours = TempDir::new(&format!("{tag}-ours"));
    let theirs = TempDir::new(&format!("{tag}-theirs"));
    for dir in [&ours.0, &theirs.0] {
        write_seed_with_config(dir, FLAG_CONFIG, &[]);
        for args in [
            ["init", "-q"].as_slice(),
            &["config", "user.email", "t@example.org"],
            &["config", "user.name", "t"],
        ] {
            assert!(
                std::process::Command::new("git")
                    .args(args)
                    .current_dir(dir)
                    .output()
                    .expect("git spawns")
                    .status
                    .success()
            );
        }
        std::fs::write(dir.join("a.rs"), "one\n").expect("a.rs");
        std::fs::create_dir_all(dir.join("src")).expect("src");
        std::fs::write(dir.join("src").join("b.rs"), "one\n").expect("b.rs");
        assert!(
            std::process::Command::new("git")
                .args(["add", ".", "-A"])
                .current_dir(dir)
                .output()
                .expect("git add")
                .status
                .success()
        );
        assert!(
            std::process::Command::new("git")
                .args(["commit", "-qm", "init"])
                .current_dir(dir)
                .output()
                .expect("git commit")
                .status
                .success()
        );
        // unstaged modification + staged addition
        std::fs::write(dir.join("a.rs"), "two\n").expect("a.rs modified");
        std::fs::write(dir.join("src").join("c.rs"), "new\n").expect("c.rs");
        assert!(
            std::process::Command::new("git")
                .args(["add", "src/c.rs"])
                .current_dir(dir)
                .output()
                .expect("git stage")
                .status
                .success()
        );
    }
    (ours, theirs)
}

/// Masks evidence commit shas (the twin repos commit at different
/// instants — the shas differ, the file names do not).
fn mask_commits(text: &str) -> String {
    const KEY: &str = "\"commit\":\"";
    let mut out = String::new();
    let mut rest = text;
    while let Some(pos) = rest.find(KEY) {
        let value_start = pos + KEY.len();
        out.push_str(&rest[..value_start]);
        rest = &rest[value_start..];
        let end = rest.find('"').expect("commit end");
        out.push_str("<C>");
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// The full flag-path comparison surface: exit code, stdout, stderr,
/// and store bytes (timestamps and commit shas masked; the Retry
/// echo normalized to the reference's binary name).
fn assert_flag_parity(
    tag: &str,
    ours: &TempDir,
    theirs: &TempDir,
    ours_args: &[&str],
    theirs_args: &[&str],
    ml: &Path,
) {
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), ours_args);
    let theirs_run = run_in(&theirs.0, ml, theirs_args);
    assert_eq!(ours_run.code, theirs_run.code, "{tag}: exit codes differ");
    let clean =
        |text: &str| mask_commits(&mask_timestamps(&echo_ours_to_reference(&normalize(text))));
    assert_eq!(
        clean(&ours_run.stdout),
        clean(&theirs_run.stdout),
        "{tag}: stdout"
    );
    assert_eq!(
        clean(&ours_run.stderr),
        clean(&theirs_run.stderr),
        "{tag}: stderr"
    );
    let files: Vec<std::ffi::OsString> = std::fs::read_dir(ours.0.join(".mulch/expertise"))
        .expect("expertise dir")
        .map(|entry| entry.expect("entry").file_name())
        .collect();
    for name in files {
        let ours_file = std::fs::read_to_string(ours.0.join(".mulch/expertise").join(&name))
            .expect("ours store");
        let theirs_file = std::fs::read_to_string(theirs.0.join(".mulch/expertise").join(&name))
            .expect("theirs store");
        assert_eq!(
            mask_commits(&mask_timestamps(&ours_file)),
            mask_commits(&mask_timestamps(&theirs_file)),
            "{tag}: store {}",
            name.display()
        );
    }
}

#[test]
fn custom_flag_path_create_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    // plain create, json create, underscored field, optional note,
    // dry-run, positional drop (customs have NO positional fallback)
    for (tag, args) in [
        ("plain", vec![
            "record", "dev", "--type", "snippet", "--name", "s1", "--code", "print(1)",
        ]),
        ("json", vec![
            "record", "dev", "--type", "snippet", "--name", "s2", "--code", "c2", "--json",
        ]),
        ("underscore", vec![
            "record",
            "dev",
            "--type",
            "sketchnote",
            "--title",
            "T1",
            "--code-name",
            "CN1",
        ]),
        ("optional", vec![
            "record", "dev", "--type", "snippet", "--name", "n1", "--code", "c", "--note",
            "wichtig",
        ]),
        ("dry", vec![
            "record",
            "dev",
            "--type",
            "snippet",
            "--name",
            "d1",
            "--code",
            "dc",
            "--dry-run",
        ]),
        ("positional-drop", vec![
            "record",
            "dev",
            "--type",
            "snippet",
            "--name",
            "p1",
            "--code",
            "pc",
            "positional body",
        ]),
        ("empty-value-missing", vec![
            "record", "dev", "--type", "snippet", "--name", "e1", "--code", "",
        ]),
        ("upsert-second-run", vec![
            "record", "dev", "--type", "snippet", "--name", "s1", "--code", "c2",
        ]),
    ] {
        let (ours, theirs) = flag_twins(&format!("ct-flag-{tag}"));
        if tag == "upsert-second-run" {
            // the same named record already exists on both sides
            for (dir, bin) in [
                (&ours.0, mulch_bin()),
                (&theirs.0, ml.to_string_lossy().as_ref()),
            ] {
                let _ = run_in(dir, Path::new(bin), &[
                    "record", "dev", "--type", "snippet", "--name", "s1", "--code", "old",
                ]);
            }
        }
        assert_flag_parity(tag, &ours, &theirs, &args, &args, &ml);
    }
}

#[test]
fn custom_flag_missing_flags_retry_and_choices_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    // missing required flags: the hyphenated flag list + the
    // underscore-keeping placeholder ride the Retry echo
    for (tag, args) in [
        ("snippet", vec!["record", "dev", "--type", "snippet"]),
        ("sketchnote", vec!["record", "dev", "--type", "sketchnote"]),
        ("no-type", vec!["record", "newdom"]),
    ] {
        let (ours, theirs) = flag_twins(&format!("ct-miss-{tag}"));
        assert_flag_parity(tag, &ours, &theirs, &args, &args, &ml);
    }
    // OUR invalid-choice rejection (clap wording is the documented
    // deviation): the SET is the registry's names and the exit code
    // matches the reference
    let (ours, theirs) = flag_twins("ct-choice");
    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &[
        "record", "dev", "--type", "nosuch",
    ]);
    let theirs_run = run_in(&theirs.0, &ml, &["record", "dev", "--type", "nosuch"]);
    assert_eq!(ours_run.code, theirs_run.code);
    assert!(
        ours_run.stderr.contains(
            "[possible values: convention, pattern, failure, decision, reference, guide, snippet, sketchnote, gitfile]"
        ),
        "the choice set is the merged registry: {}",
        ours_run.stderr
    );
    assert!(theirs_run.stderr.contains("Allowed choices are convention, pattern, failure, decision, reference, guide, snippet, sketchnote, gitfile."));
}

#[test]
fn custom_git_context_files_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    // extracts-files types auto-populate from the git context
    // (staged + unstaged, sorted): the custom AND the builtin pattern
    for (tag, args) in [
        ("custom", vec![
            "record", "dev", "--type", "gitfile", "--name", "g1",
        ]),
        ("builtin", vec![
            "record",
            "dev",
            "--type",
            "pattern",
            "--name",
            "p1",
            "--description",
            "d1",
        ]),
    ] {
        let (ours, theirs) = git_twins(&format!("ct-ctx-{tag}"));
        assert_flag_parity(tag, &ours, &theirs, &args, &args, &ml);
    }
}

#[test]
fn custom_edit_flag_path_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    // edit accepts custom field flags: scalars replace in place,
    // undeclared flags drop, the extracts-files field splits commas,
    // empty values store (the raw flag view), unregistered types
    // write no payload fields (the write gate rejects either way)
    for (tag, setup, edit) in [
        (
            "custom-field",
            vec![
                "record", "dev", "--type", "snippet", "--name", "s1", "--code", "c",
            ],
            vec!["edit", "dev", "mx-298f5e", "--code", "new"],
        ),
        (
            "underscore-and-drop",
            vec![
                "record",
                "dev",
                "--type",
                "sketchnote",
                "--title",
                "T1",
                "--code-name",
                "CN",
            ],
            vec![
                "edit",
                "dev",
                "mx-1493eb",
                "--code-name",
                "CN2",
                "--note",
                "dropped",
            ],
        ),
        (
            "files-comma",
            vec!["record", "dev", "--type", "gitfile", "--name", "g1"],
            vec!["edit", "dev", "mx-3d38e9", "--files", "x.rs, y.rs"],
        ),
        (
            "empty-stores",
            vec![
                "record", "dev", "--type", "snippet", "--name", "s1", "--code", "c",
            ],
            vec!["edit", "dev", "mx-298f5e", "--code", ""],
        ),
        (
            "files-empty-array",
            vec![
                "record",
                "dev",
                "--type",
                "pattern",
                "--name",
                "p",
                "--description",
                "d",
            ],
            vec!["edit", "dev", "mx-31be4b", "--files", ""],
        ),
    ] {
        let (ours, theirs) = flag_twins(&format!("ct-edit-{tag}"));
        for (dir, bin) in [(&ours.0, Path::new(mulch_bin())), (&theirs.0, &ml)] {
            let seeded = run_in(dir, bin, &setup);
            assert_eq!(seeded.code, 0, "{tag}: seed failed");
        }
        assert_flag_parity(tag, &ours, &theirs, &edit, &edit, &ml);
    }
    // unregistered type + --allow-unknown-types: NO payload field is
    // written (the reference's def loop skips unknown defs) — the
    // write gate rejects the update with equal bytes, store intact
    let (ours, theirs) = flag_twins("ct-edit-unknown");
    let unknown = "{\"type\":\"wtf\",\"name\":\"u\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-16T00:00:00.000Z\",\"id\":\"mx-000001\"}\n";
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(dir.join(".mulch/expertise/dev.jsonl"), unknown).expect("store");
    }
    let edit = [
        "--allow-unknown-types",
        "edit",
        "dev",
        "mx-000001",
        "--content",
        "x",
    ];
    assert_flag_parity("unknown", &ours, &theirs, &edit, &edit, &ml);
}
