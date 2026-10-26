//! `mulch update` — the hidden deprecated stub (mulch-0e39 umbrella
//! sweep): byte-parity with ml 0.10.7's update.ts on both channels,
//! the exit code, and the hidden-from-help law.

use std::path::Path;

use super::support::{TempDir, mulch_bin, reference_ml, run_in};

/// Plain mode: the `!`-prefixed warning line on STDOUT, empty
/// stderr, exit 1 — byte-identical to the reference (probe-pinned:
/// printWarning writes stdout, no ANSI when piped).
#[test]
fn plain_warning_on_stdout_exit_one() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let tmp = TempDir::new("update-plain");
    let our = run_in(&tmp.0, Path::new(mulch_bin()), &["update"]);
    let their = run_in(&tmp.0, &ml, &["update"]);
    assert_eq!(our.code, their.code);
    assert_eq!(our.code, 1, "deprecated stub exits 1");
    assert_eq!(our.stdout, their.stdout);
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
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let tmp = TempDir::new("update-json");
    for args in [vec!["--json", "update"], vec![
        "--json", "update", "--check",
    ]] {
        let our = run_in(&tmp.0, Path::new(mulch_bin()), &args);
        let their = run_in(&tmp.0, &ml, &args);
        assert_eq!(our.code, their.code);
        assert_eq!(our.code, 1);
        assert_eq!(our.stdout, their.stdout);
        assert!(our.stdout.is_empty(), "json prints no plain line");
        assert_eq!(our.stderr, their.stderr, "{args:?}");
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
/// (the stub never reads .mulch/) — the reference in the same
/// directory prints the identical surface.
#[test]
fn update_ignores_the_store_completely() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let tmp = TempDir::new("update-store");
    let store = tmp.0.join(".mulch");
    std::fs::create_dir_all(store.join("expertise")).expect("store dirs");
    std::fs::write(
        store.join("mulch.config.yaml"),
        "version: '1'\ndomains:\n  rust: {}\n",
    )
    .expect("config");
    let our = run_in(&tmp.0, Path::new(mulch_bin()), &["update"]);
    let their = run_in(&tmp.0, &ml, &["update"]);
    assert_eq!(
        (our.code, our.stdout.clone(), our.stderr.clone()),
        (their.code, their.stdout, their.stderr)
    );
}
