//! `mulch init` (and re-init) parity probes.
//!
//! Moved verbatim from the one-file battery (mulch-3e7c); sprint
//! banners stay with their probes.

use std::path::Path;

use super::support::*;

#[test]
fn init_matches_reference_on_disk_and_output() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("init-ours");
    let theirs = TempDir::new("init-theirs");

    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let theirs_run = run_in(&theirs.0, &ml, &["init"]);

    assert_eq!(ours_run.code, theirs_run.code, "exit codes differ");
    assert_eq!(
        ours_run.stdout,
        theirs_run.stdout.replace(
            &theirs.0.display().to_string(),
            &ours.0.display().to_string()
        ),
        "init stdout differs"
    );
    assert_eq!(
        read_store_file(&ours.0, "README.md"),
        read_store_file(&theirs.0, "README.md")
    );
    assert_eq!(
        read_store_file(&ours.0, "mulch.config.yaml"),
        read_store_file(&theirs.0, "mulch.config.yaml")
    );
    assert!(ours.0.join(".mulch/expertise").is_dir());
}

/// Deviation pin (mulch-228d): init additionally writes
/// `.mulch/.gitignore` (`*.lock`) for the store writer lock's
/// permanent zero-byte `.lock` sibling; the reference writes no such
/// file. Everything else init writes stays byte-identical (pinned
/// above).
#[test]
fn init_writes_lock_gitignore_deviation() {
    let ours = TempDir::new("init-lockignore");
    run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    assert_eq!(
        read_store_file(&ours.0, ".gitignore"),
        "*.lock\n",
        "the lock ignore must stay stable"
    );
}

#[test]
fn init_reinit_message_matches() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("reinit-ours");
    let theirs = TempDir::new("reinit-theirs");
    run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    run_in(&theirs.0, &ml, &["init"]);

    let ours_run = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let theirs_run = run_in(&theirs.0, &ml, &["init"]);
    assert_eq!(ours_run.code, theirs_run.code);
    assert_eq!(ours_run.stdout, theirs_run.stdout);
}

#[test]
fn init_gitattributes_matches_reference() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    // Fresh init, pre-existing text with and without a trailing
    // newline, the line already present (idempotence), and a re-init
    // after the first write: the union-merge line lands byte-for-byte
    // on both sides in every case.
    for (tag, seed) in [
        ("fresh", None),
        ("newline", Some("custom line\n")),
        ("no-newline", Some("no-newline-x")),
        ("already", Some(".mulch/expertise/*.jsonl merge=union\n")),
    ] {
        let ours = TempDir::new(&format!("initga-{tag}-o"));
        let theirs = TempDir::new(&format!("initga-{tag}-t"));
        for dir in [&ours.0, &theirs.0] {
            if let Some(seed) = seed {
                std::fs::write(dir.join(".gitattributes"), seed).expect("seed attributes");
            }
        }
        run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
        run_in(&theirs.0, &ml, &["init"]);
        assert_eq!(
            std::fs::read_to_string(ours.0.join(".gitattributes")).expect("our attributes"),
            std::fs::read_to_string(theirs.0.join(".gitattributes")).expect("their attributes"),
            "{tag} first init"
        );
        // the second init never duplicates the line
        run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
        run_in(&theirs.0, &ml, &["init"]);
        assert_eq!(
            std::fs::read_to_string(ours.0.join(".gitattributes")).expect("our attributes"),
            std::fs::read_to_string(theirs.0.join(".gitattributes")).expect("their attributes"),
            "{tag} re-init"
        );
    }
}
