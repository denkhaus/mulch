//! `mulch diff` — expertise record changes since a git ref
//! (mulch-176f). Reference: commands/diff.ts (ml 0.10.7), probed live
//! 2026-10-26 (/tmp/opencode/sprint54A). Named `diff_cmd` after the
//! `move_cmd` precedent — a module named after its verb, avoiding a
//! bare `diff` path that reads like a diff-library module.
//!
//! Pipeline (diff.ts:105-200): the store-registry bootstrap (every
//! command builds the registry from the config at startup — a broken
//! `custom_types` fails globally, cli.ts:47-67), the git-repo gate
//! (plain and json carry DIFFERENT sentences, like `learn`), then
//! `git diff <since> -- .mulch/expertise/` under the read law (ANY
//! failure reads as an empty diff — a missing ref is "no changes",
//! exit 0), the header/±line parse, and the two renderers. The plain
//! renderer computes per-record summaries — an unknown type THROWS
//! (`Unknown record type: …`, format.ts:16-19) after the entry header
//! already printed; the json renderer never touches summaries, so the
//! same store succeeds with `--json` (probe-pinned both ways).
//!
//! Quiet (diff.ts:173-189): the HEADER and the no-changes line print
//! regardless; only the per-entry lines are `isQuiet()`-gated
//! (probe-pinned: `diff -q` on a dirty store prints exactly the
//! header).

// 176f prep: the command lands ahead of its dispatch wiring — the
// host applies the cli.rs/mod.rs patch (see /tmp/sprint54/A).
#![allow(
    dead_code,
    reason = "176f prep: dispatch wiring lands with the host patch"
)]

use serde_json::{Map, Value};

use crate::cli::{DiffArgs, GlobalOpts};
use crate::commands::failure::summary_text;
use crate::output::{Failure, print_json, print_line, success_envelope};

/// One domain's diff entry (diff.ts:10-14): the FULL parsed records,
/// not projections — the json envelope ships them verbatim.
#[derive(Debug)]
pub(crate) struct DiffEntry {
    /// The domain whose `<domain>.jsonl` changed.
    pub(crate) domain:  String,
    /// Records on `+` lines (a modified record appears here AND in
    /// `removed` — the line diff is the whole law).
    pub(crate) added:   Vec<Value>,
    /// Records on `-` lines.
    pub(crate) removed: Vec<Value>,
}

/// Runs `diff` (plain, `--json`, `-q`).
pub(crate) fn run(opts: &GlobalOpts, args: &DiffArgs) -> Result<(), Failure> {
    let cwd = std::env::current_dir()
        .map_err(|source| Failure::handled("diff", format!("resolving cwd: {source}")))?;

    // The registry bootstrap (every command, cli.ts:47-67): the store
    // opens for its registry; a missing/unreadable store falls back to
    // builtins (init.ts:47-53); a broken custom_types block fails the
    // GLOBAL config-error surface before diff's own logic.
    let registry = match mulch::StoreFiles::locate(&cwd) {
        Ok(mulch::StoreLocation::Open(store)) => store.registry().clone(),
        Ok(mulch::StoreLocation::Missing | mulch::StoreLocation::NoConfig) => {
            mulch::Registry::builtins()
        }
        Err(mulch::Error::CustomTypes { message }) => {
            return Err(Failure::config_error(&message));
        }
        Err(_) => mulch::Registry::builtins(),
    };

    if !crate::commands::git::is_git_repo(&cwd) {
        // The two surfaces carry DIFFERENT sentences (diff.ts:114-125):
        // plain is the lowercase `Error:` line, json the remediation
        // sentence in a stderr envelope — the same pair as `learn`
        // (the third inline copy; a fold candidate, see the delivery
        // note).
        let mut failure = Failure::handled_on_stderr("diff", "Error: not in a git repository.");
        if opts.json
            && let Some(body) = failure.envelope.as_object_mut()
        {
            body.insert(
                "error".into(),
                Value::String(
                    "Not in a git repository. Run this command from within a git repository."
                        .into(),
                ),
            );
        }
        return Err(failure);
    }

    // The read law: `git diff <since> -- .mulch/expertise/`; ANY
    // failure (missing ref, spawn error) reads as the empty diff
    // (diff.ts:128-138).
    let diff_output = crate::commands::git::run(&cwd, &[
        "diff",
        args.since.as_str(),
        "--",
        ".mulch/expertise/",
    ])
    .unwrap_or_default();

    let entries = parse_expertise_diff(&diff_output);
    if entries.is_empty() {
        if opts.json {
            let mut fields = Map::new();
            fields.insert("since".into(), Value::String(args.since.clone()));
            fields.insert("domains".into(), Value::Array(Vec::new()));
            fields.insert(
                "message".into(),
                Value::String("No expertise changes found.".into()),
            );
            print_json(&success_envelope("diff", fields), false);
        } else {
            // Not quiet-gated (diff.ts:152) — probed: -q still prints it.
            print_line(false, "No expertise changes found.");
        }
        return Ok(());
    }

    if opts.json {
        // The json renderer ships the full records and never computes
        // summaries — an unknown type here is NOT an error.
        let mut fields = Map::new();
        fields.insert("since".into(), Value::String(args.since.clone()));
        fields.insert("domains".into(), domains_value(&entries));
        print_json(&success_envelope("diff", fields), false);
        return Ok(());
    }

    // The header prints BEFORE the quiet gate (diff.ts:168): the
    // template's own newlines plus console.log's give exactly
    // `\n…\n\n` (probe-pinned bytes).
    #[allow(clippy::print_stdout, reason = "rendering is this command's job")]
    {
        println!("\nExpertise changes since {}\n", args.since);
    }
    if !opts.quiet {
        for entry in &entries {
            print_line(
                false,
                &format!(
                    "{} ({} {}):",
                    entry.domain,
                    entry.added.len() + entry.removed.len(),
                    if entry.added.len() + entry.removed.len() == 1 {
                        "change"
                    } else {
                        "changes"
                    }
                ),
            );
            for record in &entry.added {
                let summary = record_summary(&registry, record)?;
                print_line(false, &format!("  + {} {summary}", record_line(record)));
            }
            for record in &entry.removed {
                let summary = record_summary(&registry, record)?;
                print_line(false, &format!("  - {} {summary}", record_line(record)));
            }
            print_line(false, "");
        }
    }
    Ok(())
}

/// The `  + [type] <id> <summary>` record line head: the type
/// bracket, the id ("" when absent — the reference's `record.id ?? ""`
/// leaves the double space, probe-pinned) and the summary.
fn record_line(record: &Value) -> String {
    let record_type = record
        .get("type")
        .map_or_else(|| "undefined".into(), mulch::value_text);
    let id = match record.get("id") {
        None | Some(Value::Null) => String::new(),
        Some(value) => mulch::value_text(value),
    };
    format!("[{record_type}] {id}")
}

/// getRecordSummary for the plain renderer (format.ts:14-20): an
/// unregistered type THROWS — here the handled `Error:` failure (the
/// entry header above already printed, matching the reference's
/// mid-render crash point). The registry lookup is the RAW type: a
/// MISSING type is `Map.get(undefined)` — a miss that throws on the
/// literal `undefined`, NOT the convention fallback the strict
/// readers use.
fn record_summary(registry: &mulch::Registry, record: &Value) -> Result<String, Failure> {
    let raw_type = record.get("type");
    let shown = raw_type.map_or_else(|| "undefined".into(), mulch::value_text);
    if !matches!(
        raw_type.and_then(Value::as_str),
        Some(name) if registry.row(name).is_some()
    ) {
        return Err(Failure::error_line(
            "diff",
            format!("Unknown record type: {shown}"),
        ));
    }
    mulch::record_summary_value(registry, record)
        .map(|value| summary_text(value.as_ref()))
        .map_err(|source| Failure::error_line("diff", source))
}

/// The json envelope's `domains` array: `{domain, added, removed}` in
/// that key order, records verbatim.
fn domains_value(entries: &[DiffEntry]) -> Value {
    Value::Array(
        entries
            .iter()
            .map(|entry| {
                let mut object = Map::new();
                object.insert("domain".into(), Value::String(entry.domain.clone()));
                object.insert("added".into(), Value::Array(entry.added.clone()));
                object.insert("removed".into(), Value::Array(entry.removed.clone()));
                Value::Object(object)
            })
            .collect(),
    )
}

/// parseExpertiseDiff (diff.ts:16-77): `diff --git` headers set the
/// current domain (the FIRST `.mulch/expertise/<name>.jsonl` path in
/// the line — a rename attributes to the OLD name), `+++`/`---`/`@@`
/// metadata lines skip, and `+`/`-` body lines become records when
/// they parse as JSON (anything else — context lines, comments —
/// skips silently). Entries with neither adds nor removes drop;
/// survivors sort by domain.
pub(crate) fn parse_expertise_diff(diff_output: &str) -> Vec<DiffEntry> {
    let mut order: Vec<String> = Vec::new();
    let mut entries: Vec<DiffEntry> = Vec::new();
    let mut current: Option<usize> = None;

    for line in diff_output.split('\n') {
        if line.starts_with("diff --git") {
            current = domain_from_header(line).map(|domain| {
                if let Some(index) = order.iter().position(|seen| seen == &domain) {
                    index
                } else {
                    order.push(domain.clone());
                    entries.push(DiffEntry {
                        domain,
                        added: Vec::new(),
                        removed: Vec::new(),
                    });
                    entries.len() - 1
                }
            });
            continue;
        }
        if line.starts_with("+++") || line.starts_with("---") || line.starts_with("@@") {
            continue;
        }
        if let Some((added, body)) = line.as_bytes().first().and_then(|mark| match mark {
            b'+' => Some((true, &line[1..])),
            b'-' => Some((false, &line[1..])),
            _ => None,
        }) && let Some(index) = current
        {
            // JSON.parse failure skips the line (context noise,
            // comments, any non-JSON body).
            if let Ok(record) = serde_json::from_str::<Value>(body) {
                if added {
                    entries[index].added.push(record);
                } else {
                    entries[index].removed.push(record);
                }
            }
        }
    }

    let mut kept: Vec<DiffEntry> = entries
        .into_iter()
        .filter(|entry| !entry.added.is_empty() || !entry.removed.is_empty())
        .collect();
    // DEVIATION (README): the reference sorts with localeCompare, we
    // by bytes — identical for same-case domain names (the probe set:
    // `_`-leading, embedded hyphens), observably different only for
    // MIXED case (`Beta` sorts before `asset` here, after in the
    // reference's collation). Byte order is environment-independent;
    // ICU collation is not worth the dependency.
    kept.sort_by(|a, b| a.domain.cmp(&b.domain));
    kept
}

/// The `.mulch/expertise/<domain>.jsonl` capture of a `diff --git`
/// header: the FIRST path in the line whose segment after
/// `.mulch/expertise/` ends `.jsonl` with a non-empty stem. The JS
/// regex's greedy `[^/]+` backtracks to the LAST `.jsonl` in the
/// slash-delimited segment (a header's trailing ` b/…` text can share
/// the segment), and subdirectory files never match — `[^/]+` cannot
/// cross a slash.
fn domain_from_header(line: &str) -> Option<String> {
    const MARKER: &str = ".mulch/expertise/";
    const SUFFIX: &str = ".jsonl";
    let mut search = 0;
    while let Some(found) = line[search..].find(MARKER) {
        let start = search + found + MARKER.len();
        let segment = line[start..].split('/').next().unwrap_or_default();
        if let Some(index) = segment.rfind(SUFFIX)
            && !segment[..index].is_empty()
        {
            return Some(segment[..index].to_string());
        }
        search = start;
    }
    None
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static SERIAL: AtomicU64 = AtomicU64::new(0);

    /// One unique temp dir per call.
    fn scratch_dir(tag: &str) -> PathBuf {
        let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "mulch-diff-cmd-{tag}-{}-{serial}",
            std::process::id()
        ));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("scratch root");
        dir
    }

    /// Fixture git: repo-local config kills hooks and signing so the
    /// machine's global config cannot break the fixture.
    fn fixture_git(cwd: &Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .args([
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(cwd)
            .output()
            .expect("fixture git spawns");
        assert!(
            output.status.success(),
            "fixture git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// A scratch git repo with a `.mulch` store committed at c0.
    fn scratch_repo(tag: &str) -> PathBuf {
        let dir = scratch_dir(tag);
        std::fs::create_dir_all(dir.join(".mulch/expertise")).expect("store tree");
        std::fs::write(
            dir.join(".mulch/mulch.config.yaml"),
            "version: '1'\ndomains:\n  dev: {}\n",
        )
        .expect("config");
        fixture_git(&dir, &["init", "-q"]);
        fixture_git(&dir, &["config", "user.email", "t@example.io"]);
        fixture_git(&dir, &["config", "user.name", "T"]);
        fixture_git(&dir, &["add", "-A"]);
        fixture_git(&dir, &["commit", "-qm", "c0"]);
        dir
    }

    /// Tracks a store-relative write and commits it.
    fn commit_file(repo: &Path, relative: &str, body: &str, message: &str) {
        std::fs::write(repo.join(relative), body).expect("scratch file");
        fixture_git(repo, &["add", relative]);
        fixture_git(repo, &["commit", "-qm", message]);
    }

    /// The parse law over the probed diff shapes: added and removed
    /// records land per domain; `+++`/`---`/`@@` metadata and
    /// non-JSON body lines (comments, context) skip; empty entries
    /// drop; survivors sort by domain (`_under` before `amid` before
    /// `zdev` — probe-pinned); a `+` line before ANY header never
    /// lands.
    #[test]
    fn parse_law_matches_the_probed_diff_shapes() {
        let diff = "diff --git a/.mulch/expertise/dev.jsonl b/.mulch/expertise/dev.jsonl\n\
                    index 111..222 100644\n\
                    --- a/.mulch/expertise/dev.jsonl\n\
                    +++ b/.mulch/expertise/dev.jsonl\n\
                    @@ -1 +1,2 @@\n\
                    +# a comment line\n\
                    +{\"type\":\"pattern\",\"id\":\"mx-1\",\"name\":\"P\"}\n\
                    -{\"type\":\"convention\",\"id\":\"mx-2\",\"content\":\"gone\"}\n\
                    +plain context noise that is not json\n\
                    diff --git a/.mulch/expertise/empty.jsonl b/.mulch/expertise/empty.jsonl\n\
                    +++ b/.mulch/expertise/empty.jsonl\n\
                    @@ -0,0 +1 @@\n\
                    +# only a comment here\n";
        let entries = parse_expertise_diff(diff);
        assert_eq!(entries.len(), 1, "the comment-only domain drops");
        assert_eq!(entries[0].domain, "dev");
        assert_eq!(entries[0].added.len(), 1);
        assert_eq!(entries[0].added[0]["id"], "mx-1");
        assert_eq!(entries[0].removed.len(), 1);
        assert_eq!(entries[0].removed[0]["id"], "mx-2");

        // sort + first-match header law (probe4's three-domain shape)
        let multi = "diff --git a/.mulch/expertise/zdev.jsonl b/.mulch/expertise/zdev.jsonl\n\
                     +{\"type\":\"pattern\",\"id\":\"mx-z\"}\n\
                     diff --git a/.mulch/expertise/amid.jsonl b/.mulch/expertise/amid.jsonl\n\
                     +{\"type\":\"pattern\",\"id\":\"mx-m\"}\n\
                     diff --git a/.mulch/expertise/_under.jsonl b/.mulch/expertise/_under.jsonl\n\
                     +{\"type\":\"pattern\",\"id\":\"mx-u\"}\n";
        let entries = parse_expertise_diff(multi);
        let domains: Vec<&str> = entries.iter().map(|e| e.domain.as_str()).collect();
        assert_eq!(domains, ["_under", "amid", "zdev"]);

        // a rename attributes to the FIRST path's domain (old name)
        let rename = "diff --git a/.mulch/expertise/old.jsonl b/.mulch/expertise/new.jsonl\n\
                      +{\"type\":\"pattern\",\"id\":\"mx-r\"}\n";
        let entries = parse_expertise_diff(rename);
        assert_eq!(entries[0].domain, "old");

        // subdirectory files never match; a stray + line without a
        // header never lands
        let none = "diff --git a/.mulch/expertise/sub/inner.jsonl b/x\n\
                    +{\"type\":\"pattern\",\"id\":\"mx-x\"}\n";
        assert!(parse_expertise_diff(none).is_empty());
        assert!(parse_expertise_diff("+{\"type\":\"pattern\"}").is_empty());
    }

    /// The record line head: the id-less record keeps its double
    /// space (`  + [convention]  no id conv` — probe-pinned bytes).
    #[test]
    fn record_line_keeps_the_empty_id_double_space() {
        let with_id: Value =
            serde_json::from_str(r#"{"type":"pattern","id":"mx-1bb21d"}"#).expect("json");
        assert_eq!(record_line(&with_id), "[pattern] mx-1bb21d");
        let id_less: Value =
            serde_json::from_str(r#"{"type":"convention","content":"no id conv"}"#).expect("json");
        assert_eq!(record_line(&id_less), "[convention] ");
    }

    /// The summary gates: a registered type renders its template (the
    /// builtin pattern summary is the bare name — probe-pinned); an
    /// unregistered one fails with the reference's throw wording; a
    /// MISSING type fails on the literal `undefined`.
    #[test]
    fn summary_gates_follow_the_reference_throw() {
        let registry = mulch::Registry::builtins();
        let pattern: Value =
            serde_json::from_str(r#"{"type":"pattern","name":"P One","description":"first"}"#)
                .expect("json");
        assert_eq!(
            record_summary(&registry, &pattern).expect("renders"),
            "P One"
        );
        let unknown: Value =
            serde_json::from_str(r#"{"type":"zzzcustom","name":"U"}"#).expect("json");
        let failure = record_summary(&registry, &unknown).expect_err("throws");
        assert_eq!(failure.message, "Error: Unknown record type: zzzcustom");
        let typeless: Value = serde_json::from_str(r#"{"content":"c"}"#).expect("json");
        let failure = record_summary(&registry, &typeless).expect_err("throws");
        assert_eq!(failure.message, "Error: Unknown record type: undefined");
    }

    /// The no-changes surface end-to-end in a clean repo: exit 0 with
    /// the plain line (a real missing ref degrades to the same empty
    /// read — the git read law).
    #[test]
    fn clean_repo_reports_no_changes() {
        let repo = scratch_repo("clean");
        let opts = plain_opts();
        let args = DiffArgs {
            since: "HEAD~1".into(),
        };
        in_scratch_cwd(&repo, || {
            // A first-commit repo has no HEAD~1: the read law empties
            // it; a truly missing ref reads as empty the same way.
            run(&opts, &args).expect("clean repo is not an error");
            let missing_ref = DiffArgs {
                since: "nosuchref".into(),
            };
            run(&opts, &missing_ref).expect("a missing ref reads as empty");
        });
        std::fs::remove_dir_all(&repo).ok();
    }

    /// The unknown-type crash point end-to-end: a plain run over a
    /// store whose diff carries an unregistered type fails AFTER the
    /// git read with the reference's throw — and the no-git gate
    /// carries the two-sentence pair (plain wording pinned here; the
    /// battery pins both channels against the reference).
    #[test]
    fn unknown_type_and_no_git_gates_fail_like_the_reference() {
        let opts = plain_opts();
        let args = DiffArgs {
            since: "HEAD~1".into(),
        };
        let outside = scratch_dir("nogit");
        in_scratch_cwd(&outside, || {
            let failure = run(&opts, &args).expect_err("not a git repository");
            assert_eq!(failure.message, "Error: not in a git repository.");
            assert_eq!(failure.code, crate::output::EXIT_ERROR);
            assert!(failure.envelope_to_stderr, "the plain error is stderr");
        });
        std::fs::remove_dir_all(&outside).ok();

        let repo = scratch_repo("unknown");
        commit_file(
            &repo,
            ".mulch/expertise/dev.jsonl",
            "{\"type\":\"zzzcustom\",\"id\":\"mx-u\",\"name\":\"U\"}\n",
            "c1",
        );
        in_scratch_cwd(&repo, || {
            let failure = run(&opts, &args).expect_err("unknown type throws in plain mode");
            assert_eq!(failure.message, "Error: Unknown record type: zzzcustom");
        });
        std::fs::remove_dir_all(&repo).ok();
    }

    /// The plain-mode global flags for run-level tests.
    fn plain_opts() -> GlobalOpts {
        GlobalOpts {
            json:                  false,
            quiet:                 false,
            verbose:               false,
            timing:                false,
            allow_unknown_types:   false,
            allow_domain_mismatch: false,
            format:                None,
        }
    }

    /// Runs `body` with the process cwd inside `root`, serialized by
    /// one mutex and restored after (run() resolves the cwd itself —
    /// the store.rs twin of this helper owns the same law).
    fn in_scratch_cwd<T>(root: &Path, body: impl FnOnce() -> T) -> T {
        static CWD: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = CWD
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let previous = std::env::current_dir().expect("cwd readable");
        std::env::set_current_dir(root).expect("chdir into the scratch repo");
        let result = body();
        std::env::set_current_dir(previous).expect("chdir back");
        result
    }
}
