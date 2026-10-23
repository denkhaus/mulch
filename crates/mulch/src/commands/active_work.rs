//! `utils/active-work.ts`: the tracker resolvers behind prime's
//! auto-context-scope (mulch-16da). Each tracker answers "what is the
//! agent working on right now" from the cheapest local signal first
//! (an in-progress issue row), then from the branch name, and gives
//! up quietly when neither carries an id.
//!
//! The reference's regexes are hand-rolled here (no regex
//! dependency in this crate); the boundary laws, dedup and sort
//! order are the JS `\b` / `new Set` / `Array#sort` semantics.
//! Warnings (`multiple candidates`) are not collected: prime is the
//! only consumer and ignores them.

use std::path::Path;

use serde_json::Value;

use crate::commands::git_context::GitContext;

/// The resolved active work, one value per tracker (`TRACKERS`
/// order).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ActiveWork {
    pub(crate) seeds:  Option<String>,
    pub(crate) gh:     Option<String>,
    pub(crate) linear: Option<String>,
    pub(crate) bead:   Option<String>,
}

/// The reference's tracker order (`TRACKERS`).
pub(crate) const TRACKERS: [&str; 4] = ["seeds", "gh", "linear", "bead"];

impl ActiveWork {
    /// The `(tracker, value)` pairs for
    /// [`mulch::prime_ranking::ActiveContext`].
    pub(crate) fn trackers(&self) -> Vec<(&'static str, String)> {
        [
            ("seeds", &self.seeds),
            ("gh", &self.gh),
            ("linear", &self.linear),
            ("bead", &self.bead),
        ]
        .into_iter()
        .filter_map(|(name, value)| value.clone().map(|value| (name, value)))
        .collect()
    }
}

/// `resolveActiveWork`: the first resolver to claim a tracker wins
/// (there is exactly one per tracker here), and only unambiguous
/// single matches surface — multiple candidates stay unresolved,
/// exactly like the reference.
pub(crate) fn resolve_active_work(cwd: &Path) -> ActiveWork {
    let git = GitContext::at(cwd);
    let branch = git.current_branch();
    // A resolver that answered (even ambiguously) claims its tracker;
    // only a resolver with NO hit falls through to the branch.
    ActiveWork {
        seeds:  match issue_file_ids(&cwd.join(".seeds").join("issues.jsonl")) {
            Some(ids) => single(ids.into_iter()),
            None => single(branch_matches(&branch, IdPattern::Seeds).into_iter()),
        },
        gh:     match resolve_gh(&branch, cwd) {
            Some(hit) => Some(hit),
            None => single(
                branch_matches(&branch, IdPattern::Gh)
                    .into_iter()
                    .map(|number| format!("#{number}")),
            ),
        },
        linear: single(branch_matches(&branch, IdPattern::Linear).into_iter()),
        bead:   match issue_file_ids(&cwd.join(".beads").join("issues.jsonl")) {
            Some(ids) => single(ids.into_iter()),
            None => single(branch_matches(&branch, IdPattern::Bead).into_iter()),
        },
    }
}

/// The reference's single-match rule: one candidate resolves, two or
/// more stay unset.
fn single(mut matches: impl Iterator<Item = String>) -> Option<String> {
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

/// `seedsResolver`/`beadResolver`: the sorted, deduped ids of the
/// `in_progress` issue rows (`None` when the file has none, which
/// lets the branch fallback run).
fn issue_file_ids(path: &Path) -> Option<Vec<String>> {
    let rows = read_jsonl(path);
    let mut ids: Vec<String> = rows
        .iter()
        .filter(|row| row.get("status").and_then(Value::as_str) == Some("in_progress"))
        .filter_map(|row| row.get("id").and_then(Value::as_str))
        .map(ToOwned::to_owned)
        .collect();
    ids.sort();
    ids.dedup();
    (!ids.is_empty()).then_some(ids)
}

/// `ghResolver`'s `gh pr view` probe: only a digits-only answer
/// resolves; any failure falls through to the branch.
fn resolve_gh(branch: &str, cwd: &Path) -> Option<String> {
    if branch.is_empty() {
        return None;
    }
    let output = std::process::Command::new("gh")
        .args(["pr", "view", "--json", "number", "--jq", ".number"])
        .current_dir(cwd)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let number = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| format!("#{number}"))
}

/// `readJsonl`: trimmed non-empty, non-comment lines that parse.
fn read_jsonl(path: &Path) -> Vec<Value> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    text.split('\n')
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                return None;
            }
            serde_json::from_str(trimmed).ok()
        })
        .collect()
}

/// The reference's four id regexes.
#[derive(Clone, Copy)]
enum IdPattern {
    /// `\b([a-z][a-z0-9]*-[a-f0-9]{4,8})\b`
    Seeds,
    /// `\b([A-Z]{2,}[A-Z0-9]*-\d+)\b`
    Linear,
    /// `\b((?:bd|bead)-\d+)\b`, case-insensitive
    Bead,
    /// `(?:^|[^a-zA-Z0-9])(?:gh-|#)(\d+)\b`, case-insensitive
    Gh,
}

/// `text.matchAll(re)` + `[...new Set].sort()`.
fn branch_matches(branch: &str, pattern: IdPattern) -> Vec<String> {
    let haystack = match pattern {
        IdPattern::Bead | IdPattern::Gh => branch.to_ascii_lowercase(),
        IdPattern::Seeds | IdPattern::Linear => branch.to_string(),
    };
    let bytes = haystack.as_bytes();
    let mut found: Vec<String> = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let end = match pattern {
            IdPattern::Seeds => match_seeds(bytes, index),
            IdPattern::Linear => match_linear(bytes, index),
            IdPattern::Bead => match_bead(bytes, index),
            IdPattern::Gh => match_gh(bytes, index),
        };
        match end {
            Some(end) => {
                found.push(haystack[index..end].to_string());
                index = end;
            }
            None => index += 1,
        }
    }
    found.sort();
    found.dedup();
    found
}

/// `\b` at `index` for a match starting there: start-of-text or a
/// non-word char before, word char at.
fn starts_word(bytes: &[u8], index: usize) -> bool {
    let boundary = index == 0 || !is_word(bytes[index - 1]);
    boundary && index < bytes.len() && is_word(bytes[index])
}

/// `\b` at the exclusive end.
fn ends_word(bytes: &[u8], end: usize) -> bool {
    end == bytes.len() || !is_word(bytes[end])
}

fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn match_seeds(bytes: &[u8], start: usize) -> Option<usize> {
    if !starts_word(bytes, start) || !bytes[start].is_ascii_lowercase() {
        return None;
    }
    let mut index = start + 1;
    while index < bytes.len()
        && (bytes[index].is_ascii_lowercase() || bytes[index].is_ascii_digit())
    {
        index += 1;
    }
    if index >= bytes.len() || bytes[index] != b'-' {
        return None;
    }
    let hex_start = index + 1;
    let mut hex_end = hex_start;
    while hex_end < bytes.len() && bytes[hex_end].is_ascii_hexdigit() {
        hex_end += 1;
    }
    // Greedy 8 first, then backtrack while the tail shrinks below 4.
    let mut end = hex_end.min(hex_start + 8);
    while end >= hex_start + 4 {
        if ends_word(bytes, end) {
            return Some(end);
        }
        end -= 1;
    }
    None
}

fn match_linear(bytes: &[u8], start: usize) -> Option<usize> {
    if !starts_word(bytes, start) || !bytes[start].is_ascii_uppercase() {
        return None;
    }
    let mut index = start;
    while index < bytes.len() && bytes[index].is_ascii_uppercase() {
        index += 1;
    }
    if index - start < 2 {
        return None;
    }
    while index < bytes.len()
        && (bytes[index].is_ascii_uppercase() || bytes[index].is_ascii_digit())
    {
        index += 1;
    }
    if index >= bytes.len() || bytes[index] != b'-' {
        return None;
    }
    let digits_start = index + 1;
    let mut end = digits_start;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    (end > digits_start && ends_word(bytes, end)).then_some(end)
}

fn match_bead(bytes: &[u8], start: usize) -> Option<usize> {
    if !starts_word(bytes, start) {
        return None;
    }
    let rest = &bytes[start..];
    let prefix = if rest.starts_with(b"bead-") {
        "bead-".len()
    } else if rest.starts_with(b"bd-") {
        "bd-".len()
    } else {
        return None;
    };
    let mut end = start + prefix;
    let digits_start = end;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    (end > digits_start && ends_word(bytes, end)).then_some(end)
}

fn match_gh(bytes: &[u8], start: usize) -> Option<usize> {
    // The regex's leading class consumes one non-alnum char (or the
    // text start); `start` is the group position here.
    let boundary = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
    if !boundary {
        return None;
    }
    let rest = &bytes[start..];
    let prefix = if rest.starts_with(b"gh-") {
        "gh-".len()
    } else if rest.starts_with(b"#") {
        1
    } else {
        return None;
    };
    let mut end = start + prefix;
    let digits_start = end;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    (end > digits_start && ends_word(bytes, end)).then_some(end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(branch: &str, pattern: IdPattern) -> Vec<String> {
        branch_matches(branch, pattern)
    }

    #[test]
    fn seeds_ids_need_a_dash_and_hex_tail() {
        assert_eq!(ids("feat/mulch-f3d0-work", IdPattern::Seeds), [
            "mulch-f3d0"
        ]);
        assert_eq!(ids("mulch-f3d0abc", IdPattern::Seeds), ["mulch-f3d0abc"]);
        assert_eq!(
            ids("mulch-f3d0abcde", IdPattern::Seeds),
            Vec::<String>::new()
        );
        assert_eq!(ids("x-mulch-f3d0", IdPattern::Seeds), ["mulch-f3d0"]);
        assert_eq!(ids("MULCH-f3d0", IdPattern::Seeds), Vec::<String>::new());
    }

    #[test]
    fn linear_ids_need_two_leading_uppercase() {
        assert_eq!(ids("ENG-42-x", IdPattern::Linear), ["ENG-42"]);
        assert_eq!(ids("A-42", IdPattern::Linear), Vec::<String>::new());
        assert_eq!(ids("eng-42", IdPattern::Linear), Vec::<String>::new());
    }

    #[test]
    fn bead_ids_are_case_insensitive_and_lowercased() {
        assert_eq!(ids("BD-7", IdPattern::Bead), ["bd-7"]);
        assert_eq!(ids("bead-12", IdPattern::Bead), ["bead-12"]);
    }

    #[test]
    fn gh_ids_need_a_boundary_before_the_prefix() {
        assert_eq!(ids("#42-fix", IdPattern::Gh), ["#42"]);
        assert_eq!(ids("gh-7", IdPattern::Gh), ["gh-7"]);
        assert_eq!(ids("x#42", IdPattern::Gh), Vec::<String>::new());
    }
}
