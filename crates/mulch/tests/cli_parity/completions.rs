//! Battery for `mulch completions` (mulch-176f): the three template
//! bodies, the case-insensitive shell match, the quiet/json
//! invariance, and the unsupported-shell error law — pinned on OUR
//! bytes (ADR-0008: the parity regime against the ml executable is
//! dissolved; our template bytes are the regression memory). The
//! templates still carry the inherited `ml` alias lines (bash/zsh
//! footers, fish's per-command `complete -c ml` rows) — frozen as-is.

use std::path::Path;

use super::support::{TempDir, mulch_bin, run_in};

/// The bash/zsh/fish template law: header, footer, word-list
/// contents, and the honest-help rule (our list names implemented
/// commands only — `update` must not appear).
#[test]
fn completions_templates_and_word_lists_are_pinned() {
    let ours = TempDir::new("comp-bash");
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
        if shell == "fish" {
            // fish has no shared footer (per-command lines) and its
            // generator ignores the global options — pin the mulch/ml
            // pair law and our own list.
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
            // the ml alias line is the footer shape (frozen)
            assert!(
                our.stdout.ends_with(footer),
                "fish: the last line is the ml audit command"
            );
            // setup landed with the setup slice (sprint 55): honest
            // help follows the enum.
            assert!(
                our.stdout.contains("'setup'"),
                "fish: ours lists the implemented setup command"
            );
            assert!(
                !our.stdout.contains("--json"),
                "fish: no global options in our script"
            );
        } else {
            assert!(our.stdout.ends_with(footer), "{shell}: our footer");
            // our list carries the globals
            for word in ["--json", "--format", "--version"] {
                assert!(our.stdout.contains(word), "{shell}: our list has {word}");
            }
        }
        // ours never names a command this port has not implemented
        // (update — config/diff are sibling slices and may
        // legitimately appear once wired; setup/audit landed with
        // their slices, sprint 55)
        assert!(
            !our.stdout.contains(" update ") && !our.stdout.contains("'update'"),
            "{shell}: update must not be in our list (honest help)"
        );
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
/// script).
#[test]
fn completions_shell_names_match_case_insensitively() {
    let ours = TempDir::new("comp-case");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["completions", "BASH"]);
    assert_eq!(our.code, 0);
    assert!(our.stdout.starts_with("# mulch bash completions"));
}

/// The unsupported-shell law: exit 1, the `✗` line with the RAW
/// argument — in plain AND json mode (no envelope; printError
/// bypasses jsonMode).
#[test]
fn completions_unsupported_shell_is_the_plain_line_in_both_modes() {
    let ours = TempDir::new("comp-uns");
    for args in [
        vec!["completions", "pwsh"],
        vec!["--json", "completions", "pwsh"],
        vec!["completions", ""],
    ] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(our.code, 1, "{args:?}: our exit");
        let expected = if args.contains(&"") {
            "✗ Unsupported shell: \"\". Supported: bash, zsh, fish\n"
        } else {
            "✗ Unsupported shell: \"pwsh\". Supported: bash, zsh, fish\n"
        };
        assert_eq!(our.stderr, expected, "{args:?}: our stderr");
        assert!(our.stdout.is_empty(), "{args:?}: no stdout");
    }
}
