//! Differential battery for `mulch diff` (mulch-176f): twin GIT
//! repos with mirrored stores and commits — ours vs the reference
//! `ml` — across the change shapes (added/modified/removed records,
//! whole-file deletion, comment lines), `--since` variants, the
//! quiet header-only law, the json envelope (full records, and its
//! unknown-type SUCCESS where plain throws), and the no-git
//! two-sentence gate. Probed live 2026-10-26
//! (/tmp/opencode/sprint54A). The twin-git helpers mirror learn.rs's
//! private copies (a hoist into support.rs is a noted follow-up for
//! the host).

use std::path::Path;
use std::process::Command;

use super::support::{TempDir, mulch_bin, reference_ml, run_in, write_seed};

/// Deterministic git in a fixture dir: fixed identity and dates.
fn run_git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_DATE", "2026-10-26T10:00:00 +0000")
        .env("GIT_COMMITTER_DATE", "2026-10-26T10:00:00 +0000")
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Twin git repos with the seed store committed as c0.
fn twin_git(tag: &str, domains: &[(&str, &str)]) -> (TempDir, TempDir) {
    let ours = TempDir::new(format!("{tag}-o").as_str());
    let theirs = TempDir::new(format!("{tag}-t").as_str());
    for dir in [&ours.0, &theirs.0] {
        write_seed(dir, domains);
        run_git(dir, &["init", "-q"]);
        run_git(dir, &["config", "user.email", "t@example.io"]);
        run_git(dir, &["config", "user.name", "Twin"]);
        run_git(dir, &["add", "-A"]);
        run_git(dir, &["commit", "-qm", "c0"]);
    }
    (ours, theirs)
}

/// Commits the current tree in BOTH twins.
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

/// Runs `diff` in both twins and asserts channel-split byte equality.
fn assert_diff_parity(ml: &Path, ours: &TempDir, theirs: &TempDir, args: &[&str]) {
    let our = run_in(&ours.0, Path::new(mulch_bin()), args);
    let their = run_in(&theirs.0, ml, args);
    assert_eq!(our.code, their.code, "{args:?} exit code");
    assert_eq!(our.stdout, their.stdout, "{args:?} stdout");
    assert_eq!(our.stderr, their.stderr, "{args:?} stderr");
}

/// The seeded dev records (fixed ids/timestamps for byte-stable
/// envelopes).
const P_ONE: &str = "{\
\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-20T10:00:00.000Z\",\
\"name\":\"P One\",\"description\":\"first\",\"id\":\"mx-1bb21d\"\
}";

const CONV_ONE: &str = "{\
\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-21T10:00:00.000Z\",\
\"content\":\"Conv one\",\"id\":\"mx-9d9d9d\"\
}";

/// The change shapes: added records (c1), a modified record (removed
/// plus added, c2), and a whole-file deletion (c3) — plain and json
/// parity, with the exact probed bytes pinned on our side.
#[test]
fn diff_added_modified_removed_match_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_git("diff-shapes", &[("dev", ""), ("mid", "")]);
    let dirs = [&ours, &theirs];

    // c1: one added record in dev, one in mid (sort order + counts)
    twin_write(dirs, ".mulch/expertise/dev.jsonl", &format!("{P_ONE}\n"));
    twin_write(dirs, ".mulch/expertise/mid.jsonl", &format!("{CONV_ONE}\n"));
    twin_commit(dirs, "c1");
    assert_diff_parity(&ml, &ours, &theirs, &["diff"]);
    assert_diff_parity(&ml, &ours, &theirs, &["--json", "diff"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["diff"]);
    assert_eq!(
        our.stdout,
        "\nExpertise changes since HEAD~1\n\ndev (1 change):\n  + [pattern] mx-1bb21d P One\n\nmid (1 change):\n  + [convention] mx-9d9d9d Conv one\n\n"
    );

    // c2: modify P One (removed + added) — adds render before removes
    let edited = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-20T10:00:00.000Z\",\"name\":\"P One EDITED\",\"description\":\"first\",\"id\":\"mx-1bb21d\"}\n";
    twin_write(dirs, ".mulch/expertise/dev.jsonl", edited);
    twin_commit(dirs, "c2");
    assert_diff_parity(&ml, &ours, &theirs, &["diff"]);
    assert_diff_parity(&ml, &ours, &theirs, &["--json", "diff"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["diff"]);
    assert_eq!(
        our.stdout,
        "\nExpertise changes since HEAD~1\n\ndev (2 changes):\n  + [pattern] mx-1bb21d P One EDITED\n  - [pattern] mx-1bb21d P One\n\n"
    );

    // c3: delete dev's file — all its records are removals; the json
    // envelope carries the full record objects (pretty-printed,
    // original key order)
    twin_write(dirs, ".mulch/expertise/dev.jsonl", "");
    twin_commit(dirs, "c3");
    assert_diff_parity(&ml, &ours, &theirs, &["diff"]);
    assert_diff_parity(&ml, &ours, &theirs, &[
        "--json", "diff", "--since", "HEAD~2",
    ]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json", "diff", "--since", "HEAD~2",
    ]);
    let record: serde_json::Value = serde_json::from_str(P_ONE).expect("record parses");
    let expected = serde_json::to_string_pretty(&serde_json::json!({
        "success": true,
        "command": "diff",
        "since": "HEAD~2",
        "domains": [
            {
                "domain": "dev",
                "added": [],
                "removed": [record]
            }
        ]
    }))
    .expect("serializes");
    assert_eq!(our.stdout, format!("{expected}\n"));
}

/// The quiet law: `-q` prints EXACTLY the header (the no-changes line
/// stays ungated on top of that).
#[test]
fn diff_quiet_prints_only_the_header() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_git("diff-quiet", &[("dev", "")]);
    let dirs = [&ours, &theirs];
    twin_write(dirs, ".mulch/expertise/dev.jsonl", &format!("{P_ONE}\n"));
    twin_commit(dirs, "c1");
    assert_diff_parity(&ml, &ours, &theirs, &["diff", "-q"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["diff", "-q"]);
    assert_eq!(our.stdout, "\nExpertise changes since HEAD~1\n\n");
    // no-changes under quiet: the line still prints (ungated)
    assert_diff_parity(&ml, &ours, &theirs, &["diff", "-q", "--since", "HEAD"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "diff", "-q", "--since", "HEAD",
    ]);
    assert_eq!(our.stdout, "No expertise changes found.\n");
}

/// `--since` variants and the empty reads: a clean tree, a MISSING
/// ref (the git read law: failure reads as empty — exit 0, no error),
/// a sha, and the json no-changes envelope with its message.
#[test]
fn diff_since_variants_and_empty_reads_match() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_git("diff-since", &[("dev", "")]);
    let dirs = [&ours, &theirs];
    // clean tree: no changes since HEAD~1 (c0 is the only commit)
    assert_diff_parity(&ml, &ours, &theirs, &["diff"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["diff"]);
    assert_eq!(our.stdout, "No expertise changes found.\n");
    assert_eq!(our.code, 0);
    assert_diff_parity(&ml, &ours, &theirs, &["--json", "diff"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "diff"]);
    assert_eq!(
        our.stdout,
        "{\n  \"success\": true,\n  \"command\": \"diff\",\n  \"since\": \"HEAD~1\",\n  \"domains\": [],\n  \"message\": \"No expertise changes found.\"\n}\n"
    );

    // a missing ref degrades to the same empty read
    assert_diff_parity(&ml, &ours, &theirs, &["diff", "--since", "nosuchref"]);

    // c1 + a sha-pinned range
    twin_write(dirs, ".mulch/expertise/dev.jsonl", &format!("{P_ONE}\n"));
    twin_commit(dirs, "c1");
    let sha = run_git(&ours.0, &["rev-parse", "HEAD~1"])
        .trim()
        .to_string();
    assert_diff_parity(&ml, &ours, &theirs, &["diff", "--since", &sha]);
    assert_diff_parity(&ml, &ours, &theirs, &["diff", "--since", "HEAD~1"]);

    // comment lines in the jsonl are skipped by both parsers
    twin_write(
        dirs,
        ".mulch/expertise/dev.jsonl",
        &format!(
            "# archived banner\n{P_ONE}\n{{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-22T10:00:00.000Z\",\"name\":\"P Two\",\"description\":\"second\",\"id\":\"mx-2a2a2a\"}}\n"
        ),
    );
    twin_commit(dirs, "c2");
    assert_diff_parity(&ml, &ours, &theirs, &["diff"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["diff"]);
    assert!(our.stdout.contains("dev (1 change):"));
    assert!(our.stdout.contains("P Two"));
    assert!(!our.stdout.contains("archived banner"), "{}", our.stdout);
}

/// The unknown-type asymmetry: plain THROWS after the entry header
/// already printed (stdout keeps the partial render, stderr carries
/// `Error: Unknown record type: …`); `--json` SUCCEEDS with the full
/// records.
#[test]
fn diff_unknown_type_plain_throws_json_succeeds() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_git("diff-unktype", &[("dev", "")]);
    let dirs = [&ours, &theirs];
    twin_write(
        dirs,
        ".mulch/expertise/dev.jsonl",
        "{\"type\":\"zzzcustom\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-20T10:00:00.000Z\",\"name\":\"U\",\"description\":\"u\",\"id\":\"mx-uuu001\"}\n",
    );
    twin_commit(dirs, "c1");
    assert_diff_parity(&ml, &ours, &theirs, &["diff"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["diff"]);
    assert_eq!(our.code, 1);
    assert_eq!(
        our.stdout,
        "\nExpertise changes since HEAD~1\n\ndev (1 change):\n"
    );
    assert_eq!(our.stderr, "Error: Unknown record type: zzzcustom\n");

    assert_diff_parity(&ml, &ours, &theirs, &["--json", "diff"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "diff"]);
    assert_eq!(our.code, 0);
    assert_eq!(our.stderr, "");
    assert!(
        our.stdout.contains("\"type\": \"zzzcustom\""),
        "{}",
        our.stdout
    );
}

/// The no-git gate: plain's `Error: not in a git repository.` vs
/// json's remediation sentence in the stderr envelope — the same
/// two-sentence pair as `learn` (with a store present, proving the
/// gate precedes any store read).
#[test]
fn diff_no_git_gate_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_git("diff-nogit", &[("dev", P_ONE)]);
    // strip the git machinery, keep the store
    for dir in [&ours.0, &theirs.0] {
        std::fs::remove_dir_all(dir.join(".git")).expect("git dir removed");
    }
    assert_diff_parity(&ml, &ours, &theirs, &["diff"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["diff"]);
    assert_eq!(our.code, 1);
    assert_eq!(our.stderr, "Error: not in a git repository.\n");
    assert_diff_parity(&ml, &ours, &theirs, &["--json", "diff"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["--json", "diff"]);
    assert_eq!(our.code, 1);
    assert_eq!(our.stdout, "");
    assert_eq!(
        our.stderr,
        super::support::error_envelope(
            "diff",
            "Not in a git repository. Run this command from within a git repository."
        )
    );
}

/// An id-less record keeps the double space (`  + [convention]  no
/// id conv` — probe-pinned), and a broken custom_types config fails
/// the GLOBAL config-error surface before diff's own logic.
#[test]
fn diff_id_less_record_and_broken_custom_types() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_git("diff-idless", &[("dev", "")]);
    let dirs = [&ours, &theirs];
    twin_write(
        dirs,
        ".mulch/expertise/dev.jsonl",
        "{\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-20T10:00:00.000Z\",\"content\":\"no id conv\"}\n",
    );
    twin_commit(dirs, "c1");
    assert_diff_parity(&ml, &ours, &theirs, &["diff"]);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["diff"]);
    assert_eq!(
        our.stdout,
        "\nExpertise changes since HEAD~1\n\ndev (1 change):\n  + [convention]  no id conv\n\n"
    );

    // broken custom_types: both fail through the global surface
    // (before the git check even — the registry bootstrap)
    let (ours, theirs) = twin_git("diff-broken", &[("dev", "")]);
    let config =
        "version: '1'\ndomains:\n  dev: {}\ncustom_types:\n  bad:\n    required: [content]\n";
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(dir.join(".mulch").join("mulch.config.yaml"), config)
            .expect("broken config");
    }
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["diff"]);
    let their = run_in(&theirs.0, &ml, &["diff"]);
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

/// The documented sort DEVIATION (README): the reference sorts domains
/// with localeCompare, we by bytes — identical for same-case names,
/// observably different for MIXED case (`Beta` before `asset` here).
/// This pin holds OUR order so the divergence is a decision, not an
/// accident; a same-case twin stays reference-covered above.
#[test]
fn diff_sorts_domains_by_bytes_not_locale_collation() {
    let dir = TempDir::new("diff-mixed-case");
    let config = "version: '1'\ndomains:\n  asset: {}\n  Beta: {}\n";
    super::support::write_seed_with_config(&dir.0, config, &[("asset", ""), ("Beta", "")]);
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .args(["-c", "core.hooksPath=/dev/null"])
            .args(args)
            .current_dir(&dir.0)
            .output()
            .expect("fixture git");
        assert!(output.status.success(), "fixture git {args:?}");
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    git(&["init", "-q"]);
    git(&["config", "user.email", "t@t"]);
    git(&["config", "user.name", "t"]);
    git(&["add", ".mulch/"]);
    git(&["commit", "-qm", "seed"]);
    let body = |name: &str| {
        format!(
            "{{\"type\":\"pattern\",\"classification\":\"tactical\",\
             \"recorded_at\":\"2026-10-26T10:00:00.000Z\",\"name\":\"{name}\",\
             \"description\":\"d\"}}\n"
        )
    };
    std::fs::write(dir.0.join(".mulch/expertise/asset.jsonl"), body("a")).expect("asset");
    std::fs::write(dir.0.join(".mulch/expertise/Beta.jsonl"), body("b")).expect("Beta");
    git(&["add", ".mulch/"]);
    git(&["commit", "-qm", "records"]);
    let run = run_in(&dir.0, Path::new(mulch_bin()), &[
        "diff", "--since", "HEAD~1",
    ]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    // BYTE order: `Beta` (0x42) before `asset` (0x61) — the reference's
    // collation would print asset first.
    let beta_at = run.stdout.find("Beta").expect("Beta section");
    let asset_at = run.stdout.find("asset").expect("asset section");
    assert!(beta_at < asset_at, "byte order: {}", run.stdout);
}
