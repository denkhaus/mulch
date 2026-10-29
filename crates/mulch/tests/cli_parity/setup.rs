//! Battery for `mulch setup` (setup.ts, 0.10.7), ADR-0008 edition:
//! OUR byte pins for the git-hook cycle (hook file bytes), the
//! claude/cursor/codex integrations (settings/rule/AGENTS+toml
//! bytes), `.sh` recipes, the error surfaces, and the json envelopes
//! (all literals frozen from the parity era's twin runs; ours says
//! `mulch prime` where the reference said `ml prime`).
//!
//! Deviation-pinned (no reference twin ever): `.ts` recipes error
//! with our one-line native-build message; `--list`'s plain footer
//! and the missing npm shadowing are ours (README DEVIATIONS).

use std::path::Path;

use super::support::{TempDir, mulch_bin, run_in};

/// The managed hook section (byte pin): the section names `mulch` in
/// both binaries — frozen as OUR store's law.
const HOOK_SECTION: &str = concat!(
    "#!/bin/sh\n",
    "\n",
    "# mulch:start\n",
    "# Run mulch validate before committing\n",
    "if command -v mulch >/dev/null 2>&1; then\n",
    "  mulch validate\n",
    "  if [ $? -ne 0 ]; then\n",
    "    echo \"mulch validate failed. Commit aborted.\"\n",
    "    exit 1\n",
    "  fi\n",
    "fi\n",
    "# mulch:end\n",
);

/// The managed claude settings block appended into settings.json.
const CLAUDE_HOOKS: &str = concat!(
    "  \"hooks\": {\n",
    "    \"SessionStart\": [\n",
    "      {\n",
    "        \"matcher\": \"\",\n",
    "        \"hooks\": [\n",
    "          {\n",
    "            \"type\": \"command\",\n",
    "            \"command\": \"mulch prime\"\n",
    "          }\n",
    "        ]\n",
    "      }\n",
    "    ]\n",
    "  }",
);

/// The codex config.toml fence.
const CODEX_TOML: &str = "# mulch:start — managed by `mulch setup codex`\n[features]\ncodex_hooks \
                          = true\n\n[[hooks.SessionStart]]\n\n[[hooks.SessionStart.hooks]]\ntype \
                          = \"command\"\ncommand = \"mulch prime\"\nstatusMessage = \"Loading \
                          mulch expertise\"\n# mulch:end";

/// One scratch dir carrying `.mulch/` and a fresh `git init` (tests
/// may git-init inside temp dirs — the scratch-store law).
fn git_store(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    std::fs::create_dir_all(dir.0.join(".mulch")).expect("store dir");
    let out = std::process::Command::new("git")
        .arg("init")
        .arg("-q")
        .arg(&dir.0)
        .output()
        .expect("fixture git spawns");
    assert!(
        out.status.success(),
        "git init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    dir
}

/// A store dir without a git repository.
fn plain_store(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    std::fs::create_dir_all(dir.0.join(".mulch")).expect("store dir");
    dir
}

/// A `.mulch/recipes` fixture.
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

/// The git-hook lifecycle: install (byte-pinned hook file),
/// already-installed rerun, check, append-to-existing, remove, and
/// the only-shebang deletion — every message, channel, and byte OUR
/// pin.
#[test]
fn git_hook_cycle_pins_the_bytes() {
    let ours = git_store("s57-hooks");

    // Fresh install: ✔ on stdout, exit 0, the pinned hook bytes.
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["setup", "--hooks"]);
    assert_eq!(our.code, 0);
    assert_eq!(our.stderr, "");
    assert_eq!(our.stdout, "✔ Installed mulch pre-commit git hook.\n");
    assert_eq!(
        std::fs::read_to_string(ours.0.join(".git/hooks/pre-commit")).expect("hook"),
        HOOK_SECTION
    );

    // Idempotent rerun + check + json envelope.
    let rerun = run_in(&ours.0, Path::new(mulch_bin()), &["setup", "--hooks"]);
    assert_eq!(rerun.code, 0);
    assert_eq!(rerun.stdout, "✔ Git pre-commit hook already installed.\n");
    let check = run_in(&ours.0, Path::new(mulch_bin()), &[
        "setup", "--hooks", "--check",
    ]);
    assert_eq!(check.code, 0);
    assert_eq!(check.stdout, "✔ Git pre-commit hook is installed.\n");
    let check_json = run_in(&ours.0, Path::new(mulch_bin()), &[
        "setup", "--hooks", "--check", "--json",
    ]);
    assert_eq!(check_json.code, 0);
    assert_eq!(
        check_json.stdout,
        "{\n  \"success\": true,\n  \"command\": \"setup\",\n  \"target\": \"hooks\",\n  \
         \"action\": \"check\",\n  \"message\": \"Git pre-commit hook is installed.\"\n}\n"
    );

    // Append to existing content, then remove keeps the user bytes.
    std::fs::write(
        ours.0.join(".git/hooks/pre-commit"),
        "#!/bin/sh\n\necho hi\n\n\n",
    )
    .expect("seed hook");
    let append = run_in(&ours.0, Path::new(mulch_bin()), &["setup", "--hooks"]);
    assert_eq!(append.code, 0);
    assert_eq!(append.stdout, "✔ Installed mulch pre-commit git hook.\n");
    assert_eq!(
        std::fs::read_to_string(ours.0.join(".git/hooks/pre-commit")).expect("hook"),
        "#!/bin/sh\n\necho hi\n\n# mulch:start\n# Run mulch validate before committing\nif \
         command -v mulch >/dev/null 2>&1; then\n  mulch validate\n  if [ $? -ne 0 ]; then\n    \
         echo \"mulch validate failed. Commit aborted.\"\n    exit 1\n  fi\nfi\n# mulch:end\n"
    );
    let remove = run_in(&ours.0, Path::new(mulch_bin()), &[
        "setup", "--hooks", "--remove",
    ]);
    assert_eq!(remove.code, 0);
    assert_eq!(
        remove.stdout,
        "✔ Removed mulch section from pre-commit hook.\n"
    );
    let cleaned = std::fs::read_to_string(ours.0.join(".git/hooks/pre-commit")).expect("hook");
    assert_eq!(cleaned, "#!/bin/sh\n\necho hi\n");

    // Only-shebang-remains: the file is deleted (a bare hook first —
    // the append case ran above).
    std::fs::remove_file(ours.0.join(".git/hooks/pre-commit")).expect("reset hook");
    let _ = run_in(&ours.0, Path::new(mulch_bin()), &["setup", "--hooks"]);
    let remove = run_in(&ours.0, Path::new(mulch_bin()), &[
        "setup", "--hooks", "--remove",
    ]);
    assert_eq!(remove.code, 0);
    assert_eq!(
        remove.stdout,
        "✔ Removed mulch pre-commit hook (file deleted).\n"
    );
    assert!(!ours.0.join(".git/hooks/pre-commit").exists(), "hook gone");

    // Check on the missing hook: ✖ on STDERR, exit 1 — both modes.
    let check = run_in(&ours.0, Path::new(mulch_bin()), &[
        "setup", "--hooks", "--check",
    ]);
    assert_eq!(check.code, 1);
    assert!(check.stderr.starts_with("✖ Git pre-commit hook not found."));
    assert_eq!(check.stdout, "");
    let check_json = run_in(&ours.0, Path::new(mulch_bin()), &[
        "--json", "setup", "--hooks", "--check",
    ]);
    assert_eq!(check_json.code, 1);
    assert_eq!(
        check_json.stdout,
        "{\n  \"success\": false,\n  \"command\": \"setup\",\n  \"target\": \"hooks\",\n  \
         \"action\": \"check\",\n  \"message\": \"Git pre-commit hook not found.\"\n}\n"
    );
}

/// Hooks outside a git repository: the failure message, ✖ stderr
/// plain, and the json envelope on stdout.
#[test]
fn git_hook_without_a_repository_fails() {
    let ours = plain_store("s57-nogit");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["setup", "--hooks"]);
    assert_eq!(
        our.stderr,
        "✖ Not a git repository — .git directory not found.\n"
    );
    assert_eq!(our.code, 1);

    let our = run_in(&ours.0, Path::new(mulch_bin()), &[
        "setup", "--hooks", "--json",
    ]);
    assert!(our.stdout.contains("\"target\": \"hooks\""));
    assert!(our.stdout.contains("Not a git repository"));
    assert_eq!(our.stderr, "");
}

/// Claude settings: install into settings carrying unknown keys,
/// idempotency, check, and the remove cleanup — OUR byte pins (the
/// hook names THIS binary).
#[test]
fn claude_settings_lifecycle() {
    let ours = git_store("s57-claude");
    std::fs::create_dir_all(ours.0.join(".claude")).expect("claude dir");
    std::fs::write(
        ours.0.join(".claude/settings.json"),
        "{\n  \"model\": \"sonnet\",\n  \"verdict\": true\n}\n",
    )
    .expect("seed settings");

    let install = run_in(&ours.0, Path::new(mulch_bin()), &["setup", "claude"]);
    assert_eq!(install.code, 0);
    assert_eq!(install.stdout, "✔ Installed Claude SessionStart hook.\n");
    assert_eq!(
        std::fs::read_to_string(ours.0.join(".claude/settings.json")).expect("settings"),
        format!("{{\n  \"model\": \"sonnet\",\n  \"verdict\": true,\n{CLAUDE_HOOKS}\n}}\n")
    );
    let settings = std::fs::read_to_string(ours.0.join(".claude/settings.json")).expect("settings");
    assert!(settings.contains("\"command\": \"mulch prime\""));
    assert!(
        settings.contains("\"model\": \"sonnet\""),
        "unknown keys survive"
    );

    // Idempotent install + check + json envelope.
    let rerun = run_in(&ours.0, Path::new(mulch_bin()), &["setup", "claude"]);
    assert_eq!(rerun.code, 0);
    assert_eq!(rerun.stdout, "✔ Claude hooks already installed.\n");
    let check = run_in(&ours.0, Path::new(mulch_bin()), &[
        "setup", "claude", "--check",
    ]);
    assert_eq!(check.code, 0);
    assert_eq!(check.stdout, "✔ Claude hooks are installed and correct.\n");
    let check_json = run_in(&ours.0, Path::new(mulch_bin()), &[
        "setup", "claude", "--check", "--json",
    ]);
    assert_eq!(check_json.code, 0);
    assert!(check_json.stdout.contains("\"provider\": \"claude\""));
    assert!(check_json.stdout.contains("\"action\": \"check\""));

    // Remove: hooks gone, unknown keys intact.
    let remove = run_in(&ours.0, Path::new(mulch_bin()), &[
        "setup", "claude", "--remove",
    ]);
    assert_eq!(remove.code, 0);
    assert_eq!(
        remove.stdout,
        "✔ Removed mulch hooks from Claude settings.\n"
    );
    let removed = std::fs::read_to_string(ours.0.join(".claude/settings.json")).expect("settings");
    assert!(removed.contains("\"model\": \"sonnet\""));
    assert!(!removed.contains("SessionStart"));
    // Remove again on a hooks-less document.
    let again = run_in(&ours.0, Path::new(mulch_bin()), &[
        "setup", "claude", "--remove",
    ]);
    assert_eq!(again.code, 0);

    // Check after removal: the hooks key is gone entirely, so the
    // failure is the no-hooks wording — ✖ on STDOUT, exit 1.
    let check = run_in(&ours.0, Path::new(mulch_bin()), &[
        "setup", "claude", "--check",
    ]);
    assert_eq!(check.code, 1);
    assert!(
        check
            .stdout
            .starts_with("✖ No hooks configured in Claude settings.")
    );
    assert_eq!(check.stderr, "");

    // With settings gone altogether: the not-found wording.
    std::fs::remove_file(ours.0.join(".claude/settings.json")).expect("drop settings");
    let check = run_in(&ours.0, Path::new(mulch_bin()), &[
        "setup", "claude", "--check",
    ]);
    assert_eq!(check.code, 1);
    assert!(
        check
            .stdout
            .starts_with("✖ Claude settings.json not found.")
    );
}

/// Cursor rule: install (byte pin), check, the modified-detected
/// failure on stdout, remove, and the missing-file check.
#[test]
fn cursor_rule_lifecycle() {
    let ours = git_store("s57-cursor");

    let install = run_in(&ours.0, Path::new(mulch_bin()), &["setup", "cursor"]);
    assert_eq!(install.code, 0);
    assert_eq!(
        install.stdout,
        "✔ Installed Cursor rule at .cursor/rules/mulch.mdc.\n"
    );
    let rule = std::fs::read_to_string(ours.0.join(".cursor/rules/mulch.mdc")).expect("rule");
    assert!(rule.contains("```\nmulch prime\n```"));
    assert!(!rule.contains("ml "), "the promote law reaches the rule");
    assert!(rule.starts_with("---\ndescription: Mulch expertise integration"));

    let rerun = run_in(&ours.0, Path::new(mulch_bin()), &["setup", "cursor"]);
    assert_eq!(rerun.code, 0);
    assert_eq!(rerun.stdout, "✔ Cursor rule already installed.\n");
    let check = run_in(&ours.0, Path::new(mulch_bin()), &[
        "setup", "cursor", "--check",
    ]);
    assert_eq!(check.code, 0);
    assert_eq!(check.stdout, "✔ Cursor rule is installed and correct.\n");
    let json = run_in(&ours.0, Path::new(mulch_bin()), &[
        "setup", "cursor", "--json",
    ]);
    assert_eq!(json.code, 0);
    assert!(json.stdout.contains("\"provider\": \"cursor\""));

    // Tamper + check: ✖ on stdout, exit 1 (the yellow arm).
    std::fs::write(ours.0.join(".cursor/rules/mulch.mdc"), "tampered\n").expect("tamper");
    let check = run_in(&ours.0, Path::new(mulch_bin()), &[
        "setup", "cursor", "--check",
    ]);
    assert_eq!(check.code, 1);
    assert_eq!(
        check.stdout,
        "✖ Cursor rule file exists but has been modified.\n"
    );
    assert_eq!(check.stderr, "");

    // Remove + the gone-check.
    let remove = run_in(&ours.0, Path::new(mulch_bin()), &[
        "setup", "cursor", "--remove",
    ]);
    assert_eq!(remove.code, 0);
    assert_eq!(remove.stdout, "✔ Removed Cursor rule file.\n");
    assert!(!ours.0.join(".cursor/rules/mulch.mdc").exists());
    let check = run_in(&ours.0, Path::new(mulch_bin()), &[
        "setup", "cursor", "--check",
    ]);
    assert_eq!(check.code, 1);
    assert_eq!(check.stdout, "✖ Cursor rule file not found.\n");
}

/// Codex: AGENTS.md + config.toml install (byte pins — the toml
/// fence says `mulch setup codex`), idempotency, check, remove
/// (AGENTS.md single newline, EMPTY toml file), and remove into
/// existing content.
#[test]
fn codex_lifecycle() {
    let ours = git_store("s57-codex");

    let install = run_in(&ours.0, Path::new(mulch_bin()), &["setup", "codex"]);
    assert_eq!(install.code, 0);
    assert_eq!(
        install.stdout,
        "✔ Installed Codex integration: AGENTS.md mulch section, .codex/config.toml SessionStart \
         hook.\n"
    );
    let toml = std::fs::read_to_string(ours.0.join(".codex/config.toml")).expect("toml");
    assert!(toml.starts_with("# mulch:start — managed by `mulch setup codex`"));
    assert!(toml.contains("command = \"mulch prime\""));
    let agents = std::fs::read_to_string(ours.0.join("AGENTS.md")).expect("agents");
    assert!(agents.contains("run `mulch prime` to load project expertise"));
    assert!(!agents.contains("ml "), "no reference spellings leak");

    let rerun = run_in(&ours.0, Path::new(mulch_bin()), &["setup", "codex"]);
    assert_eq!(rerun.code, 0);
    let check = run_in(&ours.0, Path::new(mulch_bin()), &[
        "setup", "codex", "--check",
    ]);
    assert_eq!(check.code, 0);
    assert_eq!(
        check.stdout,
        "✔ Codex integration installed (AGENTS.md + .codex/config.toml).\n"
    );
    let check_json = run_in(&ours.0, Path::new(mulch_bin()), &[
        "setup", "codex", "--check", "--json",
    ]);
    assert_eq!(check_json.code, 0);
    assert!(check_json.stdout.contains("\"provider\": \"codex\""));

    // Remove: the toml keeps an EMPTY file, AGENTS.md a newline.
    let remove = run_in(&ours.0, Path::new(mulch_bin()), &[
        "setup", "codex", "--remove",
    ]);
    assert_eq!(remove.code, 0);
    assert_eq!(
        remove.stdout,
        "✔ Removed Codex integration: AGENTS.md mulch section, .codex/config.toml SessionStart \
         hook.\n"
    );
    assert_eq!(
        std::fs::read(ours.0.join(".codex/config.toml")).expect("toml"),
        b""
    );
    assert_eq!(
        std::fs::read_to_string(ours.0.join("AGENTS.md")).expect("agents"),
        "\n"
    );

    // Install into existing content: append law on both files.
    std::fs::write(ours.0.join("AGENTS.md"), "Notes.\n").expect("agents");
    std::fs::create_dir_all(ours.0.join(".codex")).expect("codex dir");
    std::fs::write(ours.0.join(".codex/config.toml"), "[user]\nkey = 1\n").expect("toml");
    let append = run_in(&ours.0, Path::new(mulch_bin()), &["setup", "codex"]);
    assert_eq!(append.code, 0);
    assert_eq!(
        std::fs::read_to_string(ours.0.join(".codex/config.toml")).expect("toml"),
        format!("[user]\nkey = 1\n\n{CODEX_TOML}\n")
    );
    let remove = run_in(&ours.0, Path::new(mulch_bin()), &[
        "setup", "codex", "--remove",
    ]);
    assert_eq!(remove.code, 0);
    assert_eq!(
        std::fs::read_to_string(ours.0.join(".codex/config.toml")).expect("toml"),
        "[user]\nkey = 1\n"
    );

    // Nothing left: the no-op remove.
    let noop = run_in(&ours.0, Path::new(mulch_bin()), &[
        "setup", "codex", "--remove",
    ]);
    assert_eq!(noop.code, 0);
}

/// A `.sh` recipe: resolves from `.mulch/recipes/`, runs with the
/// action argv and the MULCH_RECIPE_* env, stdout is the message —
/// with the json envelope's absolute path (dir-folded).
#[test]
fn shell_recipe_resolution_and_run() {
    let ours = git_store("s57-shrec");
    seed_recipe(
        &ours.0,
        "helper.sh",
        "#!/bin/sh\necho \"custom $MULCH_RECIPE_ACTION for $MULCH_RECIPE_NAME\"\n",
        true,
    );

    let run = run_in(&ours.0, Path::new(mulch_bin()), &["setup", "helper"]);
    assert_eq!(run.code, 0);
    assert!(run.stdout.contains("custom install for helper"));
    let check = run_in(&ours.0, Path::new(mulch_bin()), &[
        "setup", "helper", "--check",
    ]);
    assert_eq!(check.code, 0);
    assert!(check.stdout.contains("custom check for helper"));
    let json = run_in(&ours.0, Path::new(mulch_bin()), &[
        "setup", "helper", "--json",
    ]);
    assert_eq!(json.code, 0);
    assert!(json.stdout.contains("\"provider\": \"helper\""));
    assert!(json.stdout.contains("\"source\": \"filesystem-sh\""));
    assert!(
        normalize_json_dir(&json.stdout, &ours)
            .contains("\"path\": \"<DIR>/.mulch/recipes/helper.sh\""),
        "{}",
        json.stdout
    );

    // The silent-success and failing-script fallbacks.
    seed_recipe(&ours.0, "quiet.sh", "#!/bin/sh\nexit 0\n", true);
    seed_recipe(
        &ours.0,
        "fails.sh",
        "#!/bin/sh\necho boom >&2\nexit 3\n",
        true,
    );
    let quiet = run_in(&ours.0, Path::new(mulch_bin()), &["setup", "quiet"]);
    assert_eq!(quiet.code, 0);
    assert_eq!(quiet.stdout, "✔ quiet install succeeded\n");
    let fails = run_in(&ours.0, Path::new(mulch_bin()), &["setup", "fails"]);
    assert_eq!(fails.code, 1);
    assert_eq!(
        fails.stderr,
        "Error: fails install exited with code 3: boom\n"
    );
}

/// Folds the temp root out of a json envelope (absolute script paths).
fn normalize_json_dir(text: &str, dir: &TempDir) -> String {
    text.replace(&dir.0.display().to_string(), "<DIR>")
}

/// The `.ts` recipe deviation pin: resolving one errors with OUR
/// one-line native-build message; the error renders `Error:` stderr
/// plain and the envelope on stderr in json.
#[test]
fn typescript_recipe_errors_with_the_native_limitation() {
    let ours = git_store("s57-tsrec");
    seed_recipe(&ours.0, "internal.ts", "export const x = 1;\n", false);

    let our = run_in(&ours.0, Path::new(mulch_bin()), &["setup", "internal"]);
    assert_eq!(our.code, 1);
    assert!(normalize_json_dir(&our.stderr, &ours).starts_with("Error: Recipe \"internal\" at "));
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
}

/// The npm recipe deviation pin: a local `mulch-recipe-<name>`
/// package errors with our native-build message; without
/// node_modules the unknown-provider hint (promoted to our binary).
#[test]
fn npm_recipe_errors_with_the_native_limitation() {
    let ours = git_store("s57-npmrec");
    std::fs::create_dir_all(ours.0.join("node_modules/mulch-recipe-internal")).expect("npm dir");

    let our = run_in(&ours.0, Path::new(mulch_bin()), &["setup", "internal"]);
    assert_eq!(our.code, 1);
    assert!(
        normalize_json_dir(&our.stderr, &ours)
            .starts_with("Error: npm recipe package \"mulch-recipe-internal\" found at "),
        "{}",
        our.stderr
    );
    assert!(our.stderr.contains("cannot load npm recipes"));

    // Without node_modules: the unknown-provider hint is ours.
    let bare = plain_store("s57-unknown");
    let json = run_in(&bare.0, Path::new(mulch_bin()), &["setup", "zzz", "--json"]);
    assert_eq!(json.code, 1);
    assert!(
        json.stderr.contains("Run `mulch setup --list`"),
        "the hint names this binary: {}",
        json.stderr
    );
    let plain = run_in(&bare.0, Path::new(mulch_bin()), &["setup", "zzz"]);
    assert_eq!(plain.code, 1);
    assert!(
        plain
            .stderr
            .starts_with("Error: Unknown provider \"zzz\". Run `mulch setup --list`")
    );
}

/// `--list`: the builtins table, plain and json; with recipes present
/// OUR plain footer deviates (no npm arm — pinned), the json rows
/// still list the recipe.
#[test]
fn list_output_pins_the_table_and_our_footer() {
    let ours = git_store("s57-list");

    // Builtins only: the plain table and the json envelope.
    let plain = run_in(&ours.0, Path::new(mulch_bin()), &["setup", "--list"]);
    assert_eq!(plain.code, 0);
    assert_eq!(
        plain.stdout,
        "Available providers:\n  ✓ claude  built-in\n  ✓ codex   built-in\n  ✓ cursor  \
         built-in\n\nResolution order: filesystem (.mulch/recipes/<name>.sh) → built-in. \
         TypeScript and npm recipes are not supported by this native build.\n"
    );
    let json = run_in(&ours.0, Path::new(mulch_bin()), &[
        "setup", "--list", "--json",
    ]);
    assert_eq!(json.code, 0);
    assert_eq!(
        json.stdout,
        "{\n  \"success\": true,\n  \"command\": \"setup\",\n  \"action\": \"list\",\n  \
         \"providers\": [\n    {\n      \"name\": \"claude\",\n      \"source\": \"builtin\"\n    \
         },\n    {\n      \"name\": \"codex\",\n      \"source\": \"builtin\"\n    },\n    {\n      \
         \"name\": \"cursor\",\n      \"source\": \"builtin\"\n    }\n  ]\n}\n"
    );

    // With a .sh recipe: the json rows list it (relative path,
    // source), the plain footer deviates (ours drops the npm arm and
    // names the limitation) — pinned.
    seed_recipe(&ours.0, "helper.sh", "#!/bin/sh\n", true);
    let json = run_in(&ours.0, Path::new(mulch_bin()), &[
        "setup", "--list", "--json",
    ]);
    assert_eq!(json.code, 0);
    assert!(
        json.stdout
            .contains("\"path\": \".mulch/recipes/helper.sh\""),
        "{}",
        json.stdout
    );
    let plain = run_in(&ours.0, Path::new(mulch_bin()), &["setup", "--list"]);
    assert_eq!(plain.code, 0);
    assert!(plain.stdout.contains("Available providers:"));
    assert!(plain.stdout.contains("  ✓ claude  built-in"));
    assert!(
        plain
            .stdout
            .contains("  ✓ helper  filesystem-sh: .mulch/recipes/helper.sh")
    );
    assert!(
        plain
            .stdout
            .contains("Resolution order: filesystem (.mulch/recipes/<name>.sh) → built-in."),
        "our native-honest footer: {}",
        plain.stdout
    );
    assert!(
        !plain.stdout.contains("npm (mulch-recipe-"),
        "no npm arm in the footer"
    );
}

/// The command-level error surfaces: the missing store (both modes),
/// the no-provider divergent pair, and the sticky exit when a failing
/// --hooks pass precedes a succeeding provider.
#[test]
fn error_surfaces_and_sticky_exit() {
    // No .mulch/: both modes, plain and json (stderr channel).
    let ours = TempDir::new("s57-nostore");
    for args in [
        vec!["setup", "claude"],
        vec!["setup", "claude", "--json"],
        vec!["setup", "--hooks", "--json"],
    ] {
        let our = run_in(&ours.0, Path::new(mulch_bin()), &args);
        assert_eq!(our.code, 1, "{args:?}");
        assert!(
            our.stderr
                .contains("No .mulch/ directory found. Run `mulch init` first."),
            "{args:?}: {}",
            our.stderr
        );
    }

    // No provider and no --hooks: the divergent pair (plain
    // lowercase, json capital) — pinned.
    let ours = git_store("s57-noprovider");
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["setup"]);
    assert_eq!(
        our.stderr,
        "Error: specify a provider, use --hooks, or use --list.\n"
    );
    assert_eq!(our.code, 1);
    let our = run_in(&ours.0, Path::new(mulch_bin()), &["setup", "--json"]);
    assert!(
        our.stderr
            .contains("Specify a provider, use --hooks, or use --list.")
    );

    // --hooks + provider with a FAILING hooks pass: hooks install
    // fails (no .git), the provider pass still runs and succeeds —
    // the exit stays 1 (process.exitCode stickiness, setup.ts 703).
    let sticky = plain_store("s57-sticky");
    let our = run_in(&sticky.0, Path::new(mulch_bin()), &[
        "setup", "claude", "--hooks",
    ]);
    assert_eq!(our.code, 1, "the failed hooks pass keeps the exit at 1");
    assert!(our.stdout.contains("✔ Installed Claude SessionStart hook."));
    assert!(our.stderr.contains("✖ Not a git repository"));
}
