//! The git PROCESS adapter (mulch-21d9): the subprocess side of the
//! git-context seam. The pure matching laws live in the library
//! (`mulch::git_context` — `normalize_dir_anchor`, `file_matches_any`,
//! `filter_by_context`, `match_files_to_domains`); this CLI-side
//! module owns the three changed-file laws and the environment facts
//! the read commands share, each with the reference's
//! empty-on-failure fallback (no repo, no git, failed call → no
//! files / no sha / empty branch):
//!
//! - [`GitContext::context_files`] — `utils/git-context.ts` `getContextFiles`:
//!   staged plus unstaged changes, deduped, sorted (`record`'s auto-populated
//!   files).
//! - [`GitContext::changed_files_since`] — `utils/git.ts` `getChangedFiles`:
//!   committed since a ref, plus staged, plus unstaged, set-deduped, sorted
//!   (`learn`, `prime --context`).
//! - [`GitContext::active_files`] — `utils/git.ts` `getActiveFiles`: `git
//!   status --porcelain -uall`, untracked files included individually, rename
//!   destinations kept (prime's auto-context-scope — the "what is the agent
//!   about to work on" set, deliberately wider than a diff).

// The prime/learn consumers land with the 16da children this sprint;
// until then the adapter's not-yet-called surface is prep, like the
// render seam before it.
#![allow(
    dead_code,
    reason = "16da prep (mulch-21d9): the adapter lands ahead of its consumers"
)]

use std::path::{Path, PathBuf};

/// One git environment: every call runs in [`GitContext::cwd`],
/// mirroring the reference's per-call `execFileSync` with an
/// explicit cwd.
pub(crate) struct GitContext {
    cwd: PathBuf,
}

impl GitContext {
    /// Captures the process working directory (the reference passes
    /// its resolved cwd into every git call; an unresolvable cwd
    /// degrades to the empty path, whose git calls fail into the
    /// empty-on-failure laws).
    pub(crate) fn process_cwd() -> Self {
        Self {
            cwd: std::env::current_dir().unwrap_or_default(),
        }
    }

    /// The same adapter pinned to an explicit directory (prime's
    /// auto-scope resolves against its own cwd).
    pub(crate) fn at(cwd: &Path) -> Self {
        Self {
            cwd: cwd.to_path_buf(),
        }
    }

    /// `utils/git.ts` `isGitRepo`: `rev-parse --is-inside-work-tree`
    /// succeeding is the whole law.
    pub(crate) fn is_git_repo(&self) -> bool {
        self.run(["rev-parse", "--is-inside-work-tree"]).is_some()
    }

    /// Auto-populated git evidence: the full HEAD sha when cwd is a
    /// git repository (reference probe: full 40-char sha; absent
    /// outside git).
    pub(crate) fn head_sha(&self) -> Option<String> {
        self.run(["rev-parse", "HEAD"])
            .map(|sha| sha.trim().to_string())
    }

    /// `utils/active-work.ts` `getCurrentBranch`: the branch name,
    /// empty on failure AND on a detached HEAD (the reference maps
    /// the literal `HEAD` to `""`).
    pub(crate) fn current_branch(&self) -> String {
        let Some(branch) = self.run(["rev-parse", "--abbrev-ref", "HEAD"]) else {
            return String::new();
        };
        let branch = branch.trim();
        if branch == "HEAD" {
            String::new()
        } else {
            branch.to_string()
        }
    }

    /// The reference's `getContextFiles` (`utils/git-context.ts`):
    /// staged plus unstaged changed file names, deduped and sorted;
    /// no git repository (or git failure) means no files. Untracked
    /// files never appear — the source is
    /// `git diff --name-only [--cached]`.
    pub(crate) fn context_files(&self) -> Vec<String> {
        let mut files: Vec<String> = Vec::new();
        for staged in [true, false] {
            // each call is independent (the reference's two try/catch
            // blocks): a failed staged read keeps whatever the
            // unstaged one collected
            let mut args = vec!["diff", "--name-only"];
            if staged {
                args.push("--cached");
            }
            let Some(text) = self.run(args) else {
                continue;
            };
            for file in text.lines().filter(|line| !line.is_empty()) {
                if !files.iter().any(|seen| seen == file) {
                    files.push(file.to_string());
                }
            }
        }
        files.sort();
        files
    }

    /// The reference's `getChangedFiles` (`utils/git.ts`): committed
    /// changes since `since`, plus staged, plus unstaged, as a
    /// set-deduped sorted list. A missing ref (first commit) is one
    /// more empty-on-failure — the other two reads still collect.
    pub(crate) fn changed_files_since(&self, since: &str) -> Vec<String> {
        let mut files: Vec<String> = Vec::new();
        for args in [
            vec!["diff", "--name-only", since],
            vec!["diff", "--name-only", "--cached"],
            vec!["diff", "--name-only"],
        ] {
            let Some(text) = self.run(args) else {
                continue;
            };
            for file in text.lines().filter(|line| !line.is_empty()) {
                if !files.iter().any(|seen| seen == file) {
                    files.push(file.to_string());
                }
            }
        }
        files.sort();
        files
    }

    /// The reference's `getActiveFiles` (`utils/git.ts`):
    /// `git status --porcelain -uall` — staged, unstaged, AND
    /// untracked files, rename destinations kept — parsed by
    /// [`parse_porcelain`].
    pub(crate) fn active_files(&self) -> Vec<String> {
        match self.run(["status", "--porcelain", "-uall"]) {
            Some(text) => parse_porcelain(&text),
            None => Vec::new(),
        }
    }

    /// Runs git with `args` in the captured cwd; `None` on any
    /// failure (spawn, non-zero exit, non-UTF-8 output) — the
    /// shared empty-on-failure law.
    fn run<I, S>(&self, args: I) -> Option<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let output = std::process::Command::new("git")
            .args(
                args.into_iter()
                    .map(|arg| arg.as_ref().to_string())
                    .collect::<Vec<_>>(),
            )
            .current_dir(&self.cwd)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        String::from_utf8(output.stdout).ok()
    }
}

/// The porcelain law (pure, testable): v1 lines are `XY <path>`;
/// renames (`R  old -> new`) keep the destination only. Returns the
/// deduped, sorted file set.
fn parse_porcelain(raw: &str) -> Vec<String> {
    let mut files: Vec<String> = Vec::new();
    for raw_line in raw.lines() {
        if raw_line.is_empty() {
            continue;
        }
        // slice(3) in the reference; a shorter line has no path
        // (cannot happen in porcelain v1, but stays defensive like
        // the JS slice)
        if raw_line.len() < 3 {
            continue;
        }
        let path = &raw_line[3..];
        let file = match path.find(" -> ") {
            Some(arrow) => &path[arrow + 4..],
            None => path,
        };
        if !file.is_empty() && !files.iter().any(|seen| seen == file) {
            files.push(file.to_string());
        }
    }
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::parse_porcelain;

    /// Porcelain v1 shapes: plain entries, untracked (`??`), rename
    /// destinations, dedup across staged+unstaged, sorted output.
    #[test]
    fn porcelain_parses_paths_and_rename_destinations() {
        let raw = "M  src/lib.rs\n\
                    M  src/lib.rs\n\
                    ?? notes/new.md\n\
                    R  old_name.rs -> src/moved.rs\n\
                    A  src/git_context.rs\n";
        assert_eq!(parse_porcelain(raw), vec![
            "notes/new.md".to_string(),
            "src/git_context.rs".to_string(),
            "src/lib.rs".to_string(),
            "src/moved.rs".to_string(),
        ]);
    }

    /// Empty lines and pathless residue never produce files; the
    /// empty input stays empty (the no-repo fallback shape).
    #[test]
    fn porcelain_skips_empty_lines_and_stays_empty() {
        assert_eq!(parse_porcelain(""), Vec::<String>::new());
        assert_eq!(parse_porcelain("\n\n"), Vec::<String>::new());
        assert_eq!(parse_porcelain("M  a\n\n?? b\n"), vec!["a", "b"]);
    }
}
