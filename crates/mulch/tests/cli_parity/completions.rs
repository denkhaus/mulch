//! Differential battery for `mulch completions` (mulch-176f): the
//! three template bodies vs the reference `ml` 0.10.7, the
//! case-insensitive shell match, the quiet/json invariance, and the
//! unsupported-shell error law. The WORD LIST is the documented
//! deviation (ours names this binary's implemented commands), so the
//! assertions pin the template bytes around the list and each side's
//! own list laws. Probed 2026-10-26 (/tmp/opencode/sprint54B).

use std::path::Path;

use super::support::{TempDir, mulch_bin, promote_theirs, reference_ml, run_in};

/// The bash/zsh template law: identical header and footer bytes on
/// both binaries, our list naming implemented commands only, the
/// reference's naming its full surface (the deviation itself).
#[test]
fn completions_templates_match_and_word_lists_follow_their_binaries() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("comp-bash-o");
    let theirs = TempDir::new("comp-bash-t");
    for (shell, header, footer) in [
        (
            "bash",
            "# mulch bash completions\n# Add to ~/.bashrc: eval \"$(mulch completions bash)\"\n",
            "complete -F _mulch mulch\ncomplete -F _mulch ml\n",
        ),
        (
            "zsh",
            "# mulch zsh completions\n# Add to ~/.zshrc: eval \"$(mulch completions zsh)\"\n",
            "compdef _mulch mulch\ncompdef _mulch ml\n",
        ),
        (
            "fish",
            "# mulch fish completions\n# Save to ~/.config/fish/completions/mulch.fish\n\n",
            "complete -c ml -n '__fish_use_subcommand' -a 'audit' -d 'audit command'\n",
        ),
    ] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), &["completions", shell]);
        let their = run_in(&theirs.0, &ml, &["completions", shell]);
        assert_eq!(our.code, their.code, "{shell}: exit codes");
        assert_eq!(our.code, 0, "{shell}: success");
        assert!(
            our.stderr.is_empty(),
            "{shell}: our stderr {:?}",
            our.stderr
        );
        assert!(
            our.stdout.starts_with(header),
            "{shell}: our template header"
        );
        assert!(
            their.stdout.starts_with(header),
            "{shell}: reference template header"
        );
        if shell == "fish" {
            // fish has no shared footer (per-command lines) and its
            // generator ignores the global options (completions.ts:43
            // takes them as an unused parameter) — pin the pair law
            // and each side's own list instead.
            let our_lines: Vec<&str> = our.stdout.lines().collect();
            let last = our_lines.last().expect("our fish output");
            let second_last = our_lines[our_lines.len() - 2];
            assert!(
                last.starts_with("complete -c ml "),
                "our last is the ml line"
            );
            assert_eq!(
                second_last.replace("complete -c mulch ", "complete -c "),
                last.replace("complete -c ml ", "complete -c "),
                "the mulch/ml lines complete the same command"
            );
            assert!(
                their.stdout.ends_with(footer),
                "fish: the reference's last line is its audit command"
            );
            assert!(
                their.stdout.contains("'setup'"),
                "fish: the reference lists its full surface"
            );
            // setup landed with the setup slice (sprint 55): ours
            // lists it too now — honest help follows the enum.
            assert!(
                our.stdout.contains("'setup'"),
                "fish: ours lists the implemented setup command"
            );
            assert!(
                !our.stdout.contains("--json"),
                "fish: no global options in our script"
            );
            assert!(
                !their.stdout.contains("--json"),
                "fish: no global options in the reference's script either"
            );
        } else {
            assert!(our.stdout.ends_with(footer), "{shell}: our footer");
            assert!(their.stdout.ends_with(footer), "{shell}: reference footer");
            // the shared word-list laws: both carry their globals
            for word in ["--json", "--format", "--version"] {
                assert!(our.stdout.contains(word), "{shell}: our list has {word}");
                assert!(their.stdout.contains(word), "{shell}: reference has {word}");
            }
        }
        // ours never names a reference command this port has not
        // implemented (update — config/diff are sibling slices and
        // may legitimately appear once wired; setup/audit landed
        // with their slices, sprint 55)
        assert!(
            !our.stdout.contains(" update ") && !our.stdout.contains("'update'"),
            "{shell}: update must not be in our list (honest help)"
        );
        if shell != "fish" {
            assert!(
                their.stdout.contains(" setup "),
                "{shell}: the reference's list carries its own surface"
            );
        }
    }
}

/// Quiet and json never change a byte of the script (probed:
/// `process.stdout.write` bypasses both).
#[test]
fn completions_output_is_quiet_and_json_invariant() {
    let dir = TempDir::new("comp-inv");
    let plain = run_in(&dir.0, Path::new(mulch_bin()), &["completions", "bash"]);
    let quiet = run_in(&dir.0, Path::new(mulch_bin()), &[
        "--quiet",
        "completions",
        "bash",
    ]);
    let json = run_in(&dir.0, Path::new(mulch_bin()), &[
        "--json",
        "completions",
        "bash",
    ]);
    assert_eq!(plain.code, 0);
    assert_eq!(plain.stdout, quiet.stdout, "--quiet does not suppress");
    assert_eq!(plain.stdout, json.stdout, "--json does not envelope");
}

/// Shell names match case-insensitively (`BASH` generates the bash
/// script on both binaries).
#[test]
fn completions_shell_names_match_case_insensitively() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("comp-case-o");
    let theirs = TempDir::new("comp-case-t");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["completions", "BASH"]);
    let their = run_in(&theirs.0, &ml, &["completions", "BASH"]);
    assert_eq!(our.code, their.code, "BASH exit");
    assert!(our.stdout.starts_with("# mulch bash completions"));
    assert!(their.stdout.starts_with("# mulch bash completions"));
}

/// The unsupported-shell law: byte-identical stderr on both binaries
/// — exit 1, the `✗` line with the RAW argument — in plain AND json
/// mode (no envelope; printError bypasses jsonMode).
#[test]
fn completions_unsupported_shell_is_the_plain_line_in_both_modes() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("comp-uns-o");
    let theirs = TempDir::new("comp-uns-t");
    for args in [
        vec!["completions", "pwsh"],
        vec!["--json", "completions", "pwsh"],
        vec!["completions", ""],
    ] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        let their = run_in(&theirs.0, &ml, &args);
        assert_eq!(our.code, 1, "{args:?}: our exit");
        assert_eq!(our.code, their.code, "{args:?}: exit codes");
        let expected = if args.contains(&"") {
            "✗ Unsupported shell: \"\". Supported: bash, zsh, fish\n"
        } else {
            "✗ Unsupported shell: \"pwsh\". Supported: bash, zsh, fish\n"
        };
        assert_eq!(our.stderr, expected, "{args:?}: our stderr");
        assert_eq!(
            promote_theirs(&their.stderr),
            expected,
            "{args:?}: reference stderr"
        );
        assert!(our.stdout.is_empty(), "{args:?}: no stdout");
    }
}
