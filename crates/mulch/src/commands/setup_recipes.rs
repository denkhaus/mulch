//! `mulch setup`'s recipe layer — parity with the reference 0.10.7
//! (setup.ts git-hook helpers + built-in provider recipes,
//! recipe-discovery.ts, native-honest subset).
//!
//! Probed laws (ml 0.10.7, /tmp/sprint55/setup/probes):
//!
//! - Git hook: `MULCH_HOOK_SECTION` is byte-identical to the reference (its
//!   bytes already name `mulch`, not `ml`); append to existing content with
//!   `trimEnd` + one blank line, or create with a `#!/bin/sh` shebang; chmod
//!   0755; removal cuts the marker-bounded section, collapses `\n{3,}` to
//!   `\n\n`, and DELETES the file when only the shebang (or nothing) remains.
//! - Claude: `.claude/settings.json` — parse preserving unknown keys and their
//!   order (serde_json preserve_order mirrors JS object insertion order),
//!   ensure a `hooks.SessionStart` ENTRY (an array of groups) carrying
//!   `{matcher:"", hooks:[{type:"command", command}]}`; OUR command is `mulch
//!   prime` (promote law; the reference writes `ml prime`); write
//!   `to_string_pretty + "\n"`.
//! - Cursor: `.cursor/rules/mulch.mdc` — the template with `mulch` spellings,
//!   footer = the embedded session-close from the config (onboard's seam),
//!   default preset fallback.
//! - Codex: AGENTS.md marker section (onboard's marker law) plus
//!   `.codex/config.toml` fenced block; the toml marker names the invocation —
//!   `mulch setup codex` (promote law) — and the hook command is `mulch prime`.
//!
//! DEVIATIONS (README): `.mulch/recipes/<name>.ts` recipes and npm
//! `mulch-recipe-<name>` packages resolve to a one-line error naming
//! the native limitation (a Rust binary cannot import TypeScript or
//! load npm packages); resolution order is filesystem `.sh` →
//! built-in, and `--list` never probes npm shadows. Throw wordings
//! (io, JSON parse) are this port's where the reference carries
//! V8/Bun phrases — the shared wording deviation family.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use super::onboard::{
    MARKER_END, MARKER_START, collapse_blank_runs, embedded_session_close, has_marker_section,
    read_session_close_config, remove_marker_section,
};

// ────────────────────────────────────────────────────────────
// Shared shapes (recipe-discovery.ts)
// ────────────────────────────────────────────────────────────

/// One recipe outcome (recipe-discovery.ts `RecipeResult`).
#[derive(Debug)]
pub(super) struct RecipeResult {
    pub(super) success: bool,
    pub(super) message: String,
}

impl RecipeResult {
    pub(super) fn ok(message: impl Into<String>) -> Self {
        Self {
            success: true,
            message: message.into(),
        }
    }

    pub(super) fn fail(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
        }
    }
}

/// A recipe that THREW instead of returning (setup.ts 750-762): the
/// reference converts a thrown error into a one-line failure wrapper
/// (`recipe "<name>" <action> threw (<label>): <msg>`); the message
/// rides in this alias.
pub(super) type RecipeThrow = String;

/// The three recipe actions (setup.ts 680/740).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Action {
    Install,
    Check,
    Remove,
}

impl Action {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Action::Install => "install",
            Action::Check => "check",
            Action::Remove => "remove",
        }
    }
}

/// Where a resolved recipe came from (recipe-discovery.ts
/// `RecipeSource`; the npm arm is unreachable in this port — see the
/// module docs).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RecipeSource {
    Builtin,
    FilesystemTs,
    FilesystemSh,
}

impl RecipeSource {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            RecipeSource::Builtin => "builtin",
            RecipeSource::FilesystemTs => "filesystem-ts",
            RecipeSource::FilesystemSh => "filesystem-sh",
        }
    }

    /// The `--list` plain-source sort key (setup.ts 840):
    /// filesystem-ts, then -sh, then npm, then builtin — filesystem
    /// before builtin so the ACTIVE recipe sorts first.
    pub(super) fn list_order(self) -> u8 {
        match self {
            RecipeSource::FilesystemTs => 0,
            RecipeSource::FilesystemSh => 1,
            RecipeSource::Builtin => 3,
        }
    }
}

/// A resolved recipe with its provenance (recipe-discovery.ts
/// `RecipeWithSource`; the `name` field is dropped — it is always
/// the caller's provider argument by construction).
#[derive(Debug)]
pub(super) struct RecipeWithSource {
    pub(super) source: RecipeSource,
    pub(super) recipe: ProviderRecipe,
    /// Absolute script path for filesystem recipes (the json envelope
    /// carries it verbatim; only `--list` relativizes).
    pub(super) path:   Option<PathBuf>,
}

/// A provider recipe: the three built-ins plus the `.sh` runner.
#[derive(Debug)]
pub(super) enum ProviderRecipe {
    Claude,
    Cursor,
    Codex,
    /// A `.mulch/recipes/<name>.sh` script (makeShellRecipe).
    Shell {
        script_path: PathBuf,
    },
}

impl ProviderRecipe {
    /// Runs one action; `Err` is the thrown-equivalent the command
    /// layer wraps (setup.ts 742-762).
    pub(super) fn run(&self, action: Action, cwd: &Path) -> Result<RecipeResult, RecipeThrow> {
        match (self, action) {
            (ProviderRecipe::Claude, Action::Install) => claude_install(cwd),
            (ProviderRecipe::Claude, Action::Check) => claude_check(cwd),
            (ProviderRecipe::Claude, Action::Remove) => claude_remove(cwd),
            (ProviderRecipe::Cursor, Action::Install) => cursor_install(cwd),
            (ProviderRecipe::Cursor, Action::Check) => cursor_check(cwd),
            (ProviderRecipe::Cursor, Action::Remove) => cursor_remove(cwd),
            (ProviderRecipe::Codex, Action::Install) => codex_install(cwd),
            (ProviderRecipe::Codex, Action::Check) => codex_check(cwd),
            (ProviderRecipe::Codex, Action::Remove) => codex_remove(cwd),
            (ProviderRecipe::Shell { script_path }, action) => {
                Ok(run_shell_recipe(script_path, action.as_str(), cwd))
            }
        }
    }
}

// ────────────────────────────────────────────────────────────
// Git hook helpers (setup.ts 41-146)
// ────────────────────────────────────────────────────────────

const HOOK_MARKER_START: &str = "# mulch:start";
const HOOK_MARKER_END: &str = "# mulch:end";

/// The managed pre-commit section — byte-identical to the reference
/// (setup.ts 44-53; its bytes already say `mulch`, verified against
/// the 0.10.7 binary's written hook). `concat!` because `const`
/// cannot call `format!`.
pub(super) const MULCH_HOOK_SECTION: &str = concat!(
    "# mulch:start\n",
    "# Run mulch validate before committing\n",
    "if command -v mulch >/dev/null 2>&1; then\n",
    "  mulch validate\n",
    "  if [ $? -ne 0 ]; then\n",
    "    echo \"mulch validate failed. Commit aborted.\"\n",
    "    exit 1\n",
    "  fi\n",
    "fi\n",
    "# mulch:end"
);

/// `installGitHook` (setup.ts 55-90).
pub(super) fn install_git_hook(cwd: &Path) -> Result<RecipeResult, RecipeThrow> {
    let git_dir = cwd.join(".git");
    if !git_dir.exists() {
        return Ok(RecipeResult::fail(
            "Not a git repository — .git directory not found.",
        ));
    }

    let hooks_dir = git_dir.join("hooks");
    std::fs::create_dir_all(&hooks_dir)
        .map_err(|source| format!("creating {}: {source}", hooks_dir.display()))?;

    let hook_path = hooks_dir.join("pre-commit");
    let mut content = String::new();
    if hook_path.exists() {
        content = std::fs::read_to_string(&hook_path)
            .map_err(|source| format!("reading {}: {source}", hook_path.display()))?;
        if content.contains(HOOK_MARKER_START) {
            return Ok(RecipeResult::ok("Git pre-commit hook already installed."));
        }
    }

    content = if content.is_empty() {
        format!("#!/bin/sh\n\n{MULCH_HOOK_SECTION}\n")
    } else {
        format!("{}\n\n{MULCH_HOOK_SECTION}\n", content.trim_end())
    };

    std::fs::write(&hook_path, &content)
        .map_err(|source| format!("writing {}: {source}", hook_path.display()))?;
    set_executable(&hook_path)
        .map_err(|source| format!("chmod {}: {source}", hook_path.display()))?;

    Ok(RecipeResult::ok("Installed mulch pre-commit git hook."))
}

/// `checkGitHook` (setup.ts 92-107).
pub(super) fn check_git_hook(cwd: &Path) -> Result<RecipeResult, RecipeThrow> {
    let hook_path = cwd.join(".git").join("hooks").join("pre-commit");
    let Some(content) = read_optional(&hook_path)? else {
        return Ok(RecipeResult::fail("Git pre-commit hook not found."));
    };
    if !content.contains(HOOK_MARKER_START) {
        return Ok(RecipeResult::fail(
            "Git pre-commit hook exists but has no mulch section.",
        ));
    }
    Ok(RecipeResult::ok("Git pre-commit hook is installed."))
}

/// `removeGitHook` (setup.ts 109-146).
pub(super) fn remove_git_hook(cwd: &Path) -> Result<RecipeResult, RecipeThrow> {
    let hook_path = cwd.join(".git").join("hooks").join("pre-commit");
    let Some(content) = read_optional(&hook_path)? else {
        return Ok(RecipeResult::ok(
            "Git pre-commit hook not found; nothing to remove.",
        ));
    };
    let (Some(start), Some(end)) = (
        content.find(HOOK_MARKER_START),
        content.find(HOOK_MARKER_END),
    ) else {
        return Ok(RecipeResult::ok(
            "No mulch section in pre-commit hook; nothing to remove.",
        ));
    };
    let before = &content[..start];
    let after = &content[end + HOOK_MARKER_END.len()..];
    let cleaned = collapse_blank_runs(&format!("{before}{after}"))
        .trim()
        .to_string();

    // If only the shebang (or nothing) remains, delete the file.
    if cleaned.is_empty() || cleaned == "#!/bin/sh" {
        std::fs::remove_file(&hook_path)
            .map_err(|source| format!("removing {}: {source}", hook_path.display()))?;
        return Ok(RecipeResult::ok(
            "Removed mulch pre-commit hook (file deleted).",
        ));
    }

    std::fs::write(&hook_path, format!("{cleaned}\n"))
        .map_err(|source| format!("writing {}: {source}", hook_path.display()))?;
    Ok(RecipeResult::ok(
        "Removed mulch section from pre-commit hook.",
    ))
}

/// Reads a file that may be absent (`existsSync` + `readFile` pair).
fn read_optional(path: &Path) -> Result<Option<String>, RecipeThrow> {
    if !path.exists() {
        return Ok(None);
    }
    std::fs::read_to_string(path)
        .map(Some)
        .map_err(|source| format!("reading {}: {source}", path.display()))
}

/// chmod 0755 (setup.ts `chmod(hookPath, 0o755)`).
fn set_executable(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

// ────────────────────────────────────────────────────────────
// Claude (setup.ts 152-312)
// ────────────────────────────────────────────────────────────

/// OUR hook command — the promote law (user directive 2026-10-13):
/// the reference writes `ml prime`, our generated settings name this
/// binary (README DEVIATIONS).
const CLAUDE_HOOK_COMMAND: &str = "mulch prime";

fn claude_settings_path(cwd: &Path) -> PathBuf {
    cwd.join(".claude").join("settings.json")
}

/// `parseClaudeSettings` (setup.ts 192-199): a parse failure throws
/// `Failed to parse Claude settings at <path>: <msg>`; the reason
/// wording is serde's where the reference carries V8's (the shared
/// wording deviation family).
fn parse_claude_settings(
    raw: &str,
    settings_path: &Path,
) -> Result<Map<String, Value>, RecipeThrow> {
    serde_json::from_str(raw).map_err(|error| {
        format!(
            "Failed to parse Claude settings at {}: {error}",
            settings_path.display()
        )
    })
}

/// The settings' `hooks` object — absent or `null` creates it (the
/// reference's falsy check); a present non-object is the reference's
/// strict-mode TypeError, thrown here with an owned wording.
fn hooks_object(settings: &mut Map<String, Value>) -> Result<&mut Map<String, Value>, RecipeThrow> {
    match settings.get("hooks") {
        Some(Value::Object(_)) => {}
        None | Some(Value::Null) => {
            settings.insert("hooks".into(), json!({}));
        }
        Some(_) => return Err("Claude settings \"hooks\" is not an object".into()),
    }
    let Some(Value::Object(hooks)) = settings.get_mut("hooks") else {
        unreachable!("the match above guarantees an object");
    };
    Ok(hooks)
}

/// The event's group ARRAY (`hooks.SessionStart` maps straight to
/// `ClaudeHookGroup[]`, setup.ts 218-225) — absent creates it; a
/// present non-array throws.
fn event_groups<'a>(
    hooks: &'a mut Map<String, Value>,
    event: &str,
) -> Result<&'a mut Vec<Value>, RecipeThrow> {
    match hooks.get(event) {
        Some(Value::Array(_)) => {}
        None | Some(Value::Null) => {
            hooks.insert(event.into(), Value::Array(Vec::new()));
        }
        Some(_) => return Err(format!("Claude hooks \"{event}\" is not a group array")),
    }
    let Some(Value::Array(groups)) = hooks.get_mut(event) else {
        unreachable!("the match above guarantees an array");
    };
    Ok(groups)
}

/// `hasMulchHook` (setup.ts 177-179): any group carrying a hook whose
/// command is ours. A malformed group is the reference's TypeError —
/// thrown here with an owned wording.
fn has_mulch_hook(groups: &[Value]) -> Result<bool, RecipeThrow> {
    for group in groups {
        let Value::Object(group) = group else {
            return Err("Claude hook group is not an object".into());
        };
        let Some(Value::Array(hooks)) = group.get("hooks") else {
            return Err("Claude hook group has no hooks array".into());
        };
        for hook in hooks {
            if hook.get("command") == Some(&json!(CLAUDE_HOOK_COMMAND)) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// `createMulchHookGroup` (setup.ts 185-190) — key order pinned
/// (`matcher`, then `hooks`; `type`, then `command`).
fn create_mulch_hook_group() -> Value {
    json!({
        "matcher": "",
        "hooks": [ { "type": "command", "command": CLAUDE_HOOK_COMMAND } ]
    })
}

/// Writes settings as `JSON.stringify(settings, null, 2)` + newline —
/// serde pretty (2-space) over the preserve-order map matches JS
/// object key order, including `{}` for an empty object.
fn write_claude_settings(path: &Path, settings: &Map<String, Value>) -> Result<(), RecipeThrow> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|source| format!("creating {}: {source}", parent.display()))?;
    }
    let body = serde_json::to_string_pretty(settings)
        .map_err(|source| format!("serializing {}: {source}", path.display()))?;
    std::fs::write(path, format!("{body}\n"))
        .map_err(|source| format!("writing {}: {source}", path.display()))
}

fn claude_install(cwd: &Path) -> Result<RecipeResult, RecipeThrow> {
    let settings_path = claude_settings_path(cwd);
    let mut settings = match read_optional(&settings_path)? {
        Some(raw) => parse_claude_settings(&raw, &settings_path)?,
        None => Map::new(),
    };

    // SessionStart with empty matcher already covers startup, resume,
    // clear, and post-compact reload; PreCompact's stdout never
    // reaches the model after compaction (setup.ts 215-218).
    let event = "SessionStart";
    let hooks = hooks_object(&mut settings)?;
    let groups = event_groups(hooks, event)?;
    if has_mulch_hook(groups)? {
        return Ok(RecipeResult::ok("Claude hooks already installed."));
    }
    groups.push(create_mulch_hook_group());

    write_claude_settings(&settings_path, &settings)?;
    Ok(RecipeResult::ok("Installed Claude SessionStart hook."))
}

fn claude_check(cwd: &Path) -> Result<RecipeResult, RecipeThrow> {
    let settings_path = claude_settings_path(cwd);
    let Some(raw) = read_optional(&settings_path)? else {
        return Ok(RecipeResult::fail("Claude settings.json not found."));
    };
    let settings = parse_claude_settings(&raw, &settings_path)?;

    // Absent, null, AND malformed `hooks` all read as "no hooks"
    // here (the reference reaches the same message for the first
    // two; its truthy non-object edge is a TypeError family we
    // fold into the same answer — malformed settings, one verdict).
    let Some(Value::Object(hooks)) = settings.get("hooks") else {
        return Ok(RecipeResult::fail(
            "No hooks configured in Claude settings.",
        ));
    };
    let event = "SessionStart";
    let installed = match hooks.get(event) {
        Some(Value::Array(groups)) => has_mulch_hook(groups)?,
        _ => false,
    };
    if !installed {
        return Ok(RecipeResult::fail(format!("Missing hooks for: {event}.")));
    }
    Ok(RecipeResult::ok("Claude hooks are installed and correct."))
}

fn claude_remove(cwd: &Path) -> Result<RecipeResult, RecipeThrow> {
    let settings_path = claude_settings_path(cwd);
    let Some(raw) = read_optional(&settings_path)? else {
        return Ok(RecipeResult::ok(
            "Claude settings.json not found; nothing to remove.",
        ));
    };
    let mut settings = parse_claude_settings(&raw, &settings_path)?;

    // Absent/null `hooks` (and, folded in, malformed values) read as
    // "nothing to remove"; only a live object can carry our groups.
    let Some(Value::Object(hooks)) = settings.get_mut("hooks") else {
        return Ok(RecipeResult::ok(
            "No hooks in Claude settings; nothing to remove.",
        ));
    };

    let mut removed = false;
    let events: Vec<String> = hooks.keys().cloned().collect();
    for event in events {
        // Only a group ARRAY is a removable event (the reference's
        // `if (!hookGroup) continue` skips falsy values; non-array
        // truthy edges are malformed settings and stay untouched).
        let Some(Value::Array(groups)) = hooks.get_mut(&event) else {
            continue;
        };
        let before = groups.len();
        groups.retain(|group| {
            !group
                .get("hooks")
                .and_then(Value::as_array)
                .is_some_and(|hooks| {
                    hooks
                        .iter()
                        .any(|hook| hook.get("command") == Some(&json!(CLAUDE_HOOK_COMMAND)))
                })
        });
        if groups.len() < before {
            removed = true;
        }
        if groups.is_empty() {
            hooks.remove(&event);
        }
    }
    if hooks.is_empty() {
        settings.remove("hooks");
    }

    write_claude_settings(&settings_path, &settings)?;
    Ok(RecipeResult::ok(if removed {
        "Removed mulch hooks from Claude settings."
    } else {
        "No mulch hooks found in Claude settings."
    }))
}

// ────────────────────────────────────────────────────────────
// Cursor (setup.ts 314-396)
// ────────────────────────────────────────────────────────────

fn cursor_rule_path(cwd: &Path) -> PathBuf {
    cwd.join(".cursor").join("rules").join("mulch.mdc")
}

/// `buildCursorRuleContent` (setup.ts 322-346): the template with
/// THIS binary's spellings (the reference says `ml prime` — promote
/// law), footer = the embedded session-close from the config
/// (onboard's seam) with the default preset fallback.
fn build_cursor_rule_content(cwd: &Path) -> String {
    let session_close = read_session_close_config(cwd);
    let footer = embedded_session_close(session_close.as_ref());
    format!(
        "---
description: Mulch expertise integration
globs: *
alwaysApply: true
---

# Mulch Expertise

At the start of every session, run the following command to load project expertise:

```
mulch prime
```

This injects project-specific conventions, patterns, decisions, and other learnings into your context.
Use `mulch prime --files src/foo.ts` to load only records relevant to specific files.

Evidence auto-populates from git (current commit + changed files). Link trackers explicitly with `--evidence-seeds <id>` / `--evidence-gh <id>` / `--evidence-linear <id>` / `--evidence-bead <id>`, or `--relates-to <mx-id>`.

{footer}
"
    )
}

fn cursor_install(cwd: &Path) -> Result<RecipeResult, RecipeThrow> {
    let rule_path = cursor_rule_path(cwd);
    let content = build_cursor_rule_content(cwd);

    if let Some(existing) = read_optional(&rule_path)?
        && existing == content
    {
        return Ok(RecipeResult::ok("Cursor rule already installed."));
    }

    if let Some(parent) = rule_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|source| format!("creating {}: {source}", parent.display()))?;
    }
    std::fs::write(&rule_path, &content)
        .map_err(|source| format!("writing {}: {source}", rule_path.display()))?;

    Ok(RecipeResult::ok(
        "Installed Cursor rule at .cursor/rules/mulch.mdc.",
    ))
}

fn cursor_check(cwd: &Path) -> Result<RecipeResult, RecipeThrow> {
    let rule_path = cursor_rule_path(cwd);
    let Some(content) = read_optional(&rule_path)? else {
        return Ok(RecipeResult::fail("Cursor rule file not found."));
    };
    if content != build_cursor_rule_content(cwd) {
        return Ok(RecipeResult::fail(
            "Cursor rule file exists but has been modified.",
        ));
    }
    Ok(RecipeResult::ok("Cursor rule is installed and correct."))
}

fn cursor_remove(cwd: &Path) -> Result<RecipeResult, RecipeThrow> {
    let rule_path = cursor_rule_path(cwd);
    if !rule_path.exists() {
        return Ok(RecipeResult::ok(
            "Cursor rule not found; nothing to remove.",
        ));
    }
    std::fs::remove_file(&rule_path)
        .map_err(|source| format!("removing {}: {source}", rule_path.display()))?;
    Ok(RecipeResult::ok("Removed Cursor rule file."))
}

// ────────────────────────────────────────────────────────────
// Codex (setup.ts 398-585)
// ────────────────────────────────────────────────────────────

fn codex_agents_path(cwd: &Path) -> PathBuf {
    cwd.join("AGENTS.md")
}

/// `buildCodexSection` (setup.ts 404-419): the onboard markers plus
/// promoted prose (the reference says `ml prime`) and the embedded
/// session-close footer.
fn build_codex_section(cwd: &Path) -> String {
    let session_close = read_session_close_config(cwd);
    let footer = embedded_session_close(session_close.as_ref());
    format!(
        "{MARKER_START}
## Mulch Expertise

At the start of every session, run `mulch prime` to load project expertise.

This injects project-specific conventions, patterns, decisions, and other learnings into your context.
Use `mulch prime --files src/foo.ts` to load only records relevant to specific files.

Evidence auto-populates from git (current commit + changed files). Link trackers explicitly with `--evidence-seeds <id>` / `--evidence-gh <id>` / `--evidence-linear <id>` / `--evidence-bead <id>`, or `--relates-to <mx-id>`.

{footer}
{MARKER_END}"
    )
}

/// The toml block's fence lines. The start marker names the command
/// invocation — OURS says `mulch setup codex` (promote law; the
/// reference writes `ml setup codex`), so a reference-written block
/// is not detected by this port and vice versa (documented
/// DEVIATION: the marker bytes track the binary that wrote them).
const CODEX_TOML_MARKER_START: &str = "# mulch:start — managed by `mulch setup codex`";
const CODEX_TOML_MARKER_END: &str = "# mulch:end";

/// The managed `.codex/config.toml` block (setup.ts 431-441) — the
/// fence marker and the hook command promoted to this binary
/// (`mulch setup codex`, `mulch prime`). `concat!` because `const`
/// cannot call `format!`.
pub(super) const CODEX_TOML_BLOCK: &str = concat!(
    "# mulch:start — managed by `mulch setup codex`\n",
    "[features]\n",
    "codex_hooks = true\n",
    "\n",
    "[[hooks.SessionStart]]\n",
    "\n",
    "[[hooks.SessionStart.hooks]]\n",
    "type = \"command\"\n",
    "command = \"mulch prime\"\n",
    "statusMessage = \"Loading mulch expertise\"\n",
    "# mulch:end"
);

fn codex_config_path(cwd: &Path) -> PathBuf {
    cwd.join(".codex").join("config.toml")
}

fn has_codex_toml_block(content: &str) -> bool {
    content.contains(CODEX_TOML_MARKER_START)
}

/// `removeCodexTomlBlock` (setup.ts 451-463): cut from the start
/// marker through the END OF THE LINE carrying the first end marker
/// after it, collapse blank runs, `trimEnd`; an empty result is `""`
/// (the caller keeps an empty file — never deletes it).
fn remove_codex_toml_block(content: &str) -> String {
    let Some(start) = content.find(CODEX_TOML_MARKER_START) else {
        return content.to_string();
    };
    let Some(end_marker) = content[start..]
        .find(CODEX_TOML_MARKER_END)
        .map(|offset| start + offset)
    else {
        return content.to_string();
    };
    let cut_end = match content[end_marker..].find('\n') {
        Some(newline) => end_marker + newline + 1,
        None => content.len(),
    };
    let before = &content[..start];
    let after = &content[cut_end..];
    let cleaned = collapse_blank_runs(&format!("{before}{after}"))
        .trim_end()
        .to_string();
    if cleaned.is_empty() {
        String::new()
    } else {
        format!("{cleaned}\n")
    }
}

fn codex_install(cwd: &Path) -> Result<RecipeResult, RecipeThrow> {
    let agents_path = codex_agents_path(cwd);
    let toml_path = codex_config_path(cwd);

    // 1. AGENTS.md — fallback prose for Codex versions without hook support
    //    (setup.ts 470-484).
    let agents_content = read_optional(&agents_path)?.unwrap_or_default();
    let agents_already = has_marker_section(&agents_content);
    if !agents_already {
        let codex_section = build_codex_section(cwd);
        let new_agents = if agents_content.is_empty() {
            format!("{codex_section}\n")
        } else {
            format!("{}\n\n{codex_section}\n", agents_content.trim_end())
        };
        std::fs::write(&agents_path, new_agents)
            .map_err(|source| format!("writing {}: {source}", agents_path.display()))?;
    }

    // 2. .codex/config.toml — the SessionStart hook (setup.ts 486-500).
    let toml_content = read_optional(&toml_path)?.unwrap_or_default();
    let toml_already = has_codex_toml_block(&toml_content);
    if !toml_already {
        if let Some(parent) = toml_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|source| format!("creating {}: {source}", parent.display()))?;
        }
        let new_toml = if toml_content.trim().is_empty() {
            format!("{CODEX_TOML_BLOCK}\n")
        } else {
            format!("{}\n\n{CODEX_TOML_BLOCK}\n", toml_content.trim_end())
        };
        std::fs::write(&toml_path, new_toml)
            .map_err(|source| format!("writing {}: {source}", toml_path.display()))?;
    }

    if agents_already && toml_already {
        return Ok(RecipeResult::ok(
            "Codex integration already installed (AGENTS.md + .codex/config.toml).",
        ));
    }

    let mut installed: Vec<&str> = Vec::new();
    if !agents_already {
        installed.push("AGENTS.md mulch section");
    }
    if !toml_already {
        installed.push(".codex/config.toml SessionStart hook");
    }
    Ok(RecipeResult::ok(format!(
        "Installed Codex integration: {}.",
        installed.join(", ")
    )))
}

fn codex_check(cwd: &Path) -> Result<RecipeResult, RecipeThrow> {
    let agents_path = codex_agents_path(cwd);
    let toml_path = codex_config_path(cwd);

    let Some(agents_content) = read_optional(&agents_path)? else {
        return Ok(RecipeResult::fail("AGENTS.md not found."));
    };
    if !has_marker_section(&agents_content) {
        return Ok(RecipeResult::fail(
            "AGENTS.md exists but has no mulch section.",
        ));
    }

    let Some(toml_content) = read_optional(&toml_path)? else {
        return Ok(RecipeResult::fail(".codex/config.toml not found."));
    };
    if !has_codex_toml_block(&toml_content) {
        return Ok(RecipeResult::fail(
            ".codex/config.toml exists but has no mulch SessionStart hook.",
        ));
    }

    Ok(RecipeResult::ok(
        "Codex integration installed (AGENTS.md + .codex/config.toml).",
    ))
}

fn codex_remove(cwd: &Path) -> Result<RecipeResult, RecipeThrow> {
    let agents_path = codex_agents_path(cwd);
    let toml_path = codex_config_path(cwd);
    let mut removed: Vec<&str> = Vec::new();

    if let Some(content) = read_optional(&agents_path)?
        && has_marker_section(&content)
    {
        let cleaned = remove_marker_section(&content);
        std::fs::write(&agents_path, cleaned)
            .map_err(|source| format!("writing {}: {source}", agents_path.display()))?;
        removed.push("AGENTS.md mulch section");
    }

    if let Some(content) = read_optional(&toml_path)?
        && has_codex_toml_block(&content)
    {
        let cleaned = remove_codex_toml_block(&content);
        // An empty result keeps an EMPTY file rather than deleting
        // (setup.ts 566-571: a user-managed block coming back later
        // shouldn't fight a recreate race; an empty file is harmless).
        std::fs::write(&toml_path, &cleaned)
            .map_err(|source| format!("writing {}: {source}", toml_path.display()))?;
        removed.push(".codex/config.toml SessionStart hook");
    }

    if removed.is_empty() {
        return Ok(RecipeResult::ok(
            "No Codex integration found; nothing to remove.",
        ));
    }
    Ok(RecipeResult::ok(format!(
        "Removed Codex integration: {}.",
        removed.join(", ")
    )))
}

// ────────────────────────────────────────────────────────────
// Recipe discovery (recipe-discovery.ts, native-honest subset)
// ────────────────────────────────────────────────────────────

/// The npm recipe package prefix (recipe-discovery.ts:33) — appears
/// in the unknown-provider hint and the npm limitation error.
pub(super) const NPM_RECIPE_PREFIX: &str = "mulch-recipe-";

fn recipes_dir(cwd: &Path) -> PathBuf {
    cwd.join(".mulch").join("recipes")
}

/// One `--list` row (recipe-discovery.ts `FilesystemRecipeListing`
/// plus the built-in rows setup.ts adds).
pub(super) struct ProviderListing {
    pub(super) name:   String,
    pub(super) source: RecipeSource,
    pub(super) path:   Option<PathBuf>,
}

/// `listFilesystemRecipes` (recipe-discovery.ts 155-176): every
/// `.ts`/`.sh` entry under `.mulch/recipes/`, sorted by name. The
/// `.ts` entries are LISTED but not executable here (resolving them
/// errors) — the native-build deviation, documented in the README.
pub(super) fn list_filesystem_recipes(cwd: &Path) -> Vec<ProviderListing> {
    let dir = recipes_dir(cwd);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        let (stem, source) = if let Some(stem) = name.strip_suffix(".ts") {
            (stem, RecipeSource::FilesystemTs)
        } else if let Some(stem) = name.strip_suffix(".sh") {
            (stem, RecipeSource::FilesystemSh)
        } else {
            continue;
        };
        out.push(ProviderListing {
            name: stem.to_string(),
            source,
            path: Some(entry.path()),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// `resolveRecipe` (recipe-discovery.ts 178-195), native-honest:
/// filesystem `.ts` (ERROR — a Rust binary cannot import TypeScript)
/// → filesystem `.sh` → npm (ERROR when a local `node_modules`
/// package exists — no JS runtime to load it through; the reference's
/// walk-up resolution is approximated by the cwd-local lookup) →
/// built-in. `Ok(None)` is the unknown provider.
pub(super) fn resolve_recipe(
    name: &str,
    cwd: &Path,
) -> Result<Option<RecipeWithSource>, RecipeThrow> {
    let ts_path = recipes_dir(cwd).join(format!("{name}.ts"));
    if ts_path.exists() {
        return Err(format!(
            "Recipe \"{name}\" at {} is a TypeScript recipe — this native build cannot import \
             TypeScript recipes; port it to a .sh script (invoked as <script> install|check|remove \
             with MULCH_RECIPE_NAME/MULCH_RECIPE_ACTION in the environment) or run it with the \
             reference ml CLI.",
            ts_path.display()
        ));
    }

    let sh_path = recipes_dir(cwd).join(format!("{name}.sh"));
    if sh_path.exists() {
        return Ok(Some(RecipeWithSource {
            source: RecipeSource::FilesystemSh,
            recipe: ProviderRecipe::Shell {
                script_path: sh_path.clone(),
            },
            path:   Some(sh_path),
        }));
    }

    let npm_dir = cwd
        .join("node_modules")
        .join(format!("{NPM_RECIPE_PREFIX}{name}"));
    if npm_dir.exists() {
        return Err(format!(
            "npm recipe package \"{NPM_RECIPE_PREFIX}{name}\" found at {} — this native build \
             cannot load npm recipes; port it to a .mulch/recipes/{name}.sh script or run it with \
             the reference ml CLI.",
            npm_dir.display()
        ));
    }

    let recipe = match name {
        "claude" => ProviderRecipe::Claude,
        "cursor" => ProviderRecipe::Cursor,
        "codex" => ProviderRecipe::Codex,
        _ => return Ok(None),
    };
    Ok(Some(RecipeWithSource {
        source: RecipeSource::Builtin,
        recipe,
        path: None,
    }))
}

/// `makeShellRecipe`'s runner (recipe-discovery.ts 49-91): spawn the
/// script with the action as argv, the recipe name/action in the
/// env, cwd-scoped; trimmed stdout is the message, the fallbacks are
/// `"<name> <action> succeeded"` / `… exited with code <c>: <stderr>`
/// (`?` when killed by a signal — Node's `code ?? "?"`). A spawn
/// failure (missing binary, no exec bit) renders the reference's
/// `Failed to execute <path>: <msg>` shape with our io wording.
fn run_shell_recipe(script_path: &Path, action: &str, cwd: &Path) -> RecipeResult {
    // file_stem of the script IS the recipe name (recipes are
    // discovered as `.mulch/recipes/<name>.sh`).
    let name = script_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default()
        .to_string();

    let output = match std::process::Command::new(script_path)
        .arg(action)
        .current_dir(cwd)
        .env("MULCH_RECIPE_NAME", &name)
        .env("MULCH_RECIPE_ACTION", action)
        .stdin(std::process::Stdio::null())
        .output()
    {
        Ok(output) => output,
        Err(source) => {
            return RecipeResult::fail(format!(
                "Failed to execute {}: {source}",
                script_path.display()
            ));
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let ok = output.status.success();
    let fallback = if ok {
        format!("{name} {action} succeeded")
    } else {
        let code = output
            .status
            .code()
            .map_or_else(|| "?".into(), |code| code.to_string());
        if stderr.is_empty() {
            format!("{name} {action} exited with code {code}")
        } else {
            format!("{name} {action} exited with code {code}: {stderr}")
        }
    };
    RecipeResult {
        success: ok,
        message: if stdout.is_empty() { fallback } else { stdout },
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// A unique scratch dir per call (explicit paths, no chdir — the
    /// pid plus a serial keep parallel tests apart, the git.rs law).
    fn scratch(tag: &str) -> PathBuf {
        static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let serial = SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "mulch-setup-recipes-{tag}-{}-{serial}",
            std::process::id()
        ));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("scratch root");
        dir
    }

    /// A scratch dir with `.mulch/` present (the command-level store
    /// gate; the recipes themselves only read the config knob).
    fn scratch_store(tag: &str) -> PathBuf {
        let dir = scratch(tag);
        std::fs::create_dir_all(dir.join(".mulch")).expect("store dir");
        dir
    }

    /// The managed hook section, byte-pinned against the reference's
    /// written hook (probed 2026-10-26): its bytes already say
    /// `mulch` — no promote surface, byte-identical.
    #[test]
    fn mulch_hook_section_bytes_are_reference_identical() {
        assert_eq!(
            MULCH_HOOK_SECTION,
            "# mulch:start\n# Run mulch validate before committing\nif command -v mulch \
             >/dev/null 2>&1; then\n  mulch validate\n  if [ $? -ne 0 ]; then\n    echo \
             \"mulch validate failed. Commit aborted.\"\n    exit 1\n  fi\nfi\n# mulch:end"
        );
    }

    /// The git-hook lifecycle on a scratch `.git`: fresh install
    /// writes shebang + section + newline and chmods 0755; a rerun is
    /// already-installed with unmoved bytes; append to existing
    /// content follows trimEnd + one blank line; removal collapses
    /// and keeps the user's bytes; only-shebang removal deletes the
    /// file (setup.ts 55-146).
    #[test]
    fn git_hook_lifecycle_follows_the_reference_laws() {
        let dir = scratch("hookcycle");
        std::fs::create_dir_all(dir.join(".git")).expect("git dir");

        let install = install_git_hook(&dir).expect("install runs");
        assert!(install.success);
        assert_eq!(install.message, "Installed mulch pre-commit git hook.");
        let hook = dir.join(".git/hooks/pre-commit");
        let fresh = std::fs::read_to_string(&hook).expect("hook written");
        assert_eq!(
            fresh,
            format!("#!/bin/sh\n\n{MULCH_HOOK_SECTION}\n"),
            "fresh install: shebang + blank + section + newline"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            let mode = std::fs::metadata(&hook).expect("metadata").mode() & 0o777;
            assert_eq!(mode, 0o755, "the hook is executable");
        }

        let again = install_git_hook(&dir).expect("rerun");
        assert_eq!(again.message, "Git pre-commit hook already installed.");
        assert_eq!(
            std::fs::read_to_string(&hook).expect("hook"),
            fresh,
            "already-installed writes nothing"
        );

        let check = check_git_hook(&dir).expect("check");
        assert!(check.success);
        assert_eq!(check.message, "Git pre-commit hook is installed.");

        // Append to existing content: trimEnd + exactly one blank line.
        std::fs::write(&hook, "#!/bin/sh\n\necho \"custom logic\"\n\n\n").expect("custom hook");
        let appended = install_git_hook(&dir).expect("append install");
        assert_eq!(appended.message, "Installed mulch pre-commit git hook.");
        let content = std::fs::read_to_string(&hook).expect("appended hook");
        assert_eq!(
            content,
            format!("#!/bin/sh\n\necho \"custom logic\"\n\n{MULCH_HOOK_SECTION}\n")
        );

        // Removal keeps the user's bytes and collapses the blanks.
        let remove = remove_git_hook(&dir).expect("remove");
        assert_eq!(
            remove.message,
            "Removed mulch section from pre-commit hook."
        );
        assert_eq!(
            std::fs::read_to_string(&hook).expect("cleaned hook"),
            "#!/bin/sh\n\necho \"custom logic\"\n"
        );

        // A second remove on the cleaned file: no marker, nothing to
        // do (probed: the section-less hook is left alone).
        let no_section = remove_git_hook(&dir).expect("remove without section");
        assert_eq!(
            no_section.message,
            "No mulch section in pre-commit hook; nothing to remove."
        );
        assert!(no_section.success);

        // Only the shebang remains (a fresh install on a bare hook):
        // the file is deleted.
        std::fs::remove_file(&hook).expect("reset hook");
        install_git_hook(&dir).expect("fresh install");
        let remove_again = remove_git_hook(&dir).expect("remove shebang-only");
        assert_eq!(
            remove_again.message,
            "Removed mulch pre-commit hook (file deleted)."
        );
        assert!(!hook.exists(), "the hook file is gone");

        let no_hook = remove_git_hook(&dir).expect("remove missing");
        assert_eq!(
            no_hook.message,
            "Git pre-commit hook not found; nothing to remove."
        );
        assert!(no_hook.success);
    }

    /// No `.git`: the install failure message (an em-dash), success
    /// false — never a throw.
    #[test]
    fn git_hook_install_outside_a_repository_fails_cleanly() {
        let dir = scratch("nogit");
        let install = install_git_hook(&dir).expect("install runs");
        assert!(!install.success);
        assert_eq!(
            install.message,
            "Not a git repository — .git directory not found."
        );
        let check = check_git_hook(&dir).expect("check runs");
        assert_eq!(check.message, "Git pre-commit hook not found.");
        assert!(!check.success);
    }

    /// The claude settings rewrite law: unknown keys and their order
    /// survive, our hook lands in `hooks.SessionStart` (an array of
    /// groups, `mulch prime` command — the promote surface), and the
    /// remove pass empties `hooks` to `{}` leaving unknown keys
    /// intact (setup.ts 201-312).
    #[test]
    fn claude_settings_rewrite_preserves_unknown_keys_and_order() {
        let dir = scratch("claudesettings");
        let settings_path = dir.join(".claude").join("settings.json");
        std::fs::create_dir_all(settings_path.parent().expect("parent")).expect("dir");
        std::fs::write(
            &settings_path,
            concat!(
                "{\n",
                "  \"model\": \"sonnet\",\n",
                "  \"hooks\": {\n",
                "    \"PreCompact\": [\n",
                "      {\n",
                "        \"matcher\": \"x\",\n",
                "        \"hooks\": [\n",
                "          {\n",
                "            \"type\": \"command\",\n",
                "            \"command\": \"other-tool\"\n",
                "          }\n",
                "        ]\n",
                "      }\n",
                "    ]\n",
                "  },\n",
                "  \"verdict\": true\n",
                "}\n"
            ),
        )
        .expect("seed settings");

        let install = claude_install(&dir).expect("install");
        assert_eq!(install.message, "Installed Claude SessionStart hook.");
        let written = std::fs::read_to_string(&settings_path).expect("settings");
        // Key order: model, hooks (PreCompact first, SessionStart
        // appended), verdict — insertion order preserved end to end.
        assert_eq!(
            written,
            concat!(
                "{\n",
                "  \"model\": \"sonnet\",\n",
                "  \"hooks\": {\n",
                "    \"PreCompact\": [\n",
                "      {\n",
                "        \"matcher\": \"x\",\n",
                "        \"hooks\": [\n",
                "          {\n",
                "            \"type\": \"command\",\n",
                "            \"command\": \"other-tool\"\n",
                "          }\n",
                "        ]\n",
                "      }\n",
                "    ],\n",
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
                "  },\n",
                "  \"verdict\": true\n",
                "}\n"
            ),
            "serde pretty matches JSON.stringify(settings, null, 2) + \\n"
        );

        // Idempotency: the second install changes nothing.
        let again = claude_install(&dir).expect("rerun");
        assert_eq!(again.message, "Claude hooks already installed.");
        assert_eq!(
            std::fs::read_to_string(&settings_path).expect("settings"),
            written
        );

        let check = claude_check(&dir).expect("check");
        assert_eq!(check.message, "Claude hooks are installed and correct.");

        // Remove: the foreign PreCompact group survives (no mulch
        // hook inside), our SessionStart array empties and the event
        // key goes — hooks stays, model/verdict untouched.
        let remove = claude_remove(&dir).expect("remove");
        assert_eq!(remove.message, "Removed mulch hooks from Claude settings.");
        assert_eq!(
            std::fs::read_to_string(&settings_path).expect("settings"),
            concat!(
                "{\n",
                "  \"model\": \"sonnet\",\n",
                "  \"hooks\": {\n",
                "    \"PreCompact\": [\n",
                "      {\n",
                "        \"matcher\": \"x\",\n",
                "        \"hooks\": [\n",
                "          {\n",
                "            \"type\": \"command\",\n",
                "            \"command\": \"other-tool\"\n",
                "          }\n",
                "        ]\n",
                "      }\n",
                "    ]\n",
                "  },\n",
                "  \"verdict\": true\n",
                "}\n"
            )
        );
    }

    /// A settings file with ONLY our hook collapses to `{}` + newline
    /// on remove (the `delete settings.hooks` law, probed).
    #[test]
    fn claude_remove_empties_the_settings_object() {
        let dir = scratch("claudeempty");
        claude_install(&dir).expect("install");
        let remove = claude_remove(&dir).expect("remove");
        assert_eq!(remove.message, "Removed mulch hooks from Claude settings.");
        assert_eq!(
            std::fs::read_to_string(dir.join(".claude/settings.json")).expect("settings"),
            "{}\n"
        );
        let again = claude_remove(&dir).expect("remove again");
        // Empty settings: no `hooks` key at all (probed — the
        // reference's falsy check short-circuits here too).
        assert_eq!(
            again.message,
            "No hooks in Claude settings; nothing to remove."
        );
    }

    /// A malformed settings file throws the parse shape the wrapper
    /// renders (serde's reason wording — the shared deviation).
    #[test]
    fn claude_broken_settings_throws_the_parse_shape() {
        let dir = scratch("claudebroken");
        std::fs::create_dir_all(dir.join(".claude")).expect("dir");
        std::fs::write(dir.join(".claude/settings.json"), "{bad json").expect("seed");
        let error = claude_install(&dir).expect_err("install throws");
        assert!(
            error.starts_with(&format!(
                "Failed to parse Claude settings at {}",
                dir.join(".claude/settings.json").display()
            )),
            "throw carries the path: {error}"
        );
    }

    /// The cursor rule promotes THIS binary: no `ml ` spellings
    /// anywhere, the footer is the conditional embedded preset by
    /// default, and `none` suppresses it (the config knob flows).
    #[test]
    fn cursor_rule_content_promotes_this_binary() {
        let dir = scratch_store("cursorcontent");
        let content = build_cursor_rule_content(&dir);
        assert!(content.starts_with(
            "---\ndescription: Mulch expertise integration\nglobs: *\nalwaysApply: \
             true\n---\n\n# Mulch Expertise\n"
        ));
        assert!(content.contains("```\nmulch prime\n```"));
        assert!(content.contains("Use `mulch prime --files src/foo.ts`"));
        assert!(content.contains("### Before You Finish"));
        assert!(content.ends_with(
            "Skip if no insight surfaced. Unrecorded learnings are lost; ritual filler records \
             are also noise.\n"
        ));
        assert!(!content.contains("ml "), "the promote law reaches the rule");

        // The session-close knob flows into the footer.
        std::fs::write(
            dir.join(".mulch/mulch.config.yaml"),
            "prime:\n  session_close:\n    style: none\n",
        )
        .expect("config");
        let none = build_cursor_rule_content(&dir);
        assert!(!none.contains("### Before You Finish"));
    }

    /// The codex section and toml block: promoted prose, pinned
    /// bytes, and the marker deviation — a REFERENCE-written toml
    /// block (`ml setup codex` fence) is not ours to detect.
    #[test]
    fn codex_section_and_toml_block_laws() {
        let dir = scratch_store("codexcontent");
        let section = build_codex_section(&dir);
        assert!(section.starts_with("<!-- mulch:start -->\n## Mulch Expertise\n"));
        assert!(section.ends_with("<!-- mulch:end -->"));
        assert!(section.contains("run `mulch prime` to load project expertise"));
        assert!(
            !section.contains("ml "),
            "the promote law reaches the section"
        );

        assert_eq!(
            CODEX_TOML_BLOCK,
            "# mulch:start — managed by `mulch setup codex`\n[features]\ncodex_hooks = \
             true\n\n[[hooks.SessionStart]]\n\n[[hooks.SessionStart.hooks]]\ntype = \
             \"command\"\ncommand = \"mulch prime\"\nstatusMessage = \"Loading mulch \
             expertise\"\n# mulch:end"
        );
        // The reference's fence names `ml setup codex` — deliberately
        // not detected (marker bytes track the writing binary).
        assert!(!has_codex_toml_block(
            "# mulch:start — managed by `ml setup codex`\n"
        ));
        assert!(has_codex_toml_block(&format!("{CODEX_TOML_BLOCK}\n")));
    }

    /// `removeCodexTomlBlock`: the block alone empties the file;
    /// block-between-user-content collapses to the user content with
    /// one trailing newline; a start marker without an end marker
    /// after it stays untouched (setup.ts 451-463).
    #[test]
    fn remove_codex_toml_block_cuts_and_collapses() {
        assert_eq!(
            remove_codex_toml_block(&format!("{CODEX_TOML_BLOCK}\n")),
            ""
        );

        let mixed = format!("[user]\nkey = 1\n\n{CODEX_TOML_BLOCK}\n\n[other]\nkey = 2\n");
        assert_eq!(
            remove_codex_toml_block(&mixed),
            "[user]\nkey = 1\n\n[other]\nkey = 2\n",
            "the cut collapses the leftover blank runs"
        );

        let unterminated = format!("[user]\n{CODEX_TOML_MARKER_START}tail");
        assert_eq!(
            remove_codex_toml_block(&unterminated),
            unterminated,
            "no end marker after the start: unchanged"
        );
        assert_eq!(remove_codex_toml_block("no markers\n"), "no markers\n");
    }

    /// The codex lifecycle on a scratch store: install both files,
    /// idempotent rerun, check, remove (AGENTS.md keeps a single
    /// newline; the toml stays as an EMPTY file), nothing-to-remove.
    #[test]
    fn codex_lifecycle_adds_and_removes_both_files() {
        let dir = scratch_store("codexcycle");
        let install = codex_install(&dir).expect("install");
        assert_eq!(
            install.message,
            "Installed Codex integration: AGENTS.md mulch section, .codex/config.toml \
             SessionStart hook."
        );
        let agents = std::fs::read_to_string(dir.join("AGENTS.md")).expect("agents");
        let toml = std::fs::read_to_string(dir.join(".codex/config.toml")).expect("toml");
        assert!(agents.starts_with("<!-- mulch:start -->"));
        assert_eq!(toml, format!("{CODEX_TOML_BLOCK}\n"));

        let again = codex_install(&dir).expect("rerun");
        assert_eq!(
            again.message,
            "Codex integration already installed (AGENTS.md + .codex/config.toml)."
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("AGENTS.md")).expect("agents"),
            agents,
            "idempotent rerun writes nothing"
        );

        let check = codex_check(&dir).expect("check");
        assert_eq!(
            check.message,
            "Codex integration installed (AGENTS.md + .codex/config.toml)."
        );

        let remove = codex_remove(&dir).expect("remove");
        assert_eq!(
            remove.message,
            "Removed Codex integration: AGENTS.md mulch section, .codex/config.toml \
             SessionStart hook."
        );
        // The whole-file cut leaves a single newline in AGENTS.md
        // (removeMarkerSection's trailing law) and an EMPTY toml.
        assert_eq!(
            std::fs::read_to_string(dir.join("AGENTS.md")).expect("agents"),
            "\n"
        );
        assert_eq!(
            std::fs::read(dir.join(".codex/config.toml")).expect("toml"),
            b""
        );

        let none = codex_remove(&dir).expect("remove again");
        assert_eq!(
            none.message,
            "No Codex integration found; nothing to remove."
        );
    }

    /// Codex install into existing files appends after trimEnd with
    /// one blank line, and a non-empty toml keeps its user tables.
    #[test]
    fn codex_install_appends_to_existing_content() {
        let dir = scratch_store("codexappend");
        std::fs::write(dir.join("AGENTS.md"), "Project notes.\n").expect("agents");
        std::fs::create_dir_all(dir.join(".codex")).expect("codex dir");
        std::fs::write(dir.join(".codex/config.toml"), "[user]\nkey = 1\n").expect("toml");

        codex_install(&dir).expect("install");
        let agents = std::fs::read_to_string(dir.join("AGENTS.md")).expect("agents");
        assert!(agents.starts_with("Project notes.\n\n<!-- mulch:start -->"));
        let toml = std::fs::read_to_string(dir.join(".codex/config.toml")).expect("toml");
        assert!(toml.starts_with("[user]\nkey = 1\n\n# mulch:start"));

        let remove = codex_remove(&dir).expect("remove");
        assert!(remove.success);
        assert_eq!(
            std::fs::read_to_string(dir.join("AGENTS.md")).expect("agents"),
            "Project notes.\n"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join(".codex/config.toml")).expect("toml"),
            "[user]\nkey = 1\n"
        );
    }

    /// Resolution order and the native-build errors: `.ts` errors
    /// (shadowing a same-name `.sh`), `.sh` resolves with its path,
    /// a local npm package errors, builtins resolve, unknown is
    /// `None` (recipe-discovery.ts 178-195, DEVIATIONS).
    #[test]
    fn resolve_recipe_order_and_native_errors() {
        let dir = scratch_store("resolve");
        let recipes = dir.join(".mulch/recipes");
        std::fs::create_dir_all(&recipes).expect("recipes dir");

        // `.sh` resolves with source and absolute path.
        std::fs::write(recipes.join("helper.sh"), "#!/bin/sh\n").expect("sh");
        let resolved = resolve_recipe("helper", &dir)
            .expect("resolves")
            .expect("some");
        assert_eq!(resolved.source, RecipeSource::FilesystemSh);
        assert_eq!(
            resolved.path.as_deref(),
            Some(recipes.join("helper.sh").as_path())
        );

        // A `.ts` of the same name shadows the `.sh` with an ERROR
        // (the reference imports the `.ts` first — ours cannot).
        std::fs::write(recipes.join("helper.ts"), "export default {}\n").expect("ts");
        let error = resolve_recipe("helper", &dir).expect_err("ts errors");
        assert!(
            error.contains("TypeScript recipe"),
            "the error names the limitation: {error}"
        );
        assert!(error.contains(&recipes.join("helper.ts").display().to_string()));

        // A local npm package errors (no JS runtime to load through).
        std::fs::create_dir_all(dir.join("node_modules/mulch-recipe-npmmy")).expect("npm dir");
        let npm_error = resolve_recipe("npmmy", &dir).expect_err("npm errors");
        assert!(npm_error.contains("npm recipes"), "{npm_error}");

        // Builtins resolve source-only; unknown is None.
        for name in ["claude", "cursor", "codex"] {
            let resolved = resolve_recipe(name, &dir).expect("resolves").expect("some");
            assert_eq!(resolved.source, RecipeSource::Builtin);
            assert!(resolved.path.is_none());
        }
        assert!(resolve_recipe("zzz", &dir).expect("no throw").is_none());
    }

    /// The shell-recipe runner: echoed stdout wins, a silent success
    /// falls back to `"<name> <action> succeeded"`, a failing script
    /// falls back to the code + stderr shape (recipe-discovery.ts
    /// 49-91).
    #[test]
    fn shell_recipe_runner_follows_the_fallback_laws() {
        let dir = scratch_store("shellrun");
        let recipes = dir.join(".mulch/recipes");
        std::fs::create_dir_all(&recipes).expect("recipes dir");

        let talker = recipes.join("talker.sh");
        std::fs::write(
            &talker,
            "#!/bin/sh\necho \"did $MULCH_RECIPE_ACTION as \
             $MULCH_RECIPE_NAME\"\n",
        )
        .expect("talker");
        set_executable(&talker).expect("chmod");
        let talked = run_shell_recipe(&talker, "install", &dir);
        assert!(talked.success);
        assert_eq!(talked.message, "did install as talker");

        let quiet = recipes.join("quiet.sh");
        std::fs::write(&quiet, "#!/bin/sh\nexit 0\n").expect("quiet");
        set_executable(&quiet).expect("chmod");
        let silent = run_shell_recipe(&quiet, "check", &dir);
        assert!(silent.success);
        assert_eq!(silent.message, "quiet check succeeded");

        let failing = recipes.join("failing.sh");
        std::fs::write(&failing, "#!/bin/sh\necho boom >&2\nexit 3\n").expect("failing");
        set_executable(&failing).expect("chmod");
        let failed = run_shell_recipe(&failing, "remove", &dir);
        assert!(!failed.success);
        assert_eq!(failed.message, "failing remove exited with code 3: boom");

        // A non-executable script renders the Failed-to-execute shape.
        let inert = recipes.join("inert.sh");
        std::fs::write(&inert, "#!/bin/sh\n").expect("inert");
        let denied = run_shell_recipe(&inert, "install", &dir);
        assert!(!denied.success);
        assert!(
            denied
                .message
                .starts_with(&format!("Failed to execute {}:", inert.display())),
            "{}",
            denied.message
        );
    }

    /// `listFilesystemRecipes`: `.ts`/`.sh` entries with their
    /// source, other extensions skipped, names sorted.
    #[test]
    fn list_filesystem_recipes_filters_and_sorts() {
        let dir = scratch_store("listfs");
        let recipes = dir.join(".mulch/recipes");
        std::fs::create_dir_all(&recipes).expect("recipes dir");
        std::fs::write(recipes.join("zeta.sh"), "#!/bin/sh\n").expect("zeta");
        std::fs::write(recipes.join("alpha.ts"), "export default {}\n").expect("alpha");
        std::fs::write(recipes.join("notes.txt"), "not a recipe\n").expect("notes");

        let listed = list_filesystem_recipes(&dir);
        let names: Vec<(&str, RecipeSource)> = listed
            .iter()
            .map(|listing| (listing.name.as_str(), listing.source))
            .collect();
        assert_eq!(names, [
            ("alpha", RecipeSource::FilesystemTs),
            ("zeta", RecipeSource::FilesystemSh)
        ]);
        // No recipes dir at all: empty.
        let bare = scratch("listbare");
        assert!(list_filesystem_recipes(&bare).is_empty());
    }
}
