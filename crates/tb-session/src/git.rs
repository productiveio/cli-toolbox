use std::path::{Path, PathBuf};
use std::process::Command;

/// Get all worktree paths for the git repo containing `cwd`.
/// Returns the main worktree + all linked worktrees.
/// Returns just `[cwd]` if not in a git repo or git is unavailable.
pub fn repo_paths(cwd: &Path) -> Vec<PathBuf> {
    let output = match Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(cwd)
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return vec![cwd.to_path_buf()],
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let paths: Vec<PathBuf> = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .map(PathBuf::from)
        .collect();

    if paths.is_empty() {
        vec![cwd.to_path_buf()]
    } else {
        paths
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repo_paths_returns_cwd_outside_git() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = repo_paths(tmp.path());
        assert_eq!(paths, vec![tmp.path().to_path_buf()]);
    }

    #[test]
    fn test_repo_paths_returns_at_least_one_in_git() {
        let cwd = std::env::current_dir().unwrap();
        let paths = repo_paths(&cwd);
        assert!(!paths.is_empty());
    }
}
