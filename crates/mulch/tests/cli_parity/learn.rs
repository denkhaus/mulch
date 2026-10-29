//! Battery for `mulch learn` (mulch-16da) — OUR byte pins
//! (ADR-0008: the differential battery is dissolved; our bytes are
//! the regression memory): a GIT repo with a store and commits —
//! across the changed-file mixes (committed/staged/unstaged,
//! untracked never appears), --since variants, the suggestion sort,
//! and the no-git / no-store / no-changes / strict-read surfaces.
//! Probed live 2026-10-23 (/tmp/opencode/childC).

use std::path::{Path, PathBuf};
use std::process::Command;

use super::support::{
    TempDir, error_envelope, mulch_bin, normalize_dir, run_in, seeded_store, write_seed_with_config,
};

/// The matching fixture: zeta owns `src/a.rs` (one file), mid owns
/// `lib/x.ts` + `lib/y.ts` (two files) — a commit touching all three
/// plus an unrelated file must sort mid BEFORE zeta (count desc, ties
/// config order).
const ZETA_SEED: &str = "{\
\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:57.000Z\",\
\"name\":\"Z One\",\"description\":\"zeta first\",\"files\":[\"src/a.rs\"],\"id\":\"mx-aaa001\"\
}\n";

const MID_SEED: &str = "{\
\"type\":\"reference\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:58.000Z\",\
\"name\":\"M Ref\",\"description\":\"mid ref\",\"files\":[\"lib/x.ts\",\"lib/y.ts\"],\"id\":\"mx-bbb001\"\
}\n";

/// Deterministic git in a fixture dir: fixed identity and dates.
fn run_git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_DATE", "2026-10-23T10:00:00 +0000")
        .env("GIT_COMMITTER_DATE", "2026-10-23T10:00:00 +0000")
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// A git repo with the seed store committed as the first commit:
/// `git diff HEAD~1` fails (first commit), staged/unstaged still
/// collect. The TempDir guard rides along.
fn git_repo(tag: &str, config: Option<&str>, domains: &[(&str, &str)]) -> (TempDir, PathBuf) {
    let ours = seeded_store(tag, config, domains, &[]);
    run_git(&ours.0, &["init", "-q"]);
    run_git(&ours.0, &["config", "user.email", "t@example.io"]);
    run_git(&ours.0, &["config", "user.name", "Twin"]);
    run_git(&ours.0, &["add", "-A"]);
    run_git(&ours.0, &["commit", "-qm", "c0"]);
    let path = ours.0.clone();
    (ours, path)
}

/// Commits the current tree.
fn commit(dir: &Path, message: &str) {
    run_git(dir, &["add", "-A"]);
    run_git(dir, &["commit", "-qm", message]);
}

/// Writes file bytes into the repo.
fn write_file(dir: &Path, relative: &str, body: &str) {
    let path = dir.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("dirs");
    }
    std::fs::write(path, body).expect("write");
}

/// The matching + sorting core: suggestions order by match count
/// descending (mid 2 before zeta 1 despite config order), matched and
/// unmatched changed files list sorted, exact plain bytes and json.
#[test]
fn learn_suggests_and_sorts_domains_by_match_count() {
    let (_guard, ours) = git_repo("learn-sort", None, &[
        ("zeta", ZETA_SEED),
        ("mid", MID_SEED),
    ]);
    write_file(&ours, "src/a.rs", "a\n");
    write_file(&ours, "lib/x.ts", "x\n");
    write_file(&ours, "lib/y.ts", "y\n");
    write_file(&ours, "docs/other.md", "o\n");
    commit(&ours, "c1");

    let our = run_in(&ours, Path::new(mulch_bin()), &["learn"]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert_eq!(
        our.stdout,
        "\nSession learnings check\n\n\
         Changed files (4):\n\
         \x20 docs/other.md\n\
         \x20 lib/x.ts\n\
         \x20 lib/y.ts\n\
         \x20 src/a.rs\n\
         \nSuggested domains:\n\
         \x20 mid (2 files match existing records)\n\
         \x20 zeta (1 file matches existing records)\n\
         \nUnmatched files (no domain association):\n\
         \x20 docs/other.md\n\
         \nRecord learnings with:\n\
         \x20 mulch record <domain> --type <type> --description \"...\"\n\
         \n"
    );
    let our = run_in(&ours, Path::new(mulch_bin()), &["--json", "learn"]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert_eq!(
        our.stdout,
        "{\n  \"success\": true,\n  \"command\": \"learn\",\n  \"changedFiles\": [\n    \"docs/other.md\",\n    \"lib/x.ts\",\n    \"lib/y.ts\",\n    \"src/a.rs\"\n  ],\n  \"suggestedDomains\": [\n    {\n      \"domain\": \"mid\",\n      \"matchCount\": 2,\n      \"files\": [\n        \"lib/x.ts\",\n        \"lib/y.ts\"\n      ]\n    },\n    {\n      \"domain\": \"zeta\",\n      \"matchCount\": 1,\n      \"files\": [\n        \"src/a.rs\"\n      ]\n    }\n  ],\n  \"unmatchedFiles\": [\n    \"docs/other.md\"\n  ]\n}\n"
    );
}

/// The changed-file mixes: staged-only (first-commit repo: HEAD~1
/// missing, staged still collects), unstaged tracked edits, and
/// untracked files NEVER appearing (diff-based collection).
#[test]
fn learn_collects_staged_unstaged_and_never_untracked() {
    // staged-only in a first-commit repo
    let (_guard, ours) = git_repo("learn-staged", None, &[("zeta", ZETA_SEED), ("mid", "")]);
    write_file(&ours, "src/a.rs", "a\n");
    run_git(&ours, &["add", "src/a.rs"]);
    let our = run_in(&ours, Path::new(mulch_bin()), &["learn"]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert!(our.stdout.contains("Changed files (1):"));
    assert!(our.stdout.contains("src/a.rs"));

    // unstaged edit of a tracked file + an untracked file (absent)
    commit(&ours, "c1");
    write_file(&ours, "src/a.rs", "changed\n");
    write_file(&ours, "notes/untracked.md", "n\n");
    let our = run_in(&ours, Path::new(mulch_bin()), &["learn"]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert!(our.stdout.contains("src/a.rs"));
    assert!(!our.stdout.contains("untracked.md"), "{}", our.stdout);
}

/// `--since` variants: the HEAD~1 default and a MISSING ref degrading
/// to the staged/unstaged reads (exit 0, no error — probed).
#[test]
fn learn_since_variants_pin_our_bytes() {
    let (_guard, ours) = git_repo("learn-since", None, &[("zeta", ZETA_SEED), ("mid", "")]);
    write_file(&ours, "src/a.rs", "a\n");
    commit(&ours, "c1");
    write_file(&ours, "lib/x.ts", "x\n");
    commit(&ours, "c2");
    write_file(&ours, "src/b.rs", "b\n");
    commit(&ours, "c3");

    // default HEAD~1: only src/b.rs changed since c2
    let our = run_in(&ours, Path::new(mulch_bin()), &["learn"]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert!(our.stdout.contains("Changed files (1):"));
    assert!(our.stdout.contains("src/b.rs"));
    // a missing ref degrades to empty (clean tree → no changes)
    let our = run_in(&ours, Path::new(mulch_bin()), &[
        "learn",
        "--since",
        "nosuchref",
    ]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert_eq!(
        our.stdout,
        "No changed files found. Nothing to learn from.\n"
    );
}

/// The quiet paths: a clean tree (no changes, plain + json message
/// envelope) and an empty store (all files unmatched, no suggestions
/// section).
#[test]
fn learn_no_changes_and_empty_store_pin_our_bytes() {
    let (_guard, ours) = git_repo("learn-clean", None, &[("zeta", ZETA_SEED), ("mid", "")]);
    let our = run_in(&ours, Path::new(mulch_bin()), &["learn"]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert_eq!(
        our.stdout,
        "No changed files found. Nothing to learn from.\n"
    );
    let our = run_in(&ours, Path::new(mulch_bin()), &["--json", "learn"]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert_eq!(
        our.stdout,
        "{\n  \"success\": true,\n  \"command\": \"learn\",\n  \"changedFiles\": [],\n  \"suggestedDomains\": [],\n  \"unmatchedFiles\": [],\n  \"message\": \"No changed files found\"\n}\n"
    );

    // records-less store: everything unmatched, no Suggested section
    let (_guard, ours) = git_repo("learn-norecords", None, &[("zeta", ""), ("mid", "")]);
    write_file(&ours, "src/a.rs", "a\n");
    commit(&ours, "c1");
    let our = run_in(&ours, Path::new(mulch_bin()), &["learn"]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert!(
        our.stdout
            .contains("Unmatched files (no domain association):")
    );
    assert!(!our.stdout.contains("Suggested domains:"));
}

/// The gate paths: not a git repository (plain/json wordings differ
/// by design), a git repo without `.mulch/` (plain SILENT exit 0,
/// json skip envelope), and `.mulch/` without a config (the
/// readConfig wording — but only once files changed; a clean tree
/// still says no changes).
#[test]
fn learn_git_and_store_gates_pin_our_bytes() {
    // not a git repository (with a store — the git check is first)
    let ours = seeded_store("learn-nogit", None, &[("zeta", ZETA_SEED)], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["learn"]);
    assert_eq!(our.code, 1);
    assert_eq!(our.stderr, "Error: not in a git repository.\n");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "learn"]);
    assert_eq!(our.code, 1);
    assert_eq!(our.stdout, "");
    assert_eq!(
        our.stderr,
        error_envelope(
            "learn",
            "Not in a git repository. Run this command from within a git repository."
        )
    );

    // a git repo WITHOUT .mulch/: plain silent, json skip envelope
    let repo = TempDir::new("learn-nostore");
    run_git(&repo.0, &["init", "-q"]);
    run_git(&repo.0, &["config", "user.email", "t@example.io"]);
    run_git(&repo.0, &["config", "user.name", "Twin"]);
    std::fs::write(repo.0.join("f.txt"), "f\n").expect("write");
    run_git(&repo.0, &["add", "-A"]);
    run_git(&repo.0, &["commit", "-qm", "c0"]);
    std::fs::write(repo.0.join("g.txt"), "g\n").expect("write");
    run_git(&repo.0, &["add", "g.txt"]);
    let our = run_in(&repo.0, Path::new(mulch_bin()), &["learn"]);
    assert_eq!(our.code, 0);
    assert_eq!(our.stdout, "");
    assert_eq!(our.stderr, "");
    let our = run_in(&repo.0, Path::new(mulch_bin()), &["--json", "learn"]);
    assert_eq!(
        our.stdout,
        "{\n  \"success\": true,\n  \"command\": \"learn\",\n  \"changedFiles\": [],\n  \"suggestedDomains\": [],\n  \"unmatchedFiles\": [],\n  \"message\": \"No .mulch/ directory found — skipping\"\n}\n"
    );

    // .mulch/ without a config: clean tree → no changes; staged file
    // → the readConfig wording (matchFilesToDomains reaches for it).
    // The amend folds g.txt into c0 so HEAD~1 stays missing (a second
    // commit would diff g.txt back in through `git diff HEAD~1`).
    std::fs::create_dir_all(repo.0.join(".mulch")).expect("mulch dir");
    run_git(&repo.0, &["commit", "-q", "--amend", "-m", "c0"]);
    let our = run_in(&repo.0, Path::new(mulch_bin()), &["learn"]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert_eq!(
        our.stdout,
        "No changed files found. Nothing to learn from.\n"
    );
    std::fs::write(repo.0.join("h.txt"), "h\n").expect("write");
    run_git(&repo.0, &["add", "h.txt"]);
    let our = run_in(&repo.0, Path::new(mulch_bin()), &["learn"]);
    assert_eq!(our.code, 1);
    assert_eq!(
        our.stderr,
        "Error: No .mulch/ directory found. Run `mulch init` to set up this project.\n"
    );
}

/// The strict-read surfaces: a malformed line anywhere fails learn;
/// an unknown type fails it too unless bypassed — and a bypassed
/// record contributes NO files (the registry row is the gate).
#[test]
fn learn_strict_reads_pin_our_bytes() {
    let malformed = format!("{MID_SEED}{{\"type\":\"pattern\", BAD\n");
    let (_guard, ours) = git_repo("learn-malformed", None, &[
        ("zeta", ZETA_SEED),
        ("mid", malformed.as_str()),
    ]);
    write_file(&ours, "src/a.rs", "a\n");
    commit(&ours, "c1");
    for args in [&["learn"] as &[&str], &["--json", "learn"]] {
        let our = run_in(&ours, Path::new(mulch_bin()), args);
        assert_eq!(our.code, 1);
        assert_eq!(our.stdout, "");
        let our_stderr = normalize_dir(&our.stderr, &ours);
        // plain carries the Error: line (raw preview tail); json the
        // envelope (JSON-escaped preview) — the template head pins
        // per mode
        let head = if args[0] == "--json" {
            "{\n  \"success\": false,\n  \"command\": \"learn\",\n  \"error\": \"Malformed JSONL at <DIR>/.mulch/expertise/mid.jsonl:2: ".to_string()
        } else {
            "Error: Malformed JSONL at <DIR>/.mulch/expertise/mid.jsonl:2: ".to_string()
        };
        assert!(our_stderr.starts_with(&head), "our template: {our_stderr}");
        if args.first() != Some(&"--json") {
            assert!(our_stderr.ends_with(". Line: {\"type\":\"pattern\", BAD\n"));
        }
    }

    let unknown = "{\
\"type\":\"zzzcustom\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:59.000Z\",\
\"name\":\"Odd\",\"files\":[\"odd/file.ts\"],\"id\":\"mx-zzz001\"\
}\n";
    let (_guard, ours) = git_repo("learn-unktype", None, &[
        ("zeta", ZETA_SEED),
        ("mid", unknown),
    ]);
    write_file(&ours, "odd/file.ts", "o\n");
    commit(&ours, "c1");
    let our = run_in(&ours, Path::new(mulch_bin()), &["learn"]);
    assert_eq!(our.code, 1);
    assert!(normalize_dir(&our.stderr, &ours).starts_with(
        "Error: Unknown record type \"zzzcustom\" at <DIR>/.mulch/expertise/mid.jsonl:1"
    ));

    // bypassed: the odd file stays UNMATCHED (no registry row → no
    // file extraction)
    let our = run_in(&ours, Path::new(mulch_bin()), &[
        "--allow-unknown-types",
        "learn",
    ]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert!(
        our.stdout
            .contains("Unmatched files (no domain association):")
    );
    assert!(our.stdout.contains("odd/file.ts"));
    assert!(!our.stdout.contains("Suggested domains:"));
}

/// The suffix matching law both directions: a changed path DEEPER
/// than the recorded one matches (`changed.endsWith(domainFile)`);
/// a custom extracts_files type feeds the same domain file set.
#[test]
fn learn_suffix_law_and_custom_types_pin_our_bytes() {
    let (_guard, ours) = git_repo("learn-suffix", None, &[("zeta", ZETA_SEED), ("mid", "")]);
    write_file(&ours, "deep/src/a.rs", "a\n");
    commit(&ours, "c1");
    let our = run_in(&ours, Path::new(mulch_bin()), &["learn"]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert!(our.stdout.contains("Suggested domains:"));
    assert!(
        our.stdout
            .contains("zeta (1 file matches existing records)")
    );

    // a custom type with extracts_files feeds domain matching
    let config = "version: '1'\ndomains:\n  ops: {}\ncustom_types:\n  deploy_note:\n    required: [service, action]\n    optional: [files]\n    dedup_key: service\n    summary: \"{service}: {action}\"\n    extracts_files: true\n";
    let seed = "{\
\"type\":\"deploy_note\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:49:00.000Z\",\
\"service\":\"api\",\"action\":\"restart\",\"files\":[\"k8s/api.yml\"],\"id\":\"mx-dep001\"\
}\n";
    let root = TempDir::new("learn-custom-w");
    let ops = root.0.join("ops");
    write_seed_with_config(&ops, config, &[("ops", seed)]);
    run_git(&ops, &["init", "-q"]);
    run_git(&ops, &["config", "user.email", "t@example.io"]);
    run_git(&ops, &["config", "user.name", "Twin"]);
    run_git(&ops, &["add", "-A"]);
    run_git(&ops, &["commit", "-qm", "c0"]);
    write_file(&ops, "k8s/api.yml", "k\n");
    commit(&ops, "c1");
    let our = run_in(&ops, Path::new(mulch_bin()), &["learn"]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert!(
        our.stdout.contains("ops (1 file matches existing records)"),
        "{}",
        our.stdout
    );
}

/// A broken custom_types block fails learn through the GLOBAL
/// config-error surface even in a non-git directory (the registry
/// bootstrap precedes every command's own logic — probed).
#[test]
fn learn_broken_custom_types_fails_the_global_surface() {
    let config =
        "version: '1'\ndomains:\n  zeta: {}\ncustom_types:\n  bad:\n    required: [content]\n";
    let ours = seeded_store("learn-broken", Some(config), &[("zeta", "")], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["learn"]);
    assert_eq!(our.code, 1);
    assert!(
        our.stderr
            .starts_with("Config error: Custom type \"bad\" must set \"dedup_key\""),
        "{}",
        our.stderr
    );
}
