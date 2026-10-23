//! Differential battery for `mulch learn` (mulch-16da): twin GIT
//! repos with mirrored stores and commits — ours vs the reference
//! `ml` — across the changed-file mixes (committed/staged/unstaged,
//! untracked never appears), --since variants, the suggestion sort,
//! and the no-git / no-store / no-changes / strict-read surfaces.
//! Probed live 2026-10-23 (/tmp/opencode/childC).

use std::path::Path;
use std::process::Command;

use super::support::{
    TempDir, error_envelope, mulch_bin, normalize_dir, reference_ml, run_in, twin_seeded,
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

/// Twin git repos with a mirrored seed store committed as the first
/// commit: `git diff HEAD~1` fails (first commit), staged/unstaged
/// still collect.
fn twin_git(tag: &str, config: Option<&str>, domains: &[(&str, &str)]) -> (TempDir, TempDir) {
    let (ours, theirs) = twin_seeded(tag, config, domains, &[]);
    for dir in [&ours.0, &theirs.0] {
        run_git(dir, &["init", "-q"]);
        run_git(dir, &["config", "user.email", "t@example.io"]);
        run_git(dir, &["config", "user.name", "Twin"]);
        run_git(dir, &["add", "-A"]);
        run_git(dir, &["commit", "-qm", "c0"]);
    }
    (ours, theirs)
}

/// Commits files in BOTH twins (content mirrored).
fn twin_commit(dirs: [&TempDir; 2], message: &str) {
    for dir in dirs {
        run_git(&dir.0, &["add", "-A"]);
        run_git(&dir.0, &["commit", "-qm", message]);
    }
}

/// Writes the same file bytes in both twins.
fn twin_write(dirs: [&TempDir; 2], relative: &str, body: &str) {
    for dir in dirs {
        let path = dir.0.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("dirs");
        }
        std::fs::write(path, body).expect("write");
    }
}

/// Our record hint names THIS binary (promote law, user directive
/// 2026-10-13); byte comparisons normalize the name (the hint line
/// carries no backticks — plain `mulch record` → `ml record`).
fn promote_ours(text: &str) -> String {
    text.replace("mulch record", "ml record")
}

/// Runs `learn` in both twins and asserts channel-split byte equality
/// (our stdout promoted back to the reference's binary name).
fn assert_learn_parity(ml: &Path, ours: &TempDir, theirs: &TempDir, args: &[&str]) {
    let our = run_in(&ours.0, Path::new(mulch_bin()), args);
    let their = run_in(&theirs.0, ml, args);
    assert_eq!(our.code, their.code, "{args:?} exit code");
    assert_eq!(
        promote_ours(&our.stdout),
        their.stdout,
        "{args:?} stdout (ours promoted)"
    );
    assert_eq!(
        normalize_dir(&our.stderr, &ours.0),
        normalize_dir(&their.stderr, &theirs.0),
        "{args:?} stderr"
    );
}

/// The matching + sorting core: suggestions order by match count
/// descending (mid 2 before zeta 1 despite config order), matched and
/// unmatched changed files list sorted, exact plain bytes and json.
#[test]
fn learn_suggests_and_sorts_domains_by_match_count() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_git("learn-sort", None, &[
        ("zeta", ZETA_SEED),
        ("mid", MID_SEED),
    ]);
    let dirs = [&ours, &theirs];
    twin_write(dirs, "src/a.rs", "a\n");
    twin_write(dirs, "lib/x.ts", "x\n");
    twin_write(dirs, "lib/y.ts", "y\n");
    twin_write(dirs, "docs/other.md", "o\n");
    twin_commit(dirs, "c1");

    assert_learn_parity(&ml, &ours, &theirs, &["learn"]);
    assert_learn_parity(&ml, &ours, &theirs, &["--json", "learn"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["learn"]);
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
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "learn"]);
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
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    // staged-only in a first-commit repo
    let (ours, theirs) = twin_git("learn-staged", None, &[("zeta", ZETA_SEED), ("mid", "")]);
    let dirs = [&ours, &theirs];
    twin_write(dirs, "src/a.rs", "a\n");
    for dir in dirs {
        run_git(&dir.0, &["add", "src/a.rs"]);
    }
    assert_learn_parity(&ml, &ours, &theirs, &["learn"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["learn"]);
    assert!(our.stdout.contains("Changed files (1):"));
    assert!(our.stdout.contains("src/a.rs"));

    // unstaged edit of a tracked file + an untracked file (absent)
    twin_commit(dirs, "c1");
    twin_write(dirs, "src/a.rs", "changed\n");
    twin_write(dirs, "notes/untracked.md", "n\n");
    assert_learn_parity(&ml, &ours, &theirs, &["learn"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["learn"]);
    assert!(our.stdout.contains("src/a.rs"));
    assert!(!our.stdout.contains("untracked.md"), "{}", our.stdout);
}

/// `--since` variants: the HEAD~1 default, an explicit older ref, a
/// sha, and a MISSING ref degrading to the staged/unstaged reads
/// (exit 0, no error — probed).
#[test]
fn learn_since_variants_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_git("learn-since", None, &[("zeta", ZETA_SEED), ("mid", "")]);
    let dirs = [&ours, &theirs];
    twin_write(dirs, "src/a.rs", "a\n");
    twin_commit(dirs, "c1");
    twin_write(dirs, "lib/x.ts", "x\n");
    twin_commit(dirs, "c2");
    twin_write(dirs, "src/b.rs", "b\n");
    twin_commit(dirs, "c3");

    // default HEAD~1: only src/b.rs changed since c2
    assert_learn_parity(&ml, &ours, &theirs, &["learn"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["learn"]);
    assert!(our.stdout.contains("Changed files (1):"));
    assert!(our.stdout.contains("src/b.rs"));

    // HEAD~2 reaches back to c1; a sha pins the same range
    assert_learn_parity(&ml, &ours, &theirs, &["learn", "--since", "HEAD~2"]);
    assert_learn_parity(&ml, &ours, &theirs, &["learn", "--since", "HEAD"]);
    let sha = run_git(&ours.0, &["rev-parse", "HEAD~2"])
        .trim()
        .to_string();
    assert_learn_parity(&ml, &ours, &theirs, &["learn", "--since", &sha]);
    // a missing ref degrades to empty (clean tree → no changes)
    assert_learn_parity(&ml, &ours, &theirs, &["learn", "--since", "nosuchref"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "learn",
        "--since",
        "nosuchref",
    ]);
    assert_eq!(
        our.stdout,
        "No changed files found. Nothing to learn from.\n"
    );
}

/// The quiet paths: a clean tree (no changes, plain + json message
/// envelope) and an empty store (all files unmatched, no suggestions
/// section).
#[test]
fn learn_no_changes_and_empty_store_match() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_git("learn-clean", None, &[("zeta", ZETA_SEED), ("mid", "")]);
    assert_learn_parity(&ml, &ours, &theirs, &["learn"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["learn"]);
    assert_eq!(
        our.stdout,
        "No changed files found. Nothing to learn from.\n"
    );
    assert_learn_parity(&ml, &ours, &theirs, &["--json", "learn"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "learn"]);
    assert_eq!(
        our.stdout,
        "{\n  \"success\": true,\n  \"command\": \"learn\",\n  \"changedFiles\": [],\n  \"suggestedDomains\": [],\n  \"unmatchedFiles\": [],\n  \"message\": \"No changed files found\"\n}\n"
    );

    // records-less store: everything unmatched, no Suggested section
    let (ours, theirs) = twin_git("learn-norecords", None, &[("zeta", ""), ("mid", "")]);
    let dirs = [&ours, &theirs];
    twin_write(dirs, "src/a.rs", "a\n");
    twin_commit(dirs, "c1");
    assert_learn_parity(&ml, &ours, &theirs, &["learn"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["learn"]);
    assert!(
        our.stdout
            .contains("Unmatched files (no domain association):")
    );
    assert!(!our.stdout.contains("Suggested domains:"));
}

/// The gate paths: not a git repository (plain/json wordings differ
/// by design), a git repo without `.mulch/` (plain SILENT exit 0,
/// json skip envelope), and `.mulch/` without a config (the readConfig
/// wording — but only once files changed; a clean tree still says no
/// changes).
#[test]
fn learn_git_and_store_gates_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    // not a git repository (with a store — the git check is first)
    let (ours, theirs) = twin_seeded("learn-nogit", None, &[("zeta", ZETA_SEED)], &[]);
    assert_learn_parity(&ml, &ours, &theirs, &["learn"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["learn"]);
    assert_eq!(our.code, 1);
    assert_eq!(our.stderr, "Error: not in a git repository.\n");
    assert_learn_parity(&ml, &ours, &theirs, &["--json", "learn"]);
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
    let ours = TempDir::new("learn-nostore-o");
    let theirs = TempDir::new("learn-nostore-t");
    for dir in [&ours.0, &theirs.0] {
        run_git(dir, &["init", "-q"]);
        run_git(dir, &["config", "user.email", "t@example.io"]);
        run_git(dir, &["config", "user.name", "Twin"]);
        std::fs::write(dir.join("f.txt"), "f\n").expect("write");
        run_git(dir, &["add", "-A"]);
        run_git(dir, &["commit", "-qm", "c0"]);
        std::fs::write(dir.join("g.txt"), "g\n").expect("write");
        run_git(dir, &["add", "g.txt"]);
    }
    assert_learn_parity(&ml, &ours, &theirs, &["learn"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["learn"]);
    assert_eq!(our.code, 0);
    assert_eq!(our.stdout, "");
    assert_eq!(our.stderr, "");
    assert_learn_parity(&ml, &ours, &theirs, &["--json", "learn"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "learn"]);
    assert_eq!(
        our.stdout,
        "{\n  \"success\": true,\n  \"command\": \"learn\",\n  \"changedFiles\": [],\n  \"suggestedDomains\": [],\n  \"unmatchedFiles\": [],\n  \"message\": \"No .mulch/ directory found — skipping\"\n}\n"
    );

    // .mulch/ without a config: clean tree → no changes; staged file
    // → the readConfig wording (matchFilesToDomains reaches for it).
    // The amend folds g.txt into c0 so HEAD~1 stays missing (a second
    // commit would diff g.txt back in through `git diff HEAD~1`).
    for dir in [&ours.0, &theirs.0] {
        std::fs::create_dir_all(dir.join(".mulch")).expect("mulch dir");
        run_git(dir, &["commit", "-q", "--amend", "-m", "c0"]);
    }
    assert_learn_parity(&ml, &ours, &theirs, &["learn"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["learn"]);
    assert_eq!(
        our.stdout,
        "No changed files found. Nothing to learn from.\n"
    );
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(dir.join("h.txt"), "h\n").expect("write");
        run_git(dir, &["add", "h.txt"]);
    }
    assert_learn_parity(&ml, &ours, &theirs, &["learn"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["learn"]);
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
fn learn_strict_reads_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let malformed = format!("{MID_SEED}{{\"type\":\"pattern\", BAD\n");
    let (ours, theirs) = twin_git("learn-malformed", None, &[
        ("zeta", ZETA_SEED),
        ("mid", malformed.as_str()),
    ]);
    let dirs = [&ours, &theirs];
    twin_write(dirs, "src/a.rs", "a\n");
    twin_commit(dirs, "c1");
    for args in [&["learn"] as &[&str], &["--json", "learn"]] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), args);
        let their = run_in(&theirs.0, ml.as_path(), args);
        assert_eq!(our.code, their.code);
        assert_eq!(our.code, 1);
        assert_eq!(our.stdout, their.stdout);
        let our_stderr = normalize_dir(&our.stderr, &ours.0);
        let their_stderr = normalize_dir(&their.stderr, &theirs.0);
        // plain carries the Error: line (raw preview tail); json the
        // envelope (JSON-escaped preview) — bytes already equal, so
        // only the template head pins per mode
        let head = if args[0] == "--json" {
            "{\n  \"success\": false,\n  \"command\": \"learn\",\n  \"error\": \"Malformed JSONL at <DIR>/.mulch/expertise/mid.jsonl:2: ".to_string()
        } else {
            "Error: Malformed JSONL at <DIR>/.mulch/expertise/mid.jsonl:2: ".to_string()
        };
        assert!(our_stderr.starts_with(&head), "our template: {our_stderr}");
        assert!(
            their_stderr.starts_with(&head),
            "their template: {their_stderr}"
        );
        if args.first() != Some(&"--json") {
            assert!(our_stderr.ends_with(". Line: {\"type\":\"pattern\", BAD\n"));
        }
    }

    let unknown = "{\
\"type\":\"zzzcustom\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-03T20:47:59.000Z\",\
\"name\":\"Odd\",\"files\":[\"odd/file.ts\"],\"id\":\"mx-zzz001\"\
}\n";
    let (ours, theirs) = twin_git("learn-unktype", None, &[
        ("zeta", ZETA_SEED),
        ("mid", unknown),
    ]);
    let dirs = [&ours, &theirs];
    twin_write(dirs, "odd/file.ts", "o\n");
    twin_commit(dirs, "c1");
    assert_learn_parity(&ml, &ours, &theirs, &["learn"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["learn"]);
    assert_eq!(our.code, 1);
    assert!(normalize_dir(&our.stderr, &ours.0).starts_with(
        "Error: Unknown record type \"zzzcustom\" at <DIR>/.mulch/expertise/mid.jsonl:1"
    ));

    // bypassed: the odd file stays UNMATCHED (no registry row → no
    // file extraction)
    assert_learn_parity(&ml, &ours, &theirs, &["--allow-unknown-types", "learn"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--allow-unknown-types",
        "learn",
    ]);
    assert_eq!(our.code, 0);
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
fn learn_suffix_law_and_custom_types_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_git("learn-suffix", None, &[("zeta", ZETA_SEED), ("mid", "")]);
    let dirs = [&ours, &theirs];
    twin_write(dirs, "deep/src/a.rs", "a\n");
    twin_commit(dirs, "c1");
    assert_learn_parity(&ml, &ours, &theirs, &["learn"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["learn"]);
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
    let (ours, theirs) = twin_git("learn-custom", Some(config), &[("ops", seed)]);
    let dirs = [&ours, &theirs];
    twin_write(dirs, "k8s/api.yml", "k\n");
    twin_commit(dirs, "c1");
    assert_learn_parity(&ml, &ours, &theirs, &["learn"]);
    assert_learn_parity(&ml, &ours, &theirs, &["--json", "learn"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["learn"]);
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
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let config =
        "version: '1'\ndomains:\n  zeta: {}\ncustom_types:\n  bad:\n    required: [content]\n";
    let (ours, theirs) = twin_seeded("learn-broken", Some(config), &[("zeta", "")], &[]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["learn"]);
    let their = run_in(&theirs.0, &ml, &["learn"]);
    assert_eq!(our.code, their.code);
    assert_eq!(our.code, 1);
    assert_eq!(our.stderr, their.stderr);
    assert!(
        our.stderr
            .starts_with("Config error: Custom type \"bad\" must set \"dedup_key\""),
        "{}",
        our.stderr
    );
}
