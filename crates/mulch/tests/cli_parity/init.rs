//! `mulch init` (and re-init) probes — OUR byte pins (ADR-0008: the
//! parity regime against the ml executable is dissolved; the literals
//! here were frozen from our own output and are the regression
//! memory now).

use std::path::Path;

use super::support::*;

#[test]
fn init_writes_the_canonical_store_and_output() {
    let ours = TempDir::new("init-ours");
    let run = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    // the announcement names the store's parent directory
    assert_eq!(
        normalize_dir(&run.stdout, &ours.0),
        "Initialized .mulch/ in <DIR>\n"
    );
    // the config template: version + empty domains + the canonical
    // governance/shelf-life tail (CONFIG_TAIL) inline
    let config = read_store_file(&ours.0, "mulch.config.yaml");
    assert!(
        config.contains(&format!("version: '1'\ndomains: {{}}\n{CONFIG_TAIL}")),
        "template frame: {config}"
    );
    assert!(
        read_store_file(&ours.0, "README.md").starts_with("# .mulch/\n"),
        "README header"
    );
    assert!(ours.0.join(".mulch/expertise").is_dir());
}

/// Deviation pin (mulch-228d): init additionally writes
/// `.mulch/.gitignore` (`*.lock`) for the store writer lock's
/// permanent zero-byte `.lock` sibling.
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
fn init_reinit_message_is_pinned() {
    let ours = TempDir::new("reinit-ours");
    run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    let run = run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
    assert_eq!(run.code, 0);
    assert_eq!(
        run.stdout,
        "Updated .mulch/ — filled in any missing artifacts.\n"
    );
}

#[test]
fn init_gitattributes_union_merge_line_is_pinned() {
    // Fresh init, pre-existing text with and without a trailing
    // newline, the line already present (idempotence), and a re-init
    // after the first write: the union-merge line lands byte-for-byte
    // in every case and never duplicates.
    const UNION_LINE: &str = ".mulch/expertise/*.jsonl merge=union\n";
    for (tag, seed, expected) in [
        ("fresh", None, UNION_LINE.to_string()),
        (
            "newline",
            Some("custom line\n"),
            "custom line\n".to_string() + UNION_LINE,
        ),
        (
            "no-newline",
            Some("no-newline-x"),
            "no-newline-x\n".to_string() + UNION_LINE,
        ),
        ("already", Some(UNION_LINE), UNION_LINE.to_string()),
    ] {
        let ours = TempDir::new(&format!("initga-{tag}"));
        if let Some(seed) = seed {
            std::fs::write(ours.0.join(".gitattributes"), seed).expect("seed attributes");
        }
        run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
        assert_eq!(
            std::fs::read_to_string(ours.0.join(".gitattributes")).expect("attributes"),
            expected,
            "{tag} first init"
        );
        // the second init never duplicates the line
        run_in(&ours.0, Path::new(mulch_bin()), &["init"]);
        assert_eq!(
            std::fs::read_to_string(ours.0.join(".gitattributes")).expect("attributes"),
            expected,
            "{tag} re-init"
        );
    }
}
