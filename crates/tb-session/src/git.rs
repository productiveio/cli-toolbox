use std::path::{Path, PathBuf};
use std::process::Command;

/// Parse the porcelain output of `git worktree list --porcelain` into paths.
fn parse_worktree_output(stdout: &str) -> Vec<PathBuf> {
    stdout
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .map(PathBuf::from)
        .collect()
}

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
    let paths = parse_worktree_output(&stdout);

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

    #[test]
    fn test_parse_worktree_output_multiple() {
        let output = "worktree /Users/test/repo\nHEAD abc123\nbranch refs/heads/main\n\nworktree /Users/test/worktrees/feature\nHEAD def456\nbranch refs/heads/feature\n\n";
        let paths = parse_worktree_output(output);
        assert_eq!(
            paths,
            vec![
                PathBuf::from("/Users/test/repo"),
                PathBuf::from("/Users/test/worktrees/feature"),
            ]
        );
    }

    #[test]
    fn test_parse_worktree_output_empty() {
        assert!(parse_worktree_output("").is_empty());
    }

    #[test]
    fn test_parse_worktree_output_no_worktree_lines() {
        let output = "HEAD abc123\nbranch refs/heads/main\n";
        assert!(parse_worktree_output(output).is_empty());
    }
}
