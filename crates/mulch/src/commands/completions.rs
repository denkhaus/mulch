//! `mulch completions <shell>` — the reference 0.10.7's hand-rolled
//! completion scripts (completions.ts: NOT commander's generator, so
//! byte parity of the TEMPLATE is achievable).
//!
//! Probed laws (ml 0.10.7, /tmp/opencode/sprint54B/probes):
//!
//! - Shells: bash, zsh, fish — matched case-insensitively (`BASH` works,
//!   completions.ts:57).
//! - Unsupported shells print `✗ Unsupported shell: "<raw>". Supported: bash,
//!   zsh, fish` on STDERR, exit 1 — the SAME plain line in `--json` mode (no
//!   envelope; printError bypasses jsonMode) and in `--quiet` (printError
//!   ignores quiet).
//! - The scripts print verbatim on stdout (no trailing newline added, not
//!   suppressed by --quiet, not enveloped in --json).
//! - Word lists: the program's visible commands (hidden subcommands like the
//!   reference's `update` are excluded) then the global option longs — both in
//!   declaration order.
//!
//! DEVIATION (README): the reference's word list names ITS 30 visible
//! commands; ours names THIS binary's implemented commands in enum
//! declaration order (help honesty, mulch-0e39) plus our global
//! longs — the template bytes around the list are identical.

use clap::CommandFactory as _;

use crate::output::Failure;

/// The shells the reference supports (completions.ts:4).
const SUPPORTED_SHELLS: [&str; 3] = ["bash", "zsh", "fish"];

/// Runs `completions <shell>`: prints the script for a supported
/// shell (case-insensitive), or the probed plain error for any other
/// value. `opts` is deliberately not a parameter: neither `--json`
/// nor `--quiet` changes a byte of either path (probed).
pub(super) fn run(shell: &str) -> Result<(), Failure> {
    let normalized = shell.to_ascii_lowercase();
    let commands = visible_commands();
    let globals = global_options();
    let script = match normalized.as_str() {
        "bash" => generate_bash(&commands, &globals),
        "zsh" => generate_zsh(&commands, &globals),
        "fish" => generate_fish(&commands),
        _ => return Err(unsupported(shell)),
    };
    #[allow(clippy::print_stdout, reason = "the script IS the command's output")]
    {
        print!("{script}");
    }
    Ok(())
}

/// The unsupported-shell failure: the command prints the reference's
/// plain `✗` line itself (both modes render it identically), so the
/// failure returns pre-rendered with the exit contract.
fn unsupported(shell: &str) -> Failure {
    let detail = format!(
        "Unsupported shell: \"{shell}\". Supported: {}",
        SUPPORTED_SHELLS.join(", ")
    );
    #[allow(clippy::print_stderr, reason = "error rendering is the CLI boundary")]
    {
        eprintln!("✗ {detail}");
    }
    Failure::rendered_exit("completions")
}

/// This binary's visible subcommands in declaration order — the
/// honest-help word list (the reference's `visibleCommands` twin;
/// clap keeps derive declaration order, and none of our subcommands
/// is hidden).
fn visible_commands() -> Vec<String> {
    crate::cli::Cli::command()
        .get_subcommands()
        .filter(|sub| !sub.is_hide_set())
        .map(|sub| sub.get_name().to_string())
        .collect()
}

/// The global option longs in declaration order, `--`-prefixed —
/// the reference's `program.options.map(o => o.long)` twin (its
/// `--version` comes from `.version()`; ours from the declared
/// version flag). clap's auto `--help` arg is excluded: commander
/// keeps help out of `program.options` too (probed list has no
/// `--help`).
fn global_options() -> Vec<String> {
    crate::cli::Cli::command()
        .get_arguments()
        .filter_map(|arg| arg.get_long())
        .filter(|long| *long != "help")
        .map(|long| format!("--{long}"))
        .collect()
}

/// The bash script (completions.ts:16-27, byte-pinned).
fn generate_bash(commands: &[String], global_options: &[String]) -> String {
    let mut all_words = commands.to_vec();
    all_words.extend_from_slice(global_options);
    format!(
        "# mulch bash completions
# Add to ~/.bashrc: eval \"$(mulch completions bash)\"
_mulch() {{
  local cur=\"${{COMP_WORDS[COMP_CWORD]}}\"
  COMPREPLY=( $(compgen -W \"{}\" -- \"$cur\") )
}}
complete -F _mulch mulch
complete -F _mulch ml
",
        all_words.join(" ")
    )
}

/// The zsh script (completions.ts:29-41, byte-pinned).
fn generate_zsh(commands: &[String], global_options: &[String]) -> String {
    let mut all_words = commands.to_vec();
    all_words.extend_from_slice(global_options);
    format!(
        "# mulch zsh completions
# Add to ~/.zshrc: eval \"$(mulch completions zsh)\"
_mulch() {{
  local -a words
  words=({})
  _describe 'mulch commands' words
}}
compdef _mulch mulch
compdef _mulch ml
",
        all_words.join(" ")
    )
}

/// The fish script (completions.ts:43-50, byte-pinned): one
/// `complete` line per command for BOTH binary names — the reference
/// deliberately registers `mulch` and `ml`, so the template ships
/// both regardless of which is installed.
fn generate_fish(commands: &[String]) -> String {
    let mut lines = vec![
        "# mulch fish completions".to_string(),
        "# Save to ~/.config/fish/completions/mulch.fish".to_string(),
        String::new(),
    ];
    for command in commands {
        lines.push(format!(
            "complete -c mulch -n '__fish_use_subcommand' -a '{command}' -d '{command} command'"
        ));
        lines.push(format!(
            "complete -c ml -n '__fish_use_subcommand' -a '{command}' -d '{command} command'"
        ));
    }
    format!("{}\n", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::EXIT_ERROR;

    /// The three template bodies are byte-pinned against synthetic
    /// word lists (the reference's generators with a two-command,
    /// one-option program).
    #[test]
    fn template_bytes_match_the_reference_generators() {
        let commands = vec!["init".to_string(), "add".to_string()];
        let globals = vec!["--json".to_string()];
        assert_eq!(
            generate_bash(&commands, &globals),
            "# mulch bash completions
# Add to ~/.bashrc: eval \"$(mulch completions bash)\"
_mulch() {
  local cur=\"${COMP_WORDS[COMP_CWORD]}\"
  COMPREPLY=( $(compgen -W \"init add --json\" -- \"$cur\") )
}
complete -F _mulch mulch
complete -F _mulch ml
"
        );
        assert_eq!(
            generate_zsh(&commands, &globals),
            "# mulch zsh completions
# Add to ~/.zshrc: eval \"$(mulch completions zsh)\"
_mulch() {
  local -a words
  words=(init add --json)
  _describe 'mulch commands' words
}
compdef _mulch mulch
compdef _mulch ml
"
        );
        assert_eq!(
            generate_fish(&commands),
            "# mulch fish completions
# Save to ~/.config/fish/completions/mulch.fish

complete -c mulch -n '__fish_use_subcommand' -a 'init' -d 'init command'
complete -c ml -n '__fish_use_subcommand' -a 'init' -d 'init command'
complete -c mulch -n '__fish_use_subcommand' -a 'add' -d 'add command'
complete -c ml -n '__fish_use_subcommand' -a 'add' -d 'add command'
"
        );
    }

    /// The word list names THIS binary's implemented surface — every
    /// declared subcommand appears (declaration order, `delete-domain`
    /// and `move` by their renamed spellings), never a hidden or
    /// not-yet-implemented reference command.
    #[test]
    fn word_list_is_this_binarys_declared_surface() {
        let commands = visible_commands();
        for name in [
            "init",
            "add",
            "record",
            "delete-domain",
            "move",
            "prime",
            "completions",
        ] {
            assert!(
                commands.iter().any(|command| command == name),
                "{name} missing from {commands:?}"
            );
        }
        // help honesty: commands this port has not implemented never
        // appear (deliberately excludes config/diff — sibling slices
        // may land them; only not-yet-existing commands pin here)
        for absent in ["setup", "audit", "update"] {
            assert!(
                !commands.iter().any(|command| command == absent),
                "{absent} must not be listed before it is implemented"
            );
        }
        let options = global_options();
        assert_eq!(
            options,
            vec![
                "--version",
                "--json",
                "--quiet",
                "--verbose",
                "--timing",
                "--allow-unknown-types",
                "--allow-domain-mismatch",
                "--format",
            ],
            "global longs in declaration order, help excluded (probed reference order)"
        );
    }

    /// The unsupported-shell law: the raw (un-lowercased) argument is
    /// quoted, the failure carries exit 1, and it returns pre-rendered
    /// (the command already printed the line both modes share — the
    /// struct itself carries nothing but the exit contract).
    #[test]
    fn unsupported_shell_fails_pre_rendered_with_the_plain_line() {
        let failure = unsupported("PowerShell");
        assert_eq!(failure.code, EXIT_ERROR);
        assert!(failure.rendered, "the line is printed by the command");
        assert_eq!(failure.message, "");
    }

    /// Shell matching is case-insensitive and the empty string is
    /// unsupported (probed: `ml completions ""` errors with the raw
    /// empty quote).
    #[test]
    fn empty_shell_is_unsupported() {
        let failure = unsupported("");
        assert!(failure.rendered);
        assert_eq!(failure.code, EXIT_ERROR);
    }
}
