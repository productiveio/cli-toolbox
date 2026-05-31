//! Local git-worktree detection.
//!
//! Given a set of root directories (configured via `[worktrees].roots`), scan
//! each one non-recursively for git working trees and index them by the branch
//! they have checked out. PRs are then matched against the index by
//! `(repo, head branch)` so the TUI can show which PRs have a local checkout
//! and let the user copy the path / open it in an editor.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A branch → local-checkout index built by scanning the configured roots.
#[derive(Debug, Clone, Default)]
pub struct WorktreeIndex {
    /// Branch name → checkouts. A branch name can in principle appear in more
    /// than one repo (e.g. `develop`), so the value is a list and matching
    /// disambiguates by repo.
    by_branch: HashMap<String, Vec<Entry>>,
}

#[derive(Debug, Clone)]
struct Entry {
    /// Short repo name parsed from the worktree's `origin` remote (e.g.
    /// `ai-agent`). Empty when the remote couldn't be resolved.
    repo: String,
    path: PathBuf,
}

impl WorktreeIndex {
    /// Scan each root's immediate subdirectories for git working trees. A
    /// directory qualifies when it contains a `.git` entry (a file for linked
    /// worktrees, a directory for plain clones) and `git` reports a branch.
    /// Unreadable roots and detached-HEAD checkouts are skipped silently —
    /// detection is best-effort and must never block the board.
    pub fn scan(roots: &[String]) -> Self {
        let mut by_branch: HashMap<String, Vec<Entry>> = HashMap::new();
        for root in roots {
            let root = expand_tilde(root);
            let Ok(dir_entries) = std::fs::read_dir(&root) else {
                continue;
            };
            for dir_entry in dir_entries.flatten() {
                let path = dir_entry.path();
                if !path.is_dir() || !path.join(".git").exists() {
                    continue;
                }
                let Some(branch) = git_branch(&path) else {
                    continue;
                };
                let repo = git_repo_name(&path).unwrap_or_default();
                by_branch
                    .entry(branch)
                    .or_default()
                    .push(Entry { repo, path });
            }
        }
        Self { by_branch }
    }

    /// Build an index directly from `(branch, repo, path)` triples. Test-only
    /// — production code always goes through [`scan`](Self::scan).
    #[cfg(test)]
    pub(crate) fn from_triples(triples: &[(&str, &str, &str)]) -> Self {
        let mut by_branch: HashMap<String, Vec<Entry>> = HashMap::new();
        for (branch, repo, path) in triples {
            by_branch
                .entry((*branch).to_string())
                .or_default()
                .push(Entry {
                    repo: (*repo).to_string(),
                    path: PathBuf::from(path),
                });
        }
        Self { by_branch }
    }

    /// Resolve the local worktree path for a PR's `(repo, head branch)`.
    ///
    /// A unique branch match wins outright — branch names are effectively
    /// unique per task, so this is the common case even if the repo couldn't
    /// be parsed. When several checkouts share a branch name, the one whose
    /// repo matches is chosen; an unresolvable collision returns `None` rather
    /// than guessing.
    pub fn resolve(&self, repo: &str, branch: &str) -> Option<&Path> {
        let entries = self.by_branch.get(branch)?;
        match entries.as_slice() {
            [only] => Some(only.path.as_path()),
            many => many
                .iter()
                .find(|e| e.repo == repo)
                .map(|e| e.path.as_path()),
        }
    }
}

/// Expand a leading `~/` to the user's home directory. Other paths pass
/// through untouched.
fn expand_tilde(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest);
    }
    PathBuf::from(p)
}

/// The branch checked out in `dir`, or `None` for detached HEAD / non-repos.
fn git_branch(dir: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["symbolic-ref", "--quiet", "--short", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!branch.is_empty()).then_some(branch)
}

/// Short repo name from `dir`'s `origin` remote, e.g. `ai-agent`.
fn git_repo_name(dir: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    repo_name_from_remote(&url)
}

/// Extract the repo name from an ssh or https git remote URL. Handles both
/// `git@github.com:owner/repo.git` and `https://github.com/owner/repo`.
fn repo_name_from_remote(url: &str) -> Option<String> {
    let trimmed = url.trim().trim_end_matches('/');
    let trimmed = trimmed.strip_suffix(".git").unwrap_or(trimmed);
    let last = trimmed.rsplit(['/', ':']).next()?;
    (!last.is_empty()).then(|| last.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_name_parses_ssh_and_https() {
        assert_eq!(
            repo_name_from_remote("git@github.com:productiveio/ai-agent.git").as_deref(),
            Some("ai-agent")
        );
        assert_eq!(
            repo_name_from_remote("https://github.com/productiveio/cli-toolbox").as_deref(),
            Some("cli-toolbox")
        );
        assert_eq!(
            repo_name_from_remote("https://github.com/productiveio/api/").as_deref(),
            Some("api")
        );
        assert_eq!(repo_name_from_remote(""), None);
    }

    #[test]
    fn expand_tilde_only_touches_leading_tilde() {
        // Non-tilde paths pass through verbatim.
        assert_eq!(expand_tilde("/abs/path"), PathBuf::from("/abs/path"));
        // `~/x` resolves under home (when home is known).
        if let Some(home) = dirs::home_dir() {
            assert_eq!(expand_tilde("~/wt"), home.join("wt"));
        }
    }

    #[test]
    fn resolve_unique_branch_wins_and_collisions_disambiguate_by_repo() {
        let idx = WorktreeIndex::from_triples(&[
            ("fix/foo", "ai-agent", "/wt/ai-agent-foo"),
            ("develop", "api", "/wt/api"),
            ("develop", "frontend", "/wt/frontend"),
        ]);
        // Unique branch → matched regardless of repo argument.
        assert_eq!(
            idx.resolve("ai-agent", "fix/foo"),
            Some(Path::new("/wt/ai-agent-foo"))
        );
        // Colliding branch → disambiguated by repo.
        assert_eq!(
            idx.resolve("frontend", "develop"),
            Some(Path::new("/wt/frontend"))
        );
        // Colliding branch, no repo match → None rather than a wrong guess.
        assert_eq!(idx.resolve("payments", "develop"), None);
        // Unknown branch → None.
        assert_eq!(idx.resolve("api", "missing"), None);
    }
}
