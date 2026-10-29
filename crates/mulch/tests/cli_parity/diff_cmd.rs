//! Battery for `mulch diff` (mulch-176f) — OUR byte pins (ADR-0008:
//! the differential battery is dissolved; our bytes are the
//! regression memory): a GIT repo with a store and commits — across
//! the change shapes (added/modified/removed records, whole-file
//! deletion, comment lines), `--since` variants, the quiet
//! header-only law, the json envelope (full records, and its
//! unknown-type SUCCESS where plain throws), and the no-git
//! two-sentence gate. Probed live 2026-10-26
//! (/tmp/opencode/sprint54A).

use std::path::{Path, PathBuf};
use std::process::Command;

use super::support::{
    TempDir, error_envelope, mulch_bin, run_in, write_seed, write_seed_with_config,
};

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

/// A git repo with the seed store committed as c0. Returns the
/// TempDir guard and the repo path (a clone — the guard must outlive
/// use of the path).
fn git_repo(tag: &str, domains: &[(&str, &str)]) -> (TempDir, PathBuf) {
    let ours = TempDir::new(tag);
    write_seed(&ours.0, domains);
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
/// plus added, c2), and a whole-file deletion (c3) — plain and json,
/// with the exact bytes pinned on our side.
#[test]
fn diff_added_modified_removed_pin_our_bytes() {
    let (_guard, ours) = git_repo("diff-shapes", &[("dev", ""), ("mid", "")]);

    // c1: one added record in dev, one in mid (sort order + counts)
    write_file(&ours, ".mulch/expertise/dev.jsonl", &format!("{P_ONE}\n"));
    write_file(
        &ours,
        ".mulch/expertise/mid.jsonl",
        &format!("{CONV_ONE}\n"),
    );
    commit(&ours, "c1");
    let our = run_in(&ours, Path::new(mulch_bin()), &["diff"]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert_eq!(
        our.stdout,
        "\nExpertise changes since HEAD~1\n\ndev (1 change):\n  + [pattern] mx-1bb21d P One\n\nmid (1 change):\n  + [convention] mx-9d9d9d Conv one\n\n"
    );

    // c2: modify P One (removed + added) — adds render before removes
    let edited = "{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-20T10:00:00.000Z\",\"name\":\"P One EDITED\",\"description\":\"first\",\"id\":\"mx-1bb21d\"}\n";
    write_file(&ours, ".mulch/expertise/dev.jsonl", edited);
    commit(&ours, "c2");
    let our = run_in(&ours, Path::new(mulch_bin()), &["diff"]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert_eq!(
        our.stdout,
        "\nExpertise changes since HEAD~1\n\ndev (2 changes):\n  + [pattern] mx-1bb21d P One EDITED\n  - [pattern] mx-1bb21d P One\n\n"
    );

    // c3: delete dev's file — all its records are removals; the json
    // envelope carries the full record objects (pretty-printed,
    // original key order)
    write_file(&ours, ".mulch/expertise/dev.jsonl", "");
    commit(&ours, "c3");
    let our = run_in(&ours, Path::new(mulch_bin()), &["diff"]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert!(
        our.stdout.contains("dev (1 change):"),
        "removal counts: {}",
        our.stdout
    );
    let our = run_in(&ours, Path::new(mulch_bin()), &[
        "--json", "diff", "--since", "HEAD~2",
    ]);
    assert_eq!(our.code, 0, "{}", our.stderr);
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
    let (_guard, ours) = git_repo("diff-quiet", &[("dev", "")]);
    write_file(&ours, ".mulch/expertise/dev.jsonl", &format!("{P_ONE}\n"));
    commit(&ours, "c1");
    let our = run_in(&ours, Path::new(mulch_bin()), &["diff", "-q"]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert_eq!(our.stdout, "\nExpertise changes since HEAD~1\n\n");
    // no-changes under quiet: the line still prints (ungated)
    let our = run_in(&ours, Path::new(mulch_bin()), &[
        "diff", "-q", "--since", "HEAD",
    ]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert_eq!(our.stdout, "No expertise changes found.\n");
}

/// `--since` variants and the empty reads: a clean tree, a MISSING
/// ref (the git read law: failure reads as empty — exit 0, no error),
/// and comment lines skipped by the parser.
#[test]
fn diff_since_variants_and_empty_reads_pin_our_bytes() {
    let (_guard, ours) = git_repo("diff-since", &[("dev", "")]);
    // clean tree: no changes since HEAD~1 (c0 is the only commit)
    let our = run_in(&ours, Path::new(mulch_bin()), &["diff"]);
    assert_eq!(our.code, 0);
    assert_eq!(our.stdout, "No expertise changes found.\n");
    let our = run_in(&ours, Path::new(mulch_bin()), &["--json", "diff"]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert_eq!(
        our.stdout,
        "{\n  \"success\": true,\n  \"command\": \"diff\",\n  \"since\": \"HEAD~1\",\n  \"domains\": [],\n  \"message\": \"No expertise changes found.\"\n}\n"
    );

    // a missing ref degrades to the same empty read
    let our = run_in(&ours, Path::new(mulch_bin()), &[
        "diff",
        "--since",
        "nosuchref",
    ]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert_eq!(our.stdout, "No expertise changes found.\n");

    // c1: comment lines in the jsonl are skipped by the parser
    write_file(
        &ours,
        ".mulch/expertise/dev.jsonl",
        &format!(
            "# archived banner\n{P_ONE}\n{{\"type\":\"pattern\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-22T10:00:00.000Z\",\"name\":\"P Two\",\"description\":\"second\",\"id\":\"mx-2a2a2a\"}}\n"
        ),
    );
    commit(&ours, "c2");
    let our = run_in(&ours, Path::new(mulch_bin()), &["diff"]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    // both records are additions against the empty c0 store; the
    // comment line never renders
    assert!(our.stdout.contains("dev (2 changes):"), "{}", our.stdout);
    assert!(our.stdout.contains("P Two"));
    assert!(!our.stdout.contains("archived banner"), "{}", our.stdout);
}

/// The unknown-type asymmetry: plain THROWS after the entry header
/// already printed (stdout keeps the partial render, stderr carries
/// `Error: Unknown record type: …`); `--json` SUCCEEDS with the full
/// records.
#[test]
fn diff_unknown_type_plain_throws_json_succeeds() {
    let (_guard, ours) = git_repo("diff-unktype", &[("dev", "")]);
    write_file(
        &ours,
        ".mulch/expertise/dev.jsonl",
        "{\"type\":\"zzzcustom\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-20T10:00:00.000Z\",\"name\":\"U\",\"description\":\"u\",\"id\":\"mx-uuu001\"}\n",
    );
    commit(&ours, "c1");
    let our = run_in(&ours, Path::new(mulch_bin()), &["diff"]);
    assert_eq!(our.code, 1);
    assert_eq!(
        our.stdout,
        "\nExpertise changes since HEAD~1\n\ndev (1 change):\n"
    );
    assert_eq!(our.stderr, "Error: Unknown record type: zzzcustom\n");

    let our = run_in(&ours, Path::new(mulch_bin()), &["--json", "diff"]);
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
fn diff_no_git_gate_pins_our_two_sentences() {
    let (_guard, ours) = git_repo("diff-nogit", &[("dev", P_ONE)]);
    // strip the git machinery, keep the store
    std::fs::remove_dir_all(ours.join(".git")).expect("git dir removed");
    let our = run_in(&ours, Path::new(mulch_bin()), &["diff"]);
    assert_eq!(our.code, 1);
    assert_eq!(our.stderr, "Error: not in a git repository.\n");
    let our = run_in(&ours, Path::new(mulch_bin()), &["--json", "diff"]);
    assert_eq!(our.code, 1);
    assert_eq!(our.stdout, "");
    assert_eq!(
        our.stderr,
        error_envelope(
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
    let (_guard, ours) = git_repo("diff-idless", &[("dev", "")]);
    write_file(
        &ours,
        ".mulch/expertise/dev.jsonl",
        "{\"type\":\"convention\",\"classification\":\"tactical\",\"recorded_at\":\"2026-10-20T10:00:00.000Z\",\"content\":\"no id conv\"}\n",
    );
    commit(&ours, "c1");
    let our = run_in(&ours, Path::new(mulch_bin()), &["diff"]);
    assert_eq!(our.code, 0, "{}", our.stderr);
    assert_eq!(
        our.stdout,
        "\nExpertise changes since HEAD~1\n\ndev (1 change):\n  + [convention]  no id conv\n\n"
    );

    // broken custom_types: fails through the global surface (before
    // the git check even — the registry bootstrap)
    let (_guard, ours) = git_repo("diff-broken", &[("dev", "")]);
    let config =
        "version: '1'\ndomains:\n  dev: {}\ncustom_types:\n  bad:\n    required: [content]\n";
    write_seed_with_config(&ours, config, &[("dev", "")]);
    let our = run_in(&ours, Path::new(mulch_bin()), &["diff"]);
    assert_eq!(our.code, 1);
    assert!(
        our.stderr
            .starts_with("Config error: Custom type \"bad\" must set \"dedup_key\""),
        "{}",
        our.stderr
    );
}

/// The documented sort DEVIATION (README): we sort domains by BYTES,
/// not locale collation — observably different for MIXED case (`Beta`
/// before `asset` here). This pin holds OUR order so the divergence
/// is a decision, not an accident.
#[test]
fn diff_sorts_domains_by_bytes_not_locale_collation() {
    let dir = TempDir::new("diff-mixed-case");
    let config = "version: '1'\ndomains:\n  asset: {}\n  Beta: {}\n";
    write_seed_with_config(&dir.0, config, &[("asset", ""), ("Beta", "")]);
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
    // BYTE order: `Beta` (0x42) before `asset` (0x61) — locale
    // collation would print asset first.
    let beta_at = run.stdout.find("Beta").expect("Beta section");
    let asset_at = run.stdout.find("asset").expect("asset section");
    assert!(beta_at < asset_at, "byte order: {}", run.stdout);
}
