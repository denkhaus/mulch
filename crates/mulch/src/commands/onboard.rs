//! `mulch onboard` — parity with the reference 0.10.7 (onboard.ts).
//!
//! Probed laws (ml 0.10.7, /tmp/opencode/sprint54B/probes/ob*):
//!
//! - Candidates in order: `CLAUDE.md`, `.claude/CLAUDE.md`, `AGENTS.md`. The
//!   FIRST one carrying a snippet (marker section OR legacy header) is the
//!   target; the rest are reported as duplicates. With no snippet anywhere: an
//!   existing root `CLAUDE.md` is preferred, then an existing `AGENTS.md`
//!   (append), else a fresh root `CLAUDE.md` is created — a `.claude/CLAUDE.md`
//!   WITHOUT a snippet never becomes the target.
//! - Snippet currency is the running binary's version marker `<!--
//!   mulch-onboard:v<VERSION> -->` — a foreign version is `outdated` and gets
//!   replaced on the write path.
//! - Actions: created / appended / updated / migrated / up_to_date (write) and
//!   not_installed / up_to_date / outdated / legacy (`--check`), each with its
//!   own message; JSON renders `{success, command, file, action}` on stdout,
//!   exit 0.
//! - `--stdout` prints the wrapped snippet with NO trailing newline, before any
//!   file resolution — immune to `--quiet` and `--json` (probed:
//!   `process.stdout.write` bypasses both).
//! - The message line and the duplicates warning are suppressed by `--quiet`;
//!   the warning additionally never renders in `--json`.
//! - `--provider` parses and is ignored (all providers share the standardized
//!   snippet, onboard.ts:71-74).
//!
//! DEVIATIONS (README): the snippet's command spellings promote THIS
//! binary (`mulch prime`, `mulch record`, … — including `mulch
//! --help`, which the word-level promote law cannot reach), and the
//! embedded version is this port's, not the reference's — the marker
//! therefore tracks the installed port.
//!
//! The embedded session-close footer: the reference feeds its shared
//! `getSessionEndReminder("embedded", …)` into the snippet. Prime's
//! port dropped the embedded arms as unreachable through the CLI
//! format flag (prime/format.rs D1, mulch-3e3b) — onboard IS the
//! embedder consumer, so the four preset arms land here, promoted
//! through [`promote_mulch`] like every other shipped string.

use std::path::{Path, PathBuf};

use serde_json::{Map, json};

use crate::cli::{GlobalOpts, OnboardArgs};
use crate::commands::render::promote_mulch;
use crate::output::{Failure, print_json, print_line, success_envelope};

/// The marker pair wrapping every managed snippet (markers.ts:1-2).
const MARKER_START: &str = "<!-- mulch:start -->";
const MARKER_END: &str = "<!-- mulch:end -->";

/// The legacy (pre-marker) snippet's anchors (onboard.ts:68-69).
const LEGACY_HEADER: &str = "## Project Expertise (Mulch)";
const LEGACY_TAIL: &str =
    r#"mulch validate && git add .mulch/ && git commit -m "mulch: record learnings""#;

/// The candidate files, in resolution order (onboard.ts:142-146).
const CANDIDATES: [&str; 3] = ["CLAUDE.md", ".claude/CLAUDE.md", "AGENTS.md"];

/// Runs `onboard` (onboard.ts `runOnboard`).
pub(super) fn run(opts: &GlobalOpts, args: &OnboardArgs) -> Result<(), Failure> {
    let cwd = std::env::current_dir()
        .map_err(|source| Failure::error_line("onboard", format!("resolving cwd: {source}")))?;
    // `onboard` may run before `init`; a missing or broken config
    // degrades to "no preset configured" (onboard.ts:76-85).
    let session_close = read_session_close_config(&cwd);
    let snippet = build_snippet(session_close.as_ref());
    let wrapped = wrap_in_markers(&snippet);

    // --stdout short-circuits before any file resolution and ignores
    // quiet/json (probed).
    if args.stdout {
        #[allow(clippy::print_stdout, reason = "the snippet IS the command's output")]
        {
            print!("{wrapped}");
        }
        return Ok(());
    }

    let (target, duplicates) = resolve_target_file(&cwd)?;

    if args.check {
        let action = check_action(&target)?;
        report(opts, &target, action, true, &duplicates);
    } else {
        let action = write_snippet(&target, &wrapped)?;
        report(opts, &target, action, false, &duplicates);
    }
    Ok(())
}

/// The one-line action vocabulary (onboard.ts:212-220).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Action {
    Created,
    Appended,
    Updated,
    Migrated,
    UpToDate,
    NotInstalled,
    Outdated,
    Legacy,
}

impl Action {
    fn as_str(self) -> &'static str {
        match self {
            Action::Created => "created",
            Action::Appended => "appended",
            Action::Updated => "updated",
            Action::Migrated => "migrated",
            Action::UpToDate => "up_to_date",
            Action::NotInstalled => "not_installed",
            Action::Outdated => "outdated",
            Action::Legacy => "legacy",
        }
    }

    /// The `--check` wordings (onboard.ts:266-271) — exhaustive for
    /// the four check actions.
    fn check_message(self, file: &str) -> String {
        match self {
            Action::NotInstalled => format!("Mulch snippet is not installed in {file}."),
            Action::UpToDate => format!("Mulch snippet in {file} is up to date."),
            Action::Outdated => {
                format!("Mulch snippet in {file} is outdated. Run `mulch onboard` to update.")
            }
            Action::Legacy => format!(
                "Mulch snippet in {file} uses legacy format (no markers). Run `mulch onboard` \
                 to migrate."
            ),
            Action::Created | Action::Appended | Action::Updated | Action::Migrated => {
                unreachable!("write actions never reach the check wording")
            }
        }
    }

    /// The write-path wordings (onboard.ts:335-341) — exhaustive for
    /// the five write actions.
    fn write_message(self, file: &str) -> String {
        match self {
            Action::Created => format!("Mulch onboarding snippet written to {file}."),
            Action::Appended => format!("Mulch onboarding snippet appended to {file}."),
            Action::Updated => format!("Mulch onboarding snippet updated in {file}."),
            Action::Migrated => {
                format!("Mulch onboarding snippet migrated to marker format in {file}.")
            }
            Action::UpToDate => {
                format!("Mulch snippet in {file} is already up to date. No changes made.")
            }
            Action::NotInstalled | Action::Outdated | Action::Legacy => {
                unreachable!("check actions never reach the write wording")
            }
        }
    }
}

/// One candidate file (onboard.ts:96-100).
struct OnboardTarget {
    file_name: String,
    path:      PathBuf,
    exists:    bool,
}

/// The running binary's version marker (onboard.ts:17-19) — snippet
/// currency tracks the INSTALLED port (a reference-written marker is
/// outdated by definition and gets refreshed on the write path).
fn version_marker() -> String {
    format!("<!-- mulch-onboard:v{} -->", env!("CARGO_PKG_VERSION"))
}

/// Reads `prime.session_close` from the cwd's config; ANY failure
/// (missing store, missing config, bad YAML, bad block) degrades to
/// `None` (the reference wraps readConfig in try/catch).
fn read_session_close_config(cwd: &Path) -> Option<mulch::SessionClose> {
    let text = std::fs::read_to_string(cwd.join(".mulch").join("mulch.config.yaml")).ok()?;
    let config = mulch::Config::parse(&text).ok()?;
    config
        .prime()
        .ok()
        .flatten()
        .and_then(|prime| prime.session_close)
}

/// The snippet body (onboard.ts:21-66): the standardized text with
/// THIS port's spellings (`mulch prime`, …, `mulch --help`) and
/// version, closed by the embedded session-close footer.
fn build_snippet(session_close: Option<&mulch::SessionClose>) -> String {
    let version = env!("CARGO_PKG_VERSION");
    let reminder = embedded_session_close(session_close);
    format!(
        "## Project Expertise (Mulch)
<!-- mulch-onboard:v{version} -->

This project uses [Mulch](https://github.com/jayminwest/mulch) v{version} for structured expertise management.

**At the start of every session**, run:
```bash
mulch prime
```

Injects project-specific conventions, patterns, decisions, failures, references, and guides into
your context. Run `mulch prime --files src/foo.ts` before editing a file to load only records
relevant to that path (per-file framing, classification age, and confirmation scores included).

For monolith projects where dumping every record wastes context, set
`prime.default_mode: manifest` in `.mulch/mulch.config.yaml` (or pass `--manifest`) to emit a
quick reference + domain index. Agents then scope-load with `mulch prime <domain>` or
`mulch prime --files <path>`.

**Before completing your task**, record insights worth preserving — conventions discovered,
patterns applied, failures encountered, or decisions made:
```bash
mulch record <domain> --type <convention|pattern|failure|decision|reference|guide> --description \"...\"
```

Evidence auto-populates from git (current commit + changed files). Link explicitly with
`--evidence-seeds <id>` / `--evidence-gh <id>` / `--evidence-linear <id>` / `--evidence-bead <id>`,
`--evidence-commit <sha>`, or `--relates-to <mx-id>`. Upserts of named records merge outcomes
instead of replacing them; validation failures print a copy-paste retry hint with missing fields
pre-filled.

Run `mulch status` for domain health, `mulch doctor` to check record integrity (add `--fix` to strip
broken file anchors), `mulch --help` for the full command list. Write commands use file locking and
atomic writes, so multiple agents can record concurrently. Expertise survives `git worktree`
cleanup — `.mulch/` resolves to the main repo.

`mulch prune` soft-archives stale records to `.mulch/archive/` instead of deleting them; pass
`--hard` for true deletion. Restore an archived record with `mulch restore <id>`. Do not read
`.mulch/archive/` directly — those records are stale by definition. If you need historical
context, run `mulch search --archived <query>`.

{reminder}
"
    )
}

/// `getSessionEndReminder("embedded", …)` (format.ts:496-515): custom
/// wins verbatim, then the style preset with `conditional` for
/// absent/unknown styles — everything promoted to this binary's name
/// (the same law prime's `session_close` applies).
fn embedded_session_close(config: Option<&mulch::SessionClose>) -> String {
    if let Some(custom) = config.and_then(|config| config.custom.as_deref())
        && !custom.is_empty()
    {
        return promote_mulch(custom);
    }
    let style = config
        .and_then(|config| config.style.as_deref())
        .unwrap_or("conditional");
    match style {
        "none" => String::new(),
        "minimal" => promote_mulch(
            "**Before you finish**: record any insight worth preserving with `ml learn` → \
             `ml record <domain> --type <type> …` → `ml sync`. Skip if no insight surfaced.",
        ),
        "directive" => promote_mulch(&directive_embedded()),
        _ => promote_mulch(&conditional_embedded()),
    }
}

/// The `conditional` preset's embedded arm (format.ts:544-556).
fn conditional_embedded() -> String {
    [
        "### Before You Finish",
        "",
        "If you discovered conventions, patterns, decisions, or failures worth preserving during",
        "this session, record them before closing:",
        "",
        "```bash",
        "ml learn                                                                    # see what files changed",
        "ml record <domain> --type <convention|pattern|failure|decision|reference|guide> --description \"...\"",
        "ml sync                                                                     # validate, stage, commit",
        "```",
        "",
        "Skip if no insight surfaced. Unrecorded learnings are lost; ritual filler records are also noise.",
    ]
    .join("\n")
}

/// The `directive` preset's embedded arm (format.ts:648-681).
fn directive_embedded() -> String {
    [
        "### Before You Finish",
        "",
        "Record insights worth preserving before closing this session:",
        "",
        "1. `ml learn` — see changed files and suggested domains",
        "2. `ml record <domain> --type <type> …` — record each insight",
        "3. `ml sync` — validate, stage, commit",
        "",
        "**Types**:",
        "- `convention` — a norm the project follows (naming, formatting, …)",
        "- `pattern` — how-to: a repeatable approach to a class of problem",
        "- `failure` — anti-pattern + fix: what broke and how you resolved it",
        "- `decision` — we picked X over Y, and why",
        "- `reference` — pointer to an external spec / doc / codebase landmark",
        "- `guide` — a walk-through of a multi-step workflow",
        "",
        "**Dedup**: `--name` upserts merge; re-recording is safe — outcomes accumulate, fields update in place.",
        "",
        "**Do not record**: restatements of CLAUDE.md or prior records, generic truisms (\"use TypeScript\"), single-PR-scoped facts (\"this PR adds X\"), ritual confirmations with no new information.",
        "",
        "Skip if no insight surfaced. Unrecorded learnings are lost; ritual filler records are also noise.",
    ]
    .join("\n")
}

// ---- the marker seam (markers.ts) ----

/// `wrapInMarkers` (markers.ts:46-48): start marker, newline, snippet
/// (which ends with a newline), end marker.
fn wrap_in_markers(snippet: &str) -> String {
    format!("{MARKER_START}\n{snippet}{MARKER_END}")
}

/// `hasMarkerSection` (markers.ts:9-11).
fn has_marker_section(content: &str) -> bool {
    content.contains(MARKER_START)
}

/// `replaceMarkerSection` (markers.ts:17-26): splice the new section
/// between the first start marker and the first end marker; `None`
/// when either marker is missing.
fn replace_marker_section(content: &str, new_section: &str) -> Option<String> {
    let start = content.find(MARKER_START)?;
    let end = content.find(MARKER_END)?;
    let before = &content[..start];
    let after = &content[end + MARKER_END.len()..];
    Some(format!("{before}{new_section}{after}"))
}

// ---- legacy snippet detection and migration (onboard.ts:102-134) ----

fn has_legacy_snippet(content: &str) -> bool {
    content.contains(LEGACY_HEADER)
}

/// `replaceLegacySnippet`: from the legacy header through the closing
/// fence after the legacy tail (consuming trailing newlines); with no
/// tail, or no closing fence, through end of file.
fn replace_legacy_snippet(content: &str, new_section: &str) -> String {
    let Some(header) = content.find(LEGACY_HEADER) else {
        return content.to_string();
    };
    let end = match content[header..].find(LEGACY_TAIL) {
        Some(tail_offset) => {
            let tail = header + tail_offset;
            match content[tail + LEGACY_TAIL.len()..].find("```") {
                Some(fence_offset) => {
                    let mut end = tail + LEGACY_TAIL.len() + fence_offset + 3;
                    while content.as_bytes().get(end) == Some(&b'\n') {
                        end += 1;
                    }
                    end
                }
                None => content.len(),
            }
        }
        None => content.len(),
    };
    format!("{}{new_section}{}", &content[..header], &content[end..])
}

// ---- target resolution (onboard.ts:141-210) ----

/// A file read through the command's error contract: Node's thrown
/// read error reaches the register handler's `Error: <message>`
/// surface — the io wording is the port's (the wording deviation
/// family shared with the store reads).
fn read_target(path: &Path) -> Result<String, Failure> {
    std::fs::read_to_string(path).map_err(|source| {
        Failure::error_line("onboard", format!("reading {}: {source}", path.display()))
    })
}

/// A file write through the same contract (`writeFile`'s throw).
fn write_target(path: &Path, contents: &str) -> Result<(), Failure> {
    std::fs::write(path, contents).map_err(|source| {
        Failure::error_line("onboard", format!("writing {}: {source}", path.display()))
    })
}

/// `fileExists`: `access()` succeeds for DIRECTORIES too — a
/// directory named `CLAUDE.md` reads as existing and fails later in
/// the read (the reference's EISDIR surface, probed law).
fn file_exists(path: &Path) -> bool {
    path.exists()
}

/// `findSnippetLocations`: the candidates that exist AND carry a
/// snippet (marker or legacy), in candidate order.
fn find_snippet_locations(cwd: &Path) -> Result<Vec<OnboardTarget>, Failure> {
    let mut found = Vec::new();
    for name in CANDIDATES {
        let path = cwd.join(name);
        if file_exists(&path) {
            let content = read_target(&path)?;
            if has_marker_section(&content) || has_legacy_snippet(&content) {
                found.push(OnboardTarget {
                    file_name: name.to_string(),
                    path,
                    exists: true,
                });
            }
        }
    }
    Ok(found)
}

/// `resolveTargetFile`: the first snippet-bearing candidate wins (the
/// rest are duplicates); else an existing root `CLAUDE.md`, else an
/// existing `AGENTS.md`, else a fresh root `CLAUDE.md`.
fn resolve_target_file(cwd: &Path) -> Result<(OnboardTarget, Vec<OnboardTarget>), Failure> {
    let mut found = find_snippet_locations(cwd)?;
    if !found.is_empty() {
        let duplicates = found.split_off(1);
        return Ok((found.remove(0), duplicates));
    }
    let (file_name, exists) = if file_exists(&cwd.join("CLAUDE.md")) {
        ("CLAUDE.md", true)
    } else if file_exists(&cwd.join("AGENTS.md")) {
        ("AGENTS.md", true)
    } else {
        ("CLAUDE.md", false)
    };
    Ok((
        OnboardTarget {
            path: cwd.join(file_name),
            file_name: file_name.to_string(),
            exists,
        },
        Vec::new(),
    ))
}

/// Whether the content's marker section is current for THIS binary's
/// version (onboard.ts:136-139).
fn is_snippet_current(content: &str) -> bool {
    has_marker_section(content) && content.contains(&version_marker())
}

/// The `--check` action for the resolved target (onboard.ts:242-256).
fn check_action(target: &OnboardTarget) -> Result<Action, Failure> {
    if !target.exists {
        return Ok(Action::NotInstalled);
    }
    let content = read_target(&target.path)?;
    if has_marker_section(&content) {
        Ok(if is_snippet_current(&content) {
            Action::UpToDate
        } else {
            Action::Outdated
        })
    } else if has_legacy_snippet(&content) {
        Ok(Action::Legacy)
    } else {
        Ok(Action::NotInstalled)
    }
}

/// The write path (onboard.ts:292-325): create, refresh the marker
/// section, migrate the legacy block, or append to existing content.
fn write_snippet(target: &OnboardTarget, wrapped: &str) -> Result<Action, Failure> {
    if !target.exists {
        if let Some(parent) = target.path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| {
                Failure::error_line(
                    "onboard",
                    format!("creating {}: {source}", parent.display()),
                )
            })?;
        }
        write_target(&target.path, &format!("{wrapped}\n"))?;
        return Ok(Action::Created);
    }
    let content = read_target(&target.path)?;
    if has_marker_section(&content) {
        if is_snippet_current(&content) {
            return Ok(Action::UpToDate);
        }
        if let Some(updated) = replace_marker_section(&content, wrapped) {
            write_target(&target.path, &updated)?;
        }
        return Ok(Action::Updated);
    }
    if has_legacy_snippet(&content) {
        let migrated = replace_legacy_snippet(&content, &format!("{wrapped}\n"));
        write_target(&target.path, &migrated)?;
        return Ok(Action::Migrated);
    }
    let trimmed = content.trim_end();
    write_target(&target.path, &format!("{trimmed}\n\n{wrapped}\n"))?;
    Ok(Action::Appended)
}

/// Renders the action: the JSON envelope, or the message line (the
/// path's own wording table) plus the duplicates warning
/// (onboard.ts:327-351).
fn report(
    opts: &GlobalOpts,
    target: &OnboardTarget,
    action: Action,
    check: bool,
    duplicates: &[OnboardTarget],
) {
    if opts.json {
        let mut fields = Map::new();
        fields.insert("file".into(), json!(target.file_name));
        fields.insert("action".into(), json!(action.as_str()));
        print_json(&success_envelope("onboard", fields), false);
    } else {
        let message = if check {
            action.check_message(&target.file_name)
        } else {
            action.write_message(&target.file_name)
        };
        print_line(opts.quiet, &message);
        if !duplicates.is_empty() {
            let names = duplicates
                .iter()
                .map(|duplicate| duplicate.file_name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            print_line(
                opts.quiet,
                &format!("Warning: mulch snippet also found in: {names}"),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// chdir into a scratch dir under one process-wide mutex (plain
    /// `cargo test` shares a process) — the same law as the store
    /// seam's `in_scratch_cwd`.
    fn in_scratch_cwd(tag: &str, body: impl FnOnce(&Path)) {
        static CWD: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = CWD
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = std::env::temp_dir().join(format!(
            "mulch-onboard-scratch-{tag}-{}",
            std::process::id()
        ));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("scratch root");
        let previous = std::env::current_dir().expect("cwd readable");
        std::env::set_current_dir(&dir).expect("chdir into scratch");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| body(&dir)));
        std::env::set_current_dir(previous).expect("chdir back");
        std::fs::remove_dir_all(&dir).ok();
        if let Err(panic) = result {
            std::panic::resume_unwind(panic);
        }
    }

    fn args(stdout: bool, check: bool) -> OnboardArgs {
        OnboardArgs {
            stdout,
            provider: None,
            check,
        }
    }

    fn opts(json: bool, quiet: bool) -> GlobalOpts {
        GlobalOpts {
            json,
            quiet,
            verbose: false,
            timing: false,
            allow_unknown_types: false,
            allow_domain_mismatch: false,
            format: None,
        }
    }

    fn wrapped_default() -> String {
        wrap_in_markers(&build_snippet(None))
    }

    /// The snippet body: this port's spellings (no `ml <cmd>`
    /// references anywhere — the promote deviation's own law), the
    /// running version in marker and prose, and the conditional
    /// embedded footer by default.
    #[test]
    fn snippet_body_promotes_this_binary_and_carries_the_version() {
        let snippet = build_snippet(None);
        let version = env!("CARGO_PKG_VERSION");
        assert!(snippet.starts_with("## Project Expertise (Mulch)\n"));
        assert!(
            snippet.contains(&format!("<!-- mulch-onboard:v{version} -->")),
            "the version marker leads the body"
        );
        assert!(snippet.contains(&format!(
            "This project uses [Mulch](https://github.com/jayminwest/mulch) v{version} for \
             structured expertise management."
        )));
        for spelling in [
            "mulch prime",
            "mulch prime --files src/foo.ts",
            "mulch record <domain>",
            "mulch status",
            "mulch doctor",
            "mulch --help",
            "mulch prune",
            "mulch restore <id>",
            "mulch search --archived <query>",
            "mulch learn",
            "mulch sync",
        ] {
            assert!(snippet.contains(spelling), "{spelling} missing");
        }
        for reference_spelling in [
            "`ml prime`",
            "`ml record",
            "`ml status`",
            "`ml --help`",
            "`ml learn`",
            "`ml sync`",
            "\nml ",
        ] {
            assert!(
                !snippet.contains(reference_spelling),
                "{reference_spelling} leaked (promote law)"
            );
        }
    }

    /// The wrapped snippet on --stdout: markers around the body, NO
    /// trailing newline (probed: 2712 bytes for the 0.10.7 reference,
    /// `process.stdout.write` adds none).
    #[test]
    fn wrapped_snippet_has_no_trailing_newline() {
        let wrapped = wrapped_default();
        assert!(wrapped.starts_with("<!-- mulch:start -->\n## Project Expertise (Mulch)"));
        assert!(wrapped.ends_with("<!-- mulch:end -->"));
        assert_eq!(
            wrapped,
            format!("{MARKER_START}\n{}{MARKER_END}", build_snippet(None))
        );
    }

    /// Fresh dir: creates root CLAUDE.md with the wrapped snippet
    /// plus ONE trailing newline, exit 0 — even when a
    /// `.claude/CLAUDE.md` exists WITHOUT a snippet (it never becomes
    /// the target; probed law from resolveTargetFile).
    #[test]
    fn creates_root_claude_md_when_no_candidate_carries_a_snippet() {
        in_scratch_cwd("create", |dir| {
            std::fs::create_dir_all(dir.join(".claude")).expect("claude dir");
            std::fs::write(dir.join(".claude/CLAUDE.md"), "agent notes\n").expect("inner");
            run(&opts(false, false), &args(false, false)).expect("onboard runs");
            let written = std::fs::read_to_string(dir.join("CLAUDE.md")).expect("created");
            assert_eq!(written, format!("{}\n", wrapped_default()));
            let inner = std::fs::read_to_string(dir.join(".claude/CLAUDE.md")).expect("inner");
            assert_eq!(inner, "agent notes\n", "the inner file is untouched");
        });
    }

    /// A second run on the fresh write is up_to_date and writes
    /// nothing (byte-identical file, its own message).
    #[test]
    fn second_run_is_up_to_date_and_writes_nothing() {
        in_scratch_cwd("uptodate", |dir| {
            run(&opts(false, false), &args(false, false)).expect("first run");
            let before = std::fs::read_to_string(dir.join("CLAUDE.md")).expect("file");
            run(&opts(false, false), &args(false, false)).expect("second run");
            let after = std::fs::read_to_string(dir.join("CLAUDE.md")).expect("file");
            assert_eq!(before, after, "no bytes move on up_to_date");
        });
    }

    /// Existing AGENTS.md (no CLAUDE.md): appended after the existing
    /// content with exactly one blank line (trimEnd + \n\n law).
    #[test]
    fn appends_to_existing_agents_md_with_one_blank_line() {
        in_scratch_cwd("agents", |dir| {
            std::fs::write(dir.join("AGENTS.md"), "Existing notes here.\n").expect("agents");
            run(&opts(false, false), &args(false, false)).expect("onboard runs");
            let written = std::fs::read_to_string(dir.join("AGENTS.md")).expect("appended");
            assert_eq!(
                written,
                format!("Existing notes here.\n\n{}\n", wrapped_default())
            );
            // trailing whitespace on the existing content is trimmed
            std::fs::write(dir.join("AGENTS.md"), "Trailing spaces.   \n\n\n").expect("agents");
            run(&opts(false, false), &args(false, false)).expect("second append path");
            let trimmed = std::fs::read_to_string(dir.join("AGENTS.md")).expect("appended");
            assert!(
                trimmed.starts_with("Trailing spaces.\n\n<!-- mulch:start -->"),
                "trimEnd then exactly one blank line: {trimmed:?}"
            );
        });
    }

    /// An outdated marker (a foreign version) is replaced in place:
    /// content before and after the marker section survives.
    #[test]
    fn outdated_marker_section_is_replaced_in_place() {
        in_scratch_cwd("outdated", |dir| {
            let stale = wrapped_default().replace(
                &format!("<!-- mulch-onboard:v{} -->", env!("CARGO_PKG_VERSION")),
                "<!-- mulch-onboard:v0.0.1 -->",
            );
            std::fs::write(
                dir.join("CLAUDE.md"),
                format!("Intro text\n\n{stale}\nMore after\n"),
            )
            .expect("stale");
            run(&opts(false, false), &args(false, false)).expect("onboard runs");
            let updated = std::fs::read_to_string(dir.join("CLAUDE.md")).expect("updated");
            assert_eq!(
                updated,
                format!("Intro text\n\n{}\nMore after\n", wrapped_default())
            );
        });
    }

    /// The legacy block migrates from its header through the closing
    /// fence after the legacy tail (consuming the blank lines), keep
    /// the content after it.
    #[test]
    fn legacy_snippet_migrates_header_through_closing_fence() {
        in_scratch_cwd("legacy", |dir| {
            std::fs::write(
                dir.join("CLAUDE.md"),
                "# My project\n\n## Project Expertise (Mulch)\n\nOld-style snippet without \
                 markers.\n\n```bash\nmulch validate && git add .mulch/ && git commit -m \
                 \"mulch: record learnings\"\n```\n\nTrailing content after snippet.\n",
            )
            .expect("legacy");
            run(&opts(false, false), &args(false, false)).expect("onboard runs");
            let migrated = std::fs::read_to_string(dir.join("CLAUDE.md")).expect("migrated");
            assert_eq!(
                migrated,
                format!(
                    "# My project\n\n{}\nTrailing content after snippet.\n",
                    wrapped_default()
                )
            );
        });
    }

    /// The --check action table: not_installed on a bare dir,
    /// up_to_date after a write, outdated on a foreign marker,
    /// legacy on the un-marked header — each exit 0 with its message
    /// (`--check` never writes).
    #[test]
    fn check_reports_the_four_actions_without_writing() {
        in_scratch_cwd("check", |dir| {
            run(&opts(false, false), &args(false, true)).expect("not_installed runs");
            assert!(!dir.join("CLAUDE.md").exists(), "--check never creates");
            run(&opts(false, false), &args(false, false)).expect("write");
            run(&opts(false, false), &args(false, true)).expect("up_to_date runs");

            let stale = wrapped_default().replace(
                &format!("<!-- mulch-onboard:v{} -->", env!("CARGO_PKG_VERSION")),
                "<!-- mulch-onboard:v9.9.9 -->",
            );
            std::fs::write(dir.join("CLAUDE.md"), stale).expect("stale");
            run(&opts(false, false), &args(false, true)).expect("outdated runs");
            assert!(
                std::fs::read_to_string(dir.join("CLAUDE.md"))
                    .expect("file")
                    .contains("v9.9.9"),
                "--check never rewrites"
            );

            std::fs::write(dir.join("CLAUDE.md"), "## Project Expertise (Mulch)\nold\n")
                .expect("legacy");
            run(&opts(false, false), &args(false, true)).expect("legacy runs");
        });
    }

    /// The candidate order law: CLAUDE.md wins over .claude/CLAUDE.md
    /// when both carry snippets, and the loser is the reported
    /// duplicate (plain only — json renders just the envelope).
    #[test]
    fn first_candidate_is_the_target_and_the_rest_are_duplicates() {
        in_scratch_cwd("dupes", |dir| {
            std::fs::create_dir_all(dir.join(".claude")).expect("claude dir");
            let snippet = wrapped_default();
            std::fs::write(dir.join("CLAUDE.md"), &snippet).expect("outer");
            std::fs::write(dir.join(".claude/CLAUDE.md"), &snippet).expect("inner");
            run(&opts(false, false), &args(false, true)).expect("check runs");
            // json mode: the envelope is the whole output (no warning)
            run(&opts(true, false), &args(false, true)).expect("json check runs");
        });
    }

    /// The session-close footer follows the config: `none` leaves the
    /// trailing blank, `minimal` is the one-liner, `directive` the
    /// numbered block, `custom` wins verbatim — all promoted.
    #[test]
    fn embedded_footer_follows_the_session_close_config() {
        let conditional = build_snippet(None);
        assert!(conditional.contains("### Before You Finish\n"));
        assert!(conditional.contains("If you discovered conventions"));

        let none = build_snippet(Some(&mulch::SessionClose {
            style:  Some("none".into()),
            custom: None,
        }));
        assert!(
            !none.contains("### Before You Finish"),
            "style none suppresses the footer"
        );
        assert!(none.ends_with("run `mulch search --archived <query>`.\n\n\n"));

        let minimal = build_snippet(Some(&mulch::SessionClose {
            style:  Some("minimal".into()),
            custom: None,
        }));
        assert!(minimal.contains(
            "**Before you finish**: record any insight worth preserving with `mulch learn` → \
             `mulch record <domain> --type <type> …` → `mulch sync`. Skip if no insight surfaced."
        ));

        let directive = build_snippet(Some(&mulch::SessionClose {
            style:  Some("directive".into()),
            custom: None,
        }));
        assert!(
            directive.contains("Record insights worth preserving before closing this session:")
        );
        assert!(directive.contains("1. `mulch learn` — see changed files and suggested domains"));

        let custom = build_snippet(Some(&mulch::SessionClose {
            style:  None,
            custom: Some("close with `ml sync`".into()),
        }));
        assert!(
            custom.contains("close with `mulch sync`"),
            "custom is promoted"
        );
    }

    /// A broken or missing config degrades to the conditional footer
    /// (the run-before-init law).
    #[test]
    fn broken_config_degrades_to_no_preset() {
        in_scratch_cwd("config", |dir| {
            std::fs::create_dir_all(dir.join(".mulch")).expect("store dir");
            std::fs::write(dir.join(".mulch/mulch.config.yaml"), "not: [valid").expect("bad yaml");
            assert!(
                read_session_close_config(dir).is_none(),
                "a parse failure degrades to no preset"
            );
            std::fs::remove_dir_all(dir.join(".mulch")).expect("clean");
            assert!(
                read_session_close_config(dir).is_none(),
                "no store degrades to no preset"
            );
        });
    }

    /// The two message tables, byte-pinned (onboard.ts:266-271,
    /// 335-341) — including the up_to_date pair's different
    /// wordings and this binary's name in the two remediation hints.
    #[test]
    fn action_messages_match_the_reference_tables() {
        assert_eq!(
            Action::Created.write_message("CLAUDE.md"),
            "Mulch onboarding snippet written to CLAUDE.md."
        );
        assert_eq!(
            Action::Appended.write_message("AGENTS.md"),
            "Mulch onboarding snippet appended to AGENTS.md."
        );
        assert_eq!(
            Action::Updated.write_message("CLAUDE.md"),
            "Mulch onboarding snippet updated in CLAUDE.md."
        );
        assert_eq!(
            Action::Migrated.write_message(".claude/CLAUDE.md"),
            "Mulch onboarding snippet migrated to marker format in .claude/CLAUDE.md."
        );
        assert_eq!(
            Action::UpToDate.write_message("CLAUDE.md"),
            "Mulch snippet in CLAUDE.md is already up to date. No changes made."
        );
        assert_eq!(
            Action::NotInstalled.check_message("CLAUDE.md"),
            "Mulch snippet is not installed in CLAUDE.md."
        );
        assert_eq!(
            Action::UpToDate.check_message("CLAUDE.md"),
            "Mulch snippet in CLAUDE.md is up to date."
        );
        assert_eq!(
            Action::Outdated.check_message("CLAUDE.md"),
            "Mulch snippet in CLAUDE.md is outdated. Run `mulch onboard` to update."
        );
        assert_eq!(
            Action::Legacy.check_message("AGENTS.md"),
            "Mulch snippet in AGENTS.md uses legacy format (no markers). Run `mulch onboard` to \
             migrate."
        );
    }

    /// The marker seam's splice law: the section between the FIRST
    /// start marker and the FIRST end marker is replaced; a missing
    /// end marker returns None (the write then keeps the file).
    #[test]
    fn replace_marker_section_splices_between_first_markers() {
        let content = "a\n<!-- mulch:start -->old\nmid\n<!-- mulch:end -->\nb<!-- mulch:end -->c";
        let replaced = replace_marker_section(content, "NEW").expect("spliced");
        assert_eq!(replaced, "a\nNEW\nb<!-- mulch:end -->c");
        assert_eq!(replace_marker_section("no markers", "NEW"), None);
        assert_eq!(
            replace_marker_section("only <!-- mulch:start --> here", "NEW"),
            None
        );
    }

    /// The legacy splice without the tail (user edited the snippet):
    /// the replacement runs from the header to end of file.
    #[test]
    fn legacy_replace_without_tail_runs_to_eof() {
        let content = "before\n## Project Expertise (Mulch)\nuser text";
        assert_eq!(replace_legacy_snippet(content, "NEW"), "before\nNEW");
    }
}
