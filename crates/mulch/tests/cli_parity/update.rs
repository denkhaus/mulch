//! `mulch update` — the hidden deprecated stub (mulch-0e39 umbrella
//! sweep): our byte pins for both channels, the exit code, and the
//! hidden-from-help law (ADR-0008: the reference side is released).

use std::path::Path;

use super::support::{TempDir, mulch_bin, run_in};

/// Plain mode: the `!`-prefixed warning line on STDOUT, empty
/// stderr, exit 1 (probe-pinned: printWarning writes stdout, no ANSI
/// when piped).
#[test]
fn plain_warning_on_stdout_exit_one() {
    let tmp = TempDir::new("update-plain");
    let our = run_in(&tmp.0, Path::new(mulch_bin()), &["update"]);
    assert_eq!(our.code, 1, "deprecated stub exits 1");
    assert_eq!(
        our.stdout,
        "! 'mulch update' is deprecated. Use 'mulch upgrade' instead.\n"
    );
    assert!(our.stderr.is_empty(), "plain warning rides stdout");
}

/// Json mode: the error envelope on STDERR, exit 1, and `--check`
/// changes nothing (accepted and ignored).
#[test]
fn json_envelope_on_stderr_check_ignored() {
    let tmp = TempDir::new("update-json");
    for args in [vec!["--json", "update"], vec![
        "--json", "update", "--check",
    ]] {
        let our = run_in(&tmp.0, Path::new(mulch_bin()), &args);
        assert_eq!(our.code, 1);
        assert!(our.stdout.is_empty(), "json prints no plain line");
        assert!(our.stderr.contains("\"command\": \"update\""));
        assert!(our.stderr.contains("is deprecated"));
    }
}

/// Hidden law: `update` never appears in --help nor in the
/// completions word list (the honest-help surface stays as it was —
/// the stub is reachable only by typing it).
#[test]
fn update_stays_hidden_from_help_and_completions() {
    let tmp = TempDir::new("update-hidden");
    let help = run_in(&tmp.0, Path::new(mulch_bin()), &["--help"]);
    assert!(!help.stdout.contains("\n  update"), "no update command row");
    for shell in ["bash", "zsh", "fish"] {
        let script = run_in(&tmp.0, Path::new(mulch_bin()), &["completions", shell]);
        assert!(
            !script.stdout.contains("'update'") && !script.stdout.contains(" update "),
            "{shell}: hidden stub stays out of the word list"
        );
    }
}

/// Store-ignoring law: with a store present the bytes are unchanged
/// (the stub never reads .mulch/) — frozen from the parity era's
/// twin run as our own pin.
#[test]
fn update_ignores_the_store_completely() {
    let tmp = TempDir::new("update-store");
    let store = tmp.0.join(".mulch");
    std::fs::create_dir_all(store.join("expertise")).expect("store dirs");
    std::fs::write(
        store.join("mulch.config.yaml"),
        "version: '1'\ndomains:\n  rust: {}\n",
    )
    .expect("config");
    let our = run_in(&tmp.0, Path::new(mulch_bin()), &["update"]);
    assert_eq!(our.code, 1, "store presence changes nothing");
    assert_eq!(
        our.stdout,
        "! 'mulch update' is deprecated. Use 'mulch upgrade' instead.\n"
    );
    assert!(our.stderr.is_empty(), "plain warning rides stdout");
}
