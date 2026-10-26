//! Differential battery for `mulch setup` (setup.ts, 0.10.7): twin
//! git stores, ours vs the reference `ml` — the git-hook cycle with
//! byte-identical hook bytes, the claude/cursor/codex integrations
//! (content compared through the promote law: ours says `mulch
//! prime` where the reference says `ml prime`; the codex toml fence
//! says `mulch setup codex`), `.sh` recipes, the error surfaces,
//! and the json envelopes. Probed 2026-10-26
//! (/tmp/sprint55/setup/probes).
//!
//! Deviation-pinned (no reference twin): `.ts` recipes error with
//! our one-line native-build message; `--list`'s plain footer and
//! the missing npm shadowing are ours (README DEVIATIONS).

use std::path::Path;

use super::support::{TempDir, mulch_bin, promote_theirs, reference_ml, run_in};

/// One twin pair: both dirs carry `.mulch/` and a fresh `git init`
/// (tests may git-init inside temp dirs — the scratch-store law).
fn twin_git_store(tag: &str) -> (TempDir, TempDir) {
    let ours = TempDir::new(format!("{tag}-o").as_str());
    let theirs = TempDir::new(format!("{tag}-t").as_str());
    for dir in [&ours.0, &theirs.0] {
        std::fs::create_dir_all(dir.join(".mulch")).expect("store dir");
        let out = std::process::Command::new("git")
            .arg("init")
            .arg("-q")
            .arg(dir)
            .output()
            .expect("fixture git spawns");
        assert!(
            out.status.success(),
            "git init failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    (ours, theirs)
}

/// Runs the same argv in both twins and returns (ours, theirs).
fn twin_run(
    ml: &Path,
    ours: &TempDir,
    theirs: &TempDir,
    args: &[&str],
) -> (super::support::Run, super::support::Run) {
    (
        run_in(&ours.0, Path::new(mulch_bin()), args),
        run_in(&theirs.0, ml, args),
    )
}

/// Channel-split parity: same exit code, same stdout, same stderr —
/// theirs promoted (`ml <cmd>` → `mulch <cmd>`).
fn assert_parity(ours: &super::support::Run, theirs: &super::support::Run, args: &[&str]) {
    assert_eq!(ours.code, theirs.code, "{args:?}: exit codes");
    assert_eq!(
        ours.stdout,
        promote_theirs(&theirs.stdout),
        "{args:?}: stdout"
    );
    assert_eq!(
        ours.stderr,
        promote_theirs(&theirs.stderr),
        "{args:?}: stderr"
    );
}

/// Compares a file's bytes across the twins (theirs promoted);
/// missing on BOTH sides is equal by absence.
fn assert_same_file_promoted(ours: &TempDir, theirs: &TempDir, relative: &str) {
    let our = std::fs::read_to_string(ours.0.join(relative)).ok();
    let their = std::fs::read_to_string(theirs.0.join(relative)).ok();
    match (&our, &their) {
        (None, None) => {}
        _ => assert_eq!(
            our.unwrap_or_default(),
            promote_theirs(&their.unwrap_or_default()),
            "{relative} bytes (theirs promoted)"
        ),
    }
}

/// A `.mulch/recipes` fixture written identically into both twins.
fn seed_recipe(dir: &Path, name: &str, body: &str, executable: bool) {
    use std::os::unix::fs::PermissionsExt as _;
    let dir_recipes = dir.join(".mulch/recipes");
    std::fs::create_dir_all(&dir_recipes).expect("recipes dir");
    let script = dir_recipes.join(name);
    std::fs::write(&script, body).expect("recipe script");
    if executable {
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
            .expect("chmod recipe");
    }
}

/// The git-hook lifecycle: install (byte-identical hook file),
/// already-installed rerun, check, append-to-existing, remove, and
/// the only-shebang deletion — every message, channel, and byte
/// twin-pinned (the hook section names `mulch` in BOTH binaries).
#[test]
fn git_hook_cycle_matches_the_reference_byte_for_byte() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_git_store("s55-hooks");

    // Fresh install: ✔ on stdout, exit 0, byte-identical hook.
    let (our, their) = twin_run(&ml, &ours, &theirs, &["setup", "--hooks"]);
    assert_parity(&our, &their, &["setup", "--hooks"]);
    assert!(
        our.stdout
            .starts_with("✔ Installed mulch pre-commit git hook.")
    );
    assert_same_file_promoted(&ours, &theirs, ".git/hooks/pre-commit");
    // The bytes are NOT promote-dependent: the section already says
    // mulch in the reference — pin the identity directly.
    let hook = std::fs::read_to_string(ours.0.join(".git/hooks/pre-commit")).expect("hook");
    let reference_hook =
        std::fs::read_to_string(theirs.0.join(".git/hooks/pre-commit")).expect("hook");
    assert_eq!(hook, reference_hook, "MULCH_HOOK_SECTION is byte-identical");

    // Idempotent rerun + check + json envelope.
    for args in [
        vec!["setup", "--hooks"],
        vec!["setup", "--hooks", "--check"],
        vec!["setup", "--hooks", "--check", "--json"],
    ] {
        let (our, their) = twin_run(&ml, &ours, &theirs, &args);
        assert_parity(&our, &their, &args);
    }

    // Append to existing content, then remove keeps the user bytes.
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(
            dir.join(".git/hooks/pre-commit"),
            "#!/bin/sh\n\necho hi\n\n\n",
        )
        .expect("seed hook");
    }
    let (our, their) = twin_run(&ml, &ours, &theirs, &["setup", "--hooks"]);
    assert_parity(&our, &their, &["setup", "--hooks (append)"]);
    assert_same_file_promoted(&ours, &theirs, ".git/hooks/pre-commit");
    let (our, their) = twin_run(&ml, &ours, &theirs, &["setup", "--hooks", "--remove"]);
    assert_parity(&our, &their, &["setup", "--hooks", "--remove"]);
    assert_same_file_promoted(&ours, &theirs, ".git/hooks/pre-commit");
    let cleaned = std::fs::read_to_string(ours.0.join(".git/hooks/pre-commit")).expect("hook");
    assert_eq!(cleaned, "#!/bin/sh\n\necho hi\n");

    // Only-shebang-remains: the file is deleted on both sides (a
    // bare hook first — the append case ran above).
    for dir in [&ours.0, &theirs.0] {
        std::fs::remove_file(dir.join(".git/hooks/pre-commit")).expect("reset hook");
    }
    let (our, their) = twin_run(&ml, &ours, &theirs, &["setup", "--hooks"]);
    assert_parity(&our, &their, &["setup", "--hooks (fresh)"]);
    let (our, their) = twin_run(&ml, &ours, &theirs, &["setup", "--hooks", "--remove"]);
    assert_parity(&our, &their, &["setup", "--hooks", "--remove (delete)"]);
    assert!(
        our.stdout.contains("file deleted"),
        "the deletion message: {}",
        our.stdout
    );
    for dir in [&ours.0, &theirs.0] {
        assert!(!dir.join(".git/hooks/pre-commit").exists(), "hook gone");
    }

    // Check on the missing hook: ✖ on STDERR, exit 1 — both modes.
    let (our, their) = twin_run(&ml, &ours, &theirs, &["setup", "--hooks", "--check"]);
    assert_parity(&our, &their, &["setup", "--hooks", "--check (gone)"]);
    assert!(our.stderr.starts_with("✖ Git pre-commit hook not found."));
    assert_eq!(our.stdout, "");
}

/// Hooks outside a git repository: the failure message, ✖ stderr
/// plain, and the json envelope on stdout.
#[test]
fn git_hook_without_a_repository_fails_identically() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let ours = TempDir::new("s55-nogit-o");
    let theirs = TempDir::new("s55-nogit-t");
    for dir in [&ours.0, &theirs.0] {
        std::fs::create_dir_all(dir.join(".mulch")).expect("store dir");
    }
    let (our, their) = twin_run(&ml, &ours, &theirs, &["setup", "--hooks"]);
    assert_parity(&our, &their, &["setup", "--hooks (no git)"]);
    assert_eq!(
        our.stderr,
        "✖ Not a git repository — .git directory not found.\n"
    );
    assert_eq!(our.code, 1);

    let (our, their) = twin_run(&ml, &ours, &theirs, &["setup", "--hooks", "--json"]);
    assert_parity(&our, &their, &["setup", "--hooks --json (no git)"]);
    assert!(our.stdout.contains("\"target\": \"hooks\""));
    assert!(our.stdout.contains("Not a git repository"));
    assert_eq!(our.stderr, "");
}

/// Claude settings: install into settings carrying unknown keys,
/// idempotency, check, and the remove cleanup — bytes compared
/// through the promote law (ours writes `mulch prime`).
#[test]
fn claude_settings_lifecycle_matches_with_promoted_command() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_git_store("s55-claude");
    for dir in [&ours.0, &theirs.0] {
        std::fs::create_dir_all(dir.join(".claude")).expect("claude dir");
        std::fs::write(
            dir.join(".claude/settings.json"),
            "{\n  \"model\": \"sonnet\",\n  \"verdict\": true\n}\n",
        )
        .expect("seed settings");
    }

    let (our, their) = twin_run(&ml, &ours, &theirs, &["setup", "claude"]);
    assert_parity(&our, &their, &["setup claude"]);
    assert_same_file_promoted(&ours, &theirs, ".claude/settings.json");
    // Our hook names THIS binary (the promote deviation, pinned).
    let settings = std::fs::read_to_string(ours.0.join(".claude/settings.json")).expect("settings");
    assert!(settings.contains("\"command\": \"mulch prime\""));
    assert!(
        settings.contains("\"model\": \"sonnet\""),
        "unknown keys survive"
    );

    // Idempotent install + check + json envelope.
    for args in [
        vec!["setup", "claude"],
        vec!["setup", "claude", "--check"],
        vec!["setup", "claude", "--check", "--json"],
    ] {
        let (our, their) = twin_run(&ml, &ours, &theirs, &args);
        assert_parity(&our, &their, &args);
    }

    // Remove: hooks gone, unknown keys intact, `{}` never reached
    // here because model/verdict stay.
    let (our, their) = twin_run(&ml, &ours, &theirs, &["setup", "claude", "--remove"]);
    assert_parity(&our, &their, &["setup claude --remove"]);
    assert_same_file_promoted(&ours, &theirs, ".claude/settings.json");
    let removed = std::fs::read_to_string(ours.0.join(".claude/settings.json")).expect("settings");
    assert!(removed.contains("\"model\": \"sonnet\""));
    assert!(!removed.contains("SessionStart"));

    // Remove again on a hooks-less document.
    let (our, their) = twin_run(&ml, &ours, &theirs, &["setup", "claude", "--remove"]);
    assert_parity(&our, &their, &["setup claude --remove (again)"]);

    // Check after removal: the hooks key is gone entirely, so the
    // failure is the no-hooks wording — ✖ on STDOUT (the yellow
    // --check arm), exit 1.
    let (our, their) = twin_run(&ml, &ours, &theirs, &["setup", "claude", "--check"]);
    assert_parity(&our, &their, &["setup claude --check (no hooks)"]);
    assert!(
        our.stdout
            .starts_with("✖ No hooks configured in Claude settings.")
    );
    assert_eq!(our.stderr, "");
    assert_eq!(our.code, 1);

    // With settings gone altogether: the not-found wording.
    for dir in [&ours.0, &theirs.0] {
        std::fs::remove_file(dir.join(".claude/settings.json")).expect("drop settings");
    }
    let (our, their) = twin_run(&ml, &ours, &theirs, &["setup", "claude", "--check"]);
    assert_parity(&our, &their, &["setup claude --check (missing)"]);
    assert!(our.stdout.starts_with("✖ Claude settings.json not found."));
}

/// Cursor rule: install (content compared through the promote law),
/// check, the modified-detected failure on stdout, remove, and the
/// missing-file check.
#[test]
fn cursor_rule_lifecycle_matches_with_promoted_prose() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_git_store("s55-cursor");

    let (our, their) = twin_run(&ml, &ours, &theirs, &["setup", "cursor"]);
    assert_parity(&our, &their, &["setup cursor"]);
    assert_same_file_promoted(&ours, &theirs, ".cursor/rules/mulch.mdc");
    let rule = std::fs::read_to_string(ours.0.join(".cursor/rules/mulch.mdc")).expect("rule");
    assert!(rule.contains("```\nmulch prime\n```"));
    assert!(!rule.contains("ml "), "the promote law reaches the rule");

    for args in [
        vec!["setup", "cursor"],
        vec!["setup", "cursor", "--check"],
        vec!["setup", "cursor", "--json"],
    ] {
        let (our, their) = twin_run(&ml, &ours, &theirs, &args);
        assert_parity(&our, &their, &args);
    }

    // Tamper + check: ✖ on stdout, exit 1 (the yellow arm).
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(dir.join(".cursor/rules/mulch.mdc"), "tampered\n").expect("tamper");
    }
    let (our, their) = twin_run(&ml, &ours, &theirs, &["setup", "cursor", "--check"]);
    assert_parity(&our, &their, &["setup cursor --check (modified)"]);
    assert_eq!(
        our.stdout,
        "✖ Cursor rule file exists but has been modified.\n"
    );
    assert_eq!(our.stderr, "");

    // Remove + the gone-check.
    let (our, their) = twin_run(&ml, &ours, &theirs, &["setup", "cursor", "--remove"]);
    assert_parity(&our, &their, &["setup cursor --remove"]);
    assert!(!ours.0.join(".cursor/rules/mulch.mdc").exists());
    let (our, their) = twin_run(&ml, &ours, &theirs, &["setup", "cursor", "--check"]);
    assert_parity(&our, &their, &["setup cursor --check (gone)"]);
}

/// Codex: AGENTS.md + config.toml install (content through the
/// promote law — the toml fence says `mulch setup codex` in ours),
/// idempotency, check, remove (AGENTS.md single newline, EMPTY toml
/// file), and remove into existing content.
#[test]
fn codex_lifecycle_matches_with_promoted_fence() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_git_store("s55-codex");

    let (our, their) = twin_run(&ml, &ours, &theirs, &["setup", "codex"]);
    assert_parity(&our, &their, &["setup codex"]);
    assert_same_file_promoted(&ours, &theirs, "AGENTS.md");
    assert_same_file_promoted(&ours, &theirs, ".codex/config.toml");
    // The promote deviation surfaces: our fence and command name
    // this binary (word-law promoted on their side for the compare).
    let toml = std::fs::read_to_string(ours.0.join(".codex/config.toml")).expect("toml");
    assert!(toml.starts_with("# mulch:start — managed by `mulch setup codex`"));
    assert!(toml.contains("command = \"mulch prime\""));
    let agents = std::fs::read_to_string(ours.0.join("AGENTS.md")).expect("agents");
    assert!(agents.contains("run `mulch prime` to load project expertise"));
    assert!(!agents.contains("ml "), "no reference spellings leak");

    for args in [
        vec!["setup", "codex"],
        vec!["setup", "codex", "--check"],
        vec!["setup", "codex", "--check", "--json"],
    ] {
        let (our, their) = twin_run(&ml, &ours, &theirs, &args);
        assert_parity(&our, &their, &args);
    }

    // Remove: the toml keeps an EMPTY file, AGENTS.md a newline.
    let (our, their) = twin_run(&ml, &ours, &theirs, &["setup", "codex", "--remove"]);
    assert_parity(&our, &their, &["setup codex --remove"]);
    assert_eq!(
        std::fs::read(ours.0.join(".codex/config.toml")).expect("toml"),
        b""
    );
    assert_eq!(
        std::fs::read_to_string(ours.0.join("AGENTS.md")).expect("agents"),
        "\n"
    );
    assert_same_file_promoted(&ours, &theirs, "AGENTS.md");
    assert_same_file_promoted(&ours, &theirs, ".codex/config.toml");

    // Install into existing content: append law on both files.
    for dir in [&ours.0, &theirs.0] {
        std::fs::write(dir.join("AGENTS.md"), "Notes.\n").expect("agents");
        std::fs::create_dir_all(dir.join(".codex")).expect("codex dir");
        std::fs::write(dir.join(".codex/config.toml"), "[user]\nkey = 1\n").expect("toml");
    }
    let (our, their) = twin_run(&ml, &ours, &theirs, &["setup", "codex"]);
    assert_parity(&our, &their, &["setup codex (append)"]);
    assert_same_file_promoted(&ours, &theirs, "AGENTS.md");
    assert_same_file_promoted(&ours, &theirs, ".codex/config.toml");
    let (our, their) = twin_run(&ml, &ours, &theirs, &["setup", "codex", "--remove"]);
    assert_parity(&our, &their, &["setup codex --remove (append)"]);
    assert_eq!(
        std::fs::read_to_string(ours.0.join(".codex/config.toml")).expect("toml"),
        "[user]\nkey = 1\n"
    );

    // Nothing left: the no-op remove.
    let (our, their) = twin_run(&ml, &ours, &theirs, &["setup", "codex", "--remove"]);
    assert_parity(&our, &their, &["setup codex --remove (noop)"]);
}

/// A `.sh` recipe: resolves from `.mulch/recipes/`, runs with the
/// action argv and the MULCH_RECIPE_* env, stdout is the message —
/// twin-pinned including the json envelope's absolute path.
#[test]
fn shell_recipe_resolution_and_run_match() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_git_store("s55-shrec");
    for dir in [&ours.0, &theirs.0] {
        seed_recipe(
            dir,
            "helper.sh",
            "#!/bin/sh\necho \"custom $MULCH_RECIPE_ACTION for $MULCH_RECIPE_NAME\"\n",
            true,
        );
    }

    for args in [
        vec!["setup", "helper"],
        vec!["setup", "helper", "--check"],
        vec!["setup", "helper", "--json"],
    ] {
        let (our, their) = twin_run(&ml, &ours, &theirs, &args);
        // The json envelope carries each side's ABSOLUTE script path
        // — normalize both temp roots to <DIR> before comparing.
        assert_eq!(our.code, their.code, "{args:?}: exit codes");
        assert_eq!(
            our.stdout.replace(&ours.0.display().to_string(), "<DIR>"),
            promote_theirs(&their.stdout).replace(&theirs.0.display().to_string(), "<DIR>"),
            "{args:?}: stdout (dirs normalized)"
        );
        assert_eq!(our.stderr, their.stderr, "{args:?}: stderr");
        if args.len() == 2 {
            assert!(our.stdout.contains("custom install for helper"));
        }
    }

    // The silent-success and failing-script fallbacks.
    for dir in [&ours.0, &theirs.0] {
        seed_recipe(dir, "quiet.sh", "#!/bin/sh\nexit 0\n", true);
        seed_recipe(dir, "fails.sh", "#!/bin/sh\necho boom >&2\nexit 3\n", true);
    }
    let (our, their) = twin_run(&ml, &ours, &theirs, &["setup", "quiet"]);
    assert_parity(&our, &their, &["setup quiet"]);
    assert_eq!(our.stdout, "✔ quiet install succeeded\n");
    let (our, their) = twin_run(&ml, &ours, &theirs, &["setup", "fails"]);
    assert_parity(&our, &their, &["setup fails"]);
    assert_eq!(our.code, 1);
    assert_eq!(
        our.stderr,
        "Error: fails install exited with code 3: boom\n"
    );
}

/// The `.ts` recipe deviation pin: resolving one errors with OUR
/// one-line native-build message (the reference imports it); the
/// error renders `Error:` stderr plain and the envelope on stderr in
/// json — channels pinned against the reference's own error class.
#[test]
fn typescript_recipe_errors_with_the_native_limitation() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_git_store("s55-tsrec");
    for dir in [&ours.0, &theirs.0] {
        seed_recipe(dir, "internal.ts", "export const x = 1;\n", false);
    }

    let our = run_in(&ours.0, Path::new(mulch_bin()), &["setup", "internal"]);
    assert_eq!(our.code, 1);
    assert!(our.stderr.starts_with("Error: Recipe \"internal\" at "));
    assert!(
        our.stderr
            .contains("is a TypeScript recipe — this native build cannot import TypeScript"),
        "{}",
        our.stderr
    );
    assert_eq!(our.stdout, "");

    let our_json = run_in(&ours.0, Path::new(mulch_bin()), &[
        "setup", "internal", "--json",
    ]);
    assert_eq!(our_json.code, 1);
    assert!(our_json.stderr.contains("\"command\": \"setup\""));
    assert!(our_json.stderr.contains("TypeScript recipe"));
    assert_eq!(our_json.stdout, "");

    // The same fixture on the reference side: it imports and fails
    // with ITS error (no default export) — the classes differ by
    // design; only the channels and exit code are shared.
    let their = run_in(&theirs.0, &ml, &["setup", "internal"]);
    assert_eq!(their.code, 1);
    assert!(their.stderr.starts_with("Error: Recipe \"internal\" at "));
    assert_eq!(their.stdout, "");
}

/// The npm recipe deviation pin: a local `mulch-recipe-<name>`
/// package errors with our native-build message (the reference
/// imports it through createRequire).
#[test]
fn npm_recipe_errors_with_the_native_limitation() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_git_store("s55-npmrec");
    for dir in [&ours.0, &theirs.0] {
        std::fs::create_dir_all(dir.join("node_modules/mulch-recipe-internal")).expect("npm dir");
    }

    let our = run_in(&ours.0, Path::new(mulch_bin()), &["setup", "internal"]);
    assert_eq!(our.code, 1);
    assert!(
        our.stderr
            .starts_with("Error: npm recipe package \"mulch-recipe-internal\" found at "),
        "{}",
        our.stderr
    );
    assert!(our.stderr.contains("cannot load npm recipes"));

    // Without node_modules the reference falls through to the
    // unknown-provider hint — ours matches that path byte-for-byte
    // (modulo the promoted `mulch setup --list`).
    let bare_ours = TempDir::new("s55-unknown-o");
    let bare_theirs = TempDir::new("s55-unknown-t");
    for dir in [&bare_ours.0, &bare_theirs.0] {
        std::fs::create_dir_all(dir.join(".mulch")).expect("store dir");
    }
    let (our, their) = twin_run(&ml, &bare_ours, &bare_theirs, &["setup", "zzz", "--json"]);
    assert_parity(&our, &their, &["setup zzz --json"]);
    assert!(
        promote_theirs(&their.stderr).contains("Run `mulch setup --list`"),
        "the hint promotes on their side for the compare: {}",
        their.stderr
    );
    let (our, their) = twin_run(&ml, &bare_ours, &bare_theirs, &["setup", "zzz"]);
    assert_parity(&our, &their, &["setup zzz"]);
}

/// `--list`: the builtins-only table matches the reference (plain
/// and json); with recipes present OUR plain footer deviates (no npm
/// arm — pinned), the json rows still match.
#[test]
fn list_output_matches_except_the_deviation_footer() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    let (ours, theirs) = twin_git_store("s55-list");

    // Builtins only: the json mode is byte parity; the PLAIN table's
    // header and rows match but the footer is our deviation (no npm
    // arm) — compare everything above the blank line, pin ours below.
    let (our, their) = twin_run(&ml, &ours, &theirs, &["setup", "--list"]);
    assert_eq!(our.code, their.code);
    let our_table = our.stdout.split("\n\n").next().unwrap_or_default();
    let their_table = their.stdout.split("\n\n").next().unwrap_or_default();
    assert_eq!(our_table, their_table, "the table header and rows match");
    let (our, their) = twin_run(&ml, &ours, &theirs, &["setup", "--list", "--json"]);
    assert_parity(&our, &their, &["setup --list --json"]);

    // With a .sh recipe: the json rows still match (relative path,
    // source, sort); the plain footer deviates (ours drops the npm
    // arm and names the limitation) — pinned, not compared.
    for dir in [&ours.0, &theirs.0] {
        seed_recipe(dir, "helper.sh", "#!/bin/sh\n", true);
    }
    let (our, their) = twin_run(&ml, &ours, &theirs, &["setup", "--list", "--json"]);
    assert_parity(&our, &their, &["setup --list --json (recipe)"]);
    assert!(
        our.stdout
            .contains("\"path\": \".mulch/recipes/helper.sh\"")
    );

    let our = run_in(&ours.0, Path::new(mulch_bin()), &["setup", "--list"]);
    assert_eq!(our.code, 0);
    assert!(our.stdout.contains("Available providers:"));
    assert!(our.stdout.contains("  ✓ claude  built-in"));
    assert!(
        our.stdout
            .contains("  ✓ helper  filesystem-sh: .mulch/recipes/helper.sh")
    );
    assert!(
        our.stdout
            .contains("Resolution order: filesystem (.mulch/recipes/<name>.sh) → built-in."),
        "our native-honest footer: {}",
        our.stdout
    );
    assert!(
        !our.stdout.contains("npm (mulch-recipe-"),
        "no npm arm in the footer"
    );
}

/// The command-level error surfaces: the missing store (both
/// modes), the no-provider divergent pair, and the sticky exit when
/// a failing --hooks pass precedes a succeeding provider.
#[test]
fn error_surfaces_and_sticky_exit_match() {
    let Some(ml) = reference_ml() else {
        eprintln!("skipped: no ml on PATH");
        return;
    };
    // No .mulch/: both modes, both plain and json (stderr channel).
    let ours = TempDir::new("s55-nostore-o");
    let theirs = TempDir::new("s55-nostore-t");
    for args in [
        vec!["setup", "claude"],
        vec!["setup", "claude", "--json"],
        vec!["setup", "--hooks", "--json"],
    ] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        let their = run_in(&theirs.0, &ml, &args);
        assert_parity(&our, &their, &args);
        assert_eq!(our.code, 1);
        assert!(
            our.stderr
                .contains("No .mulch/ directory found. Run `mulch init` first.")
        );
    }

    // No provider and no --hooks: the divergent pair (plain
    // lowercase, json capital) — pinned on both sides.
    let (ours, theirs) = twin_git_store("s55-noprovider");
    let (our, their) = twin_run(&ml, &ours, &theirs, &["setup"]);
    assert_parity(&our, &their, &["setup"]);
    assert_eq!(
        our.stderr,
        "Error: specify a provider, use --hooks, or use --list.\n"
    );
    let (our, their) = twin_run(&ml, &ours, &theirs, &["setup", "--json"]);
    assert_parity(&our, &their, &["setup --json"]);
    assert!(
        our.stderr
            .contains("Specify a provider, use --hooks, or use --list.")
    );

    // --hooks + provider with a FAILING hooks pass: hooks install
    // fails (no .git), the provider pass still runs and succeeds —
    // the exit stays 1 (process.exitCode stickiness, setup.ts 703).
    let sticky_ours = TempDir::new("s55-sticky-o");
    let sticky_theirs = TempDir::new("s55-sticky-t");
    for dir in [&sticky_ours.0, &sticky_theirs.0] {
        std::fs::create_dir_all(dir.join(".mulch")).expect("store dir");
    }
    let (our, their) = twin_run(&ml, &sticky_ours, &sticky_theirs, &[
        "setup", "claude", "--hooks",
    ]);
    assert_parity(&our, &their, &["setup claude --hooks (sticky)"]);
    assert_eq!(our.code, 1, "the failed hooks pass keeps the exit at 1");
    assert!(our.stdout.contains("✔ Installed Claude SessionStart hook."));
    assert!(our.stderr.contains("✖ Not a git repository"));
}
