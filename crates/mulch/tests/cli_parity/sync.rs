//! `mulch sync` pins (mulch-176f): the repo gate, the validate step
//! (escape hatches OFF at the commit gate), the worktree skip, the
//! no-changes success, and add+commit through the git adapter
//! (ADR-0008: the battery runs OUR binary only; the byte pins below
//! were frozen from the parity era's twin runs).

use std::path::Path;

use super::support::*;

/// Fixture git: raw git in `cwd` with hooks and signing neutralized
/// repo-locally so the machine's global config cannot break fixtures.
fn git(cwd: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("fixture git spawns");
    assert!(
        output.status.success(),
        "fixture git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// One scratch git repository with a root commit and a `.mulch` store
/// seeded from `config`/`domains`, ready for a sync.
fn scratch_git_store(tag: &str, config: &str, domains: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new(tag);
    std::fs::create_dir_all(dir.0.join(".mulch")).expect("mulch dir");
    git(&dir.0, &["init", "-q"]);
    git(&dir.0, &["config", "user.email", "test@mulch.local"]);
    git(&dir.0, &["config", "user.name", "mulch battery"]);
    git(&dir.0, &["config", "core.hooksPath", "/dev/null"]);
    git(&dir.0, &["config", "commit.gpgsign", "false"]);
    write_seed_with_config(&dir.0, config, domains);
    git(&dir.0, &["add", ".mulch/"]);
    git(&dir.0, &["commit", "-q", "-m", "seed"]);
    dir
}

/// Not a git repository: exit 1, and the plain/json texts DIFFER (the
/// plain line is the short lowercase sentence).
#[test]
fn sync_outside_a_git_repository_fails_with_split_texts() {
    let dir = TempDir::new("sync-norepo");
    let plain = run_in(&dir.0, Path::new(mulch_bin()), &["sync"]);
    assert_eq!(plain.code, 1);
    assert_eq!(plain.stderr, "Error: not in a git repository.\n");
    let json = run_in(&dir.0, Path::new(mulch_bin()), &["--json", "sync"]);
    assert_eq!(json.code, 1);
    assert!(
        json.stderr
            .contains("Not in a git repository. Run this command from within a git repository."),
        "{}",
        json.stderr
    );
}

/// A git repository without a store: the thrown config error under
/// sync's `Validation error:` prefix — never a findings list.
#[test]
fn sync_without_a_store_is_a_validation_error() {
    let dir = TempDir::new("sync-nostore");
    git(&dir.0, &["init", "-q"]);
    let plain = run_in(&dir.0, Path::new(mulch_bin()), &["sync"]);
    assert_eq!(plain.code, 1);
    assert!(
        plain
            .stderr
            .contains("Validation error: No .mulch/ directory found."),
        "{}",
        plain.stderr
    );
    let json = run_in(&dir.0, Path::new(mulch_bin()), &["--json", "sync"]);
    assert_eq!(json.code, 1);
    assert!(
        json.stderr
            .contains("Validation error: No .mulch/ directory found."),
        "{}",
        json.stderr
    );
}

/// A clean tree is a clean success: nothing committed, the frozen
/// plain/json/quiet bytes (frozen from the parity era).
#[test]
fn sync_with_no_changes_is_a_clean_success() {
    let ours = scratch_git_store("sync-clean", "version: '1'\ndomains:\n  dev: {}\n", &[(
        "dev", "",
    )]);
    let plain = run_in(&ours.0, Path::new(mulch_bin()), &["sync"]);
    assert_eq!(plain.code, 0);
    assert_eq!(
        plain.stdout,
        "✓ Validated 0 records\nNo .mulch/ changes to commit.\n"
    );
    let json = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "sync"]);
    assert_eq!(json.code, 0);
    assert_eq!(
        json.stdout,
        "{\n  \"success\": true,\n  \"command\": \"sync\",\n  \"validated\": true,\n  \
         \"committed\": false,\n  \"message\": \"No changes to commit\"\n}\n"
    );
    let quiet = run_in(&ours.0, Path::new(mulch_bin()), &["--quiet", "sync"]);
    assert_eq!(quiet.code, 0);
    assert_eq!(quiet.stdout, "");
}

/// The happy path: a dirty `.mulch` validates, stages, and commits —
/// the frozen output bytes, the commit message (default and
/// `--message`), and a clean tree afterwards.
#[test]
fn sync_commits_mulch_changes() {
    let config = "version: '1'\ndomains:\n  dev: {}\n";
    let body = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-26T10:00:00.000Z\",\"name\":\"p1\",\"description\":\"d\",\"id\":\"mx-abcde01\"}\n";
    for (tag, args, expect_subject) in [
        ("default", &["sync"][..], "mulch: update expertise"),
        (
            "custom",
            &["sync", "--message", "chore: expertise"][..],
            "chore: expertise",
        ),
        (
            "json",
            &["--json", "sync", "--message", "chore: expertise"][..],
            "chore: expertise",
        ),
    ] {
        let ours = scratch_git_store(&format!("sync-commit-{tag}"), config, &[("dev", "")]);
        std::fs::write(ours.0.join(".mulch/expertise/dev.jsonl"), body).expect("dirty");
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 0, "{tag}: {}", our.stderr);
        if args[0] == "--json" {
            // json stays silent on stdout; the envelope rides stdout's
            // channel contract (success prints the envelope)
            assert!(
                our.stdout.contains("\"committed\": true"),
                "{tag}: {}",
                our.stdout
            );
            assert!(
                our.stdout.contains("\"message\": \"chore: expertise\""),
                "{tag}: {}",
                our.stdout
            );
        } else {
            assert_eq!(
                our.stdout,
                format!(
                    "✓ Validated 1 records\n✓ Committed .mulch/ changes: \"{expect_subject}\"\n"
                ),
                "{tag}"
            );
        }
        assert_eq!(
            git(&ours.0, &["log", "-1", "--format=%s"]),
            format!("{expect_subject}\n"),
            "{tag}"
        );
        assert_eq!(
            git(&ours.0, &["status", "--porcelain", ".mulch/"]),
            "",
            "{tag}: .mulch committed"
        );
    }
}

/// The validate gate: a store with findings fails with the frozen
/// findings text, exits 1, and commits NOTHING (the dirt stays).
#[test]
fn sync_validation_failure_commits_nothing() {
    let config = "version: '1'\ndomains:\n  dev: {}\n";
    let bad = "# a comment line is not JSON here\n{\"type\":\"nope\",\"name\":\"x\"}\n";
    let ours = scratch_git_store("sync-invalid", config, &[("dev", "")]);
    std::fs::write(ours.0.join(".mulch/expertise/dev.jsonl"), bad).expect("dirty");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["sync"]);
    assert_eq!(our.code, 1);
    assert_eq!(
        our.stderr,
        "Validation failed:\n  dev:1 - Invalid JSON: failed to parse\n  dev:2 - Unknown record \
         type \"nope\". Register it under custom_types in mulch.config.yaml or remove the \
         record.\n\nFix errors and retry, or use --no-validate to skip.\n"
    );
    assert_ne!(
        git(&ours.0, &["status", "--porcelain", ".mulch/"]),
        "",
        "nothing was committed"
    );
    // and --no-validate walks past the findings to the commit
    let skipped = run_in(&ours.0, Path::new(mulch_bin()), &["sync", "--no-validate"]);
    assert_eq!(skipped.code, 0, "{}", skipped.stderr);
    assert_eq!(
        git(&ours.0, &["log", "-1", "--format=%s"]),
        "mulch: update expertise\n"
    );
}

/// The domain rules (probed law, pinned ungated): allowed_types and
/// required_fields wordings, `--allow-unknown-types` IGNORED by sync.
#[test]
fn sync_domain_rules_gate_the_commit() {
    let config = "version: '1'\ndomains:\n  dev:\n    allowed_types:\n      - convention\n    required_fields:\n      - evidence-commit\n";
    let body = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-26T10:00:00.000Z\",\"name\":\"p1\",\"description\":\"d\",\"id\":\"mx-cafe001\"}\n";
    let dir = scratch_git_store("sync-rules", config, &[("dev", "")]);
    std::fs::write(dir.0.join(".mulch/expertise/dev.jsonl"), body).expect("dirty");
    let run = run_in(&dir.0, Path::new(mulch_bin()), &[
        "sync",
        "--allow-unknown-types",
    ]);
    assert_eq!(run.code, 1, "{}", run.stderr);
    assert!(
        run.stderr.contains(
            "type \"pattern\" (id=mx-cafe001) is not allowed in domain \"dev\". \
             Allowed types: convention. Fix the record or relax allowed_types in \
             mulch.config.yaml."
        ),
        "{}",
        run.stderr
    );
    // The required-fields arm with an allowed type but a missing field
    let config2 = "version: '1'\ndomains:\n  dev:\n    required_fields:\n      - evidence-commit\n";
    let dir2 = scratch_git_store("sync-required", config2, &[("dev", "")]);
    std::fs::write(dir2.0.join(".mulch/expertise/dev.jsonl"), body).expect("dirty");
    let run2 = run_in(&dir2.0, Path::new(mulch_bin()), &["sync"]);
    assert_eq!(run2.code, 1, "{}", run2.stderr);
    assert!(
        run2.stderr.contains(
            "record (id=mx-cafe001) missing required field(s) \"evidence-commit\" in \
             domain \"dev\". Fix the record or relax required_fields in mulch.config.yaml."
        ),
        "{}",
        run2.stderr
    );
    assert!(
        !run.stderr.contains("Unknown record type"),
        "the escape hatch did not skip the findings"
    );
}

/// The worktree skip (ungated): inside a linked worktree the commit
/// belongs to the main repo — success, nothing staged, the skip line.
#[test]
fn sync_skips_the_commit_inside_a_worktree() {
    let main = scratch_git_store("sync-wt-main", "version: '1'\ndomains:\n  dev: {}\n", &[(
        "dev", "",
    )]);
    let linked = main.0.parent().unwrap().join(format!(
        "{}-linked",
        main.0.file_name().unwrap().to_string_lossy()
    ));
    git(&main.0, &[
        "worktree",
        "add",
        "-q",
        linked.to_string_lossy().as_ref(),
        "-b",
        "wt-branch",
    ]);
    std::fs::write(linked.join(".mulch/expertise/dev.jsonl"), "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-26T10:00:00.000Z\",\"name\":\"w\",\"description\":\"d\"}\n").expect("dirty");
    let run = run_in(&linked, Path::new(mulch_bin()), &["sync"]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(
        run.stdout.contains(
            "ℹ Skipped commit: running inside a git worktree. Changes are written to the main repo's .mulch/."
        ),
        "{}",
        run.stdout
    );
    let json = run_in(&linked, Path::new(mulch_bin()), &["--json", "sync"]);
    assert_eq!(json.code, 0, "{}", json.stderr);
    assert!(
        json.stdout.contains("\"committed\": false"),
        "{}",
        json.stdout
    );
    assert!(
        git(&main.0, &["status", "--porcelain", ".mulch/"])
            .trim()
            .is_empty(),
        "the main repo's .mulch is untouched"
    );
    std::fs::remove_dir_all(&linked).ok();
}
