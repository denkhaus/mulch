//! The git process adapter (mulch-cc7a): the ONE owner of git
//! subprocess spawns in the binary, named after the reference's
//! `utils/git.ts` seam. Every git invocation — the 21d9 read laws
//! behind [`super::git_context`] and the write laws `ml sync` needs
//! (mulch-176f: has-changes, add, commit, probed from the
//! reference's `commands/sync.ts`) — goes through [`run`] or
//! [`spawn`] here, so argv, cwd handling, and failure shape stay
//! pinned in one place.
//!
//! Two failure laws, both probed from the reference:
//!
//! - Reads ([`run`]) are empty-on-failure: the reference wraps every read in
//!   try/catch and defaults to "no signal" (`utils/git.ts`
//!   getChangedFiles/getActiveFiles, `sync.ts` gitHasChanges).
//! - Writes ([`add`], [`commit`]) THROW: the reference's `gitAdd` / `gitCommit`
//!   (`sync.ts` 42-52) let execFileSync propagate, and sync renders the thrown
//!   message as `Git error: <message>` (`sync.ts` 289-296) — [`GitError`]
//!   carries that shape.
//!
//! `gh` is deliberately NOT this seam's business: the reference
//! keeps its `gh pr view` probe in `utils/active-work.ts` (line
//! 113), off `utils/git.ts` — see `active_work::resolve_gh`.

// 176f prep (mulch-cc7a): the sync write laws land ahead of their
// only consumer (`ml sync`), like the render seam before them.
#![allow(
    dead_code,
    reason = "176f prep (mulch-cc7a): the sync laws land ahead of `ml sync`"
)]

use std::path::Path;

/// A failed git invocation from a write law — the Rust side of the
/// reference's thrown `execFileSync` error: sync prefixes the
/// rendered message with `Git error: ` (`sync.ts` 289-296).
#[derive(Debug, thiserror::Error)]
pub(crate) enum GitError {
    /// git ran and exited non-zero. Node's thrown message is
    /// `Command failed: git <argv>` with the captured stderr
    /// appended after a newline; the display mirrors that shape so
    /// 176f can render byte-parity output. stdout rides along as a
    /// field (Node's `err.stdout`): git writes some diagnostics —
    /// e.g. `nothing to commit` — there, not to stderr.
    #[error("Command failed: git {argv}\n{stderr}")]
    Failed {
        /// The argv after `git`, space-joined as Node renders it.
        argv:   String,
        /// git's exit code.
        code:   i32,
        /// git's stderr, verbatim (already lossy-decoded).
        stderr: String,
        /// git's stdout, verbatim (already lossy-decoded).
        stdout: String,
    },

    /// git could not be spawned at all (missing binary, exec
    /// permission) — Node's `spawn git ENOENT` throw.
    #[error("spawn git: {source}")]
    Spawn {
        /// The argv after `git`, kept as context for the caller.
        argv: String,

        /// The underlying spawn failure.
        #[source]
        source: std::io::Error,
    },
}

/// `utils/git.ts` `isGitRepo` (and `sync.ts` 17-27's private copy):
/// `git rev-parse --is-inside-work-tree` succeeding is the whole
/// law — the output is never read.
pub(crate) fn is_git_repo(cwd: &Path) -> bool {
    run(cwd, &["rev-parse", "--is-inside-work-tree"]).is_some()
}

/// `sync.ts` 29-40 `gitHasChanges`: `git status --porcelain .mulch/`
/// — any non-empty trimmed output means changes. The pathspec is the
/// law: dirt outside `.mulch/` does not count. A failed call reads
/// as "no changes" (the reference's catch returns false).
pub(crate) fn has_changes(cwd: &Path) -> bool {
    run(cwd, &["status", "--porcelain", ".mulch/"]).is_some_and(|status| !status.trim().is_empty())
}

/// `sync.ts` 42-44 `gitAdd`: `git add .mulch/`, cwd-scoped. Unlike
/// the read laws this THROWS — [`GitError`] carries the failure for
/// sync's `Git error: …` rendering.
pub(crate) fn add(cwd: &Path) -> Result<(), GitError> {
    spawn(cwd, &["add", ".mulch/"]).map(|_| ())
}

/// `sync.ts` 46-52 `gitCommit`: `git commit -m <message>`, returning
/// git's stdout (the reference's helper returns it; sync ignores the
/// value). No `--allow-empty` — committing nothing is a failure,
/// which is how sync's no-changes path stays a non-error.
pub(crate) fn commit(cwd: &Path, message: &str) -> Result<String, GitError> {
    spawn(cwd, &["commit", "-m", message])
}

/// The read law (21d9, moved here from git_context): runs git with
/// `args` in `cwd`; `None` on any failure — spawn error, non-zero
/// exit, non-UTF-8 stdout. The reference's per-call try/catch with
/// an empty fallback.
pub(crate) fn run(cwd: &Path, args: &[&str]) -> Option<String> {
    let output = git(cwd, args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

/// The write law's runner: same spawn, but a failure is an error
/// carrying the Node throw shape instead of an empty fallback.
fn spawn(cwd: &Path, args: &[&str]) -> Result<String, GitError> {
    let output = match git(cwd, args).output() {
        Ok(output) => output,
        Err(source) => {
            return Err(GitError::Spawn {
                argv: args.join(" "),
                source,
            });
        }
    };
    if !output.status.success() {
        return Err(GitError::Failed {
            argv:   args.join(" "),
            code:   output.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The one `git` Command construction: binary, args, cwd — the
/// single place a git process enters this binary.
fn git(cwd: &Path, args: &[&str]) -> std::process::Command {
    let mut command = std::process::Command::new("git");
    command.args(args).current_dir(cwd);
    command
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::{GitError, add, commit, has_changes, is_git_repo, run};
    use crate::commands::git_context::GitContext;

    static SERIAL: AtomicU64 = AtomicU64::new(0);

    /// Scratch-root helper: one unique temp dir per call.
    fn scratch_dir(tag: &str) -> PathBuf {
        let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "mulch-git-adapter-{tag}-{}-{serial}",
            std::process::id()
        ));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("scratch root");
        dir
    }

    /// Fixture git: runs raw git in `cwd` (test scaffolding, not the
    /// adapter). Repo-local config disables hooks and signing so the
    /// machine's global git config cannot break the fixture.
    fn fixture_git(cwd: &Path, args: &[&str]) -> String {
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
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    /// A scratch git repository: `init`, test identity, hooks and
    /// signing off (repo-local), and one root commit so HEAD, diff,
    /// and commit laws all have history to work against.
    fn scratch_repo(tag: &str) -> PathBuf {
        let dir = scratch_dir(tag);
        fixture_git(&dir, &["init", "-q"]);
        fixture_git(&dir, &["config", "user.email", "test@mulch.local"]);
        fixture_git(&dir, &["config", "user.name", "mulch adapter test"]);
        fixture_git(&dir, &["config", "core.hooksPath", "/dev/null"]);
        fixture_git(&dir, &["config", "commit.gpgsign", "false"]);
        fixture_git(&dir, &["commit", "--allow-empty", "-q", "-m", "root"]);
        dir
    }

    /// Tracks `path` (one write plus a commit) so later diffs see it.
    fn track_file(repo: &Path, path: &str, body: &str) {
        let file = repo.join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).expect("parent dir");
        }
        std::fs::write(file, body).expect("scratch file");
        fixture_git(repo, &["add", path]);
        fixture_git(repo, &["commit", "-q", "-m", &format!("track {path}")]);
    }

    #[test]
    fn is_git_repo_law_is_exit_code_only() {
        let outside = scratch_dir("outside");
        assert!(!is_git_repo(&outside));
        let repo = scratch_repo("inside");
        assert!(is_git_repo(&repo));
    }

    /// sync.ts:29-40's catch arm: outside a repository, changes read
    /// as false — never an error, never a panic.
    #[test]
    fn has_changes_law_is_false_outside_a_repo() {
        let outside = scratch_dir("haschanges-outside");
        assert!(!has_changes(&outside));
    }

    #[test]
    fn read_law_is_empty_on_failure() {
        let outside = scratch_dir("readfail");
        // No repo: the call fails into None, not a panic.
        assert_eq!(run(&outside, &["rev-parse", "HEAD"]), None);
        // A failing call INSIDE a repo is equally empty.
        let repo = scratch_repo("readfail-repo");
        assert_eq!(run(&repo, &["rev-parse", "no-such-ref"]), None);
    }

    #[test]
    fn changed_files_laws_stay_empty_in_a_clean_repo() {
        let repo = scratch_repo("clean");
        let git = GitContext::at(&repo);
        assert!(
            git.context_files().is_empty(),
            "clean repo: no context files"
        );
        assert!(git.active_files().is_empty(), "clean repo: no active files");
    }

    #[test]
    fn changed_files_laws_split_untracked_from_modified() {
        let repo = scratch_repo("dirty");
        track_file(&repo, "src/lib.rs", "one\n");
        // A modified tracked file and an untracked one.
        std::fs::write(repo.join("src/lib.rs"), "two\n").expect("modify");
        std::fs::create_dir_all(repo.join("notes")).expect("notes dir");
        std::fs::write(repo.join("notes/new.md"), "note\n").expect("untracked");
        let git = GitContext::at(&repo);
        // The diff law never sees untracked files.
        assert_eq!(git.context_files(), ["src/lib.rs"]);
        // The status law lists both, individually (-uall).
        assert_eq!(git.active_files(), ["notes/new.md", "src/lib.rs"]);
    }

    #[test]
    fn has_changes_law_scopes_to_the_mulch_dir() {
        let repo = scratch_repo("scope");
        track_file(&repo, "src/lib.rs", "one\n");
        // Clean tree first.
        assert!(!has_changes(&repo));
        // Dirt outside .mulch/ does not count (sync.ts:31 pathspec).
        std::fs::write(repo.join("src/lib.rs"), "two\n").expect("modify");
        assert!(!has_changes(&repo));
        // Dirt inside .mulch/ does — untracked counts too.
        std::fs::create_dir_all(repo.join(".mulch")).expect("mulch dir");
        std::fs::write(repo.join(".mulch/mulch.config.yaml"), "domains: {}\n").expect("mulch file");
        assert!(has_changes(&repo));
    }

    #[test]
    fn add_law_stages_only_the_mulch_dir() {
        let repo = scratch_repo("addlaw");
        std::fs::create_dir_all(repo.join(".mulch/expertise")).expect("mulch tree");
        std::fs::write(repo.join(".mulch/expertise/rust.jsonl"), "{}\n").expect("mulch file");
        std::fs::write(repo.join("other.txt"), "outside\n").expect("outside file");
        add(&repo).expect("git add succeeds");
        let status = fixture_git(&repo, &["status", "--porcelain"]);
        // .mulch/ is staged (A), the outside file stays untracked (??).
        assert_eq!(status.lines().collect::<Vec<_>>(), [
            "A  .mulch/expertise/rust.jsonl",
            "?? other.txt"
        ]);
    }

    #[test]
    fn commit_law_passes_message_bytes_verbatim() {
        let repo = scratch_repo("commitlaw");
        std::fs::create_dir_all(repo.join(".mulch")).expect("mulch dir");
        std::fs::write(repo.join(".mulch/mulch.config.yaml"), "domains: {}\n").expect("mulch file");
        add(&repo).expect("staged");
        // The reference default message (sync.ts:272), spaces and all.
        commit(&repo, "mulch: update expertise").expect("git commit succeeds");
        let subject = fixture_git(&repo, &["log", "-1", "--format=%s"]);
        assert_eq!(subject.trim_end(), "mulch: update expertise");
    }

    #[test]
    fn commit_law_has_no_allow_empty() {
        let repo = scratch_repo("emptycommit");
        // Clean tree: the reference passes no --allow-empty, so an
        // empty commit is a git failure, not an Ok. git 2.43 writes
        // the `nothing to commit` diagnostic to stdout, so that is
        // the field that carries it (Node's err.stdout).
        let error = commit(&repo, "nothing here").expect_err("nothing staged");
        match &error {
            GitError::Failed { code, stdout, .. } => {
                assert_ne!(*code, 0);
                assert!(stdout.contains("nothing to commit"), "stdout: {stdout}");
            }
            other @ GitError::Spawn { .. } => panic!("expected a failed exit, got {other:?}"),
        }
    }

    #[test]
    fn failing_write_law_renders_giterror_not_a_panic() {
        let outside = scratch_dir("outsideadd");
        let error = add(&outside).expect_err("add outside a repo fails");
        let rendered = error.to_string();
        // The Node throw shape sync prefixes with `Git error: `.
        assert!(
            rendered.starts_with("Command failed: git add .mulch/"),
            "rendered: {rendered}"
        );
        assert!(
            rendered.contains("not a git repository"),
            "rendered: {rendered}"
        );
    }
}
