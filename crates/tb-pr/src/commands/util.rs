use std::process::Command;

use crate::error::{Error, Result};

/// Parsed GitHub PR reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrRef {
    pub owner: String,
    pub repo: String,
    pub number: u64,
}

impl PrRef {
    pub fn web_url(&self) -> String {
        format!(
            "https://github.com/{}/{}/pull/{}",
            self.owner, self.repo, self.number
        )
    }
}

/// Parse a user-supplied PR reference. Accepts:
///
/// - Full URL: `https://github.com/{owner}/{repo}/pull/{N}`
/// - `owner/repo#N`
/// - Bare number `N` (resolved from `git remote get-url origin` in the cwd)
pub fn parse_pr_ref(input: &str, default_org: &str) -> Result<PrRef> {
    if let Some(r) = parse_url(input) {
        return Ok(r);
    }
    if let Some(r) = parse_owner_repo_number(input) {
        return Ok(r);
    }
    if let Ok(number) = input.parse::<u64>() {
        return resolve_from_git(number, default_org);
    }
    Err(Error::Other(format!(
        "could not parse PR reference `{input}` — expected a GitHub URL, \
         `owner/repo#N`, or a bare number inside a git checkout"
    )))
}

fn parse_url(input: &str) -> Option<PrRef> {
    let stripped = input
        .strip_prefix("https://github.com/")
        .or_else(|| input.strip_prefix("http://github.com/"))?;
    let mut parts = stripped.split('/');
    let owner = parts.next()?.to_string();
    let repo = parts.next()?.to_string();
    let kind = parts.next()?; // "pull"
    let number_str = parts.next()?;
    if kind != "pull" {
        return None;
    }
    let number: u64 = number_str.trim_end_matches('/').parse().ok()?;
    Some(PrRef {
        owner,
        repo,
        number,
    })
}

fn parse_owner_repo_number(input: &str) -> Option<PrRef> {
    let (repo_part, number_str) = input.split_once('#')?;
    let number: u64 = number_str.parse().ok()?;
    let (owner, repo) = repo_part.split_once('/')?;
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some(PrRef {
        owner: owner.to_string(),
        repo: repo.to_string(),
        number,
    })
}

fn resolve_from_git(number: u64, default_org: &str) -> Result<PrRef> {
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
        .map_err(|e| Error::Other(format!("git not available: {e}")))?;
    if !output.status.success() {
        return Err(Error::Other(
            "cwd is not a git repo — pass a full URL or `owner/repo#N` instead".to_string(),
        ));
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let (owner, repo) = parse_git_remote(&url).ok_or_else(|| {
        Error::Other(format!(
            "could not parse git remote `{url}` — pass a full URL instead"
        ))
    })?;
    if !default_org.is_empty() && owner != default_org {
        eprintln!(
            "note: repo origin owner `{owner}` does not match configured org `{default_org}`"
        );
    }
    Ok(PrRef {
        owner,
        repo,
        number,
    })
}

/// Parse an `origin` URL (ssh or https) into `(owner, repo)`.
fn parse_git_remote(url: &str) -> Option<(String, String)> {
    // git@github.com:owner/repo.git
    if let Some(rest) = url.strip_prefix("git@github.com:") {
        return split_owner_repo(rest);
    }
    // https://github.com/owner/repo(.git)
    if let Some(rest) = url.strip_prefix("https://github.com/") {
        return split_owner_repo(rest);
    }
    if let Some(rest) = url.strip_prefix("http://github.com/") {
        return split_owner_repo(rest);
    }
    None
}

fn split_owner_repo(rest: &str) -> Option<(String, String)> {
    let rest = rest.trim_end_matches('/').trim_end_matches(".git");
    let (owner, repo) = rest.split_once('/')?;
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner.to_string(), repo.to_string()))
}

/// Humanize a duration in hours into `5m`, `3h`, `2d`, `3mo`.
pub fn humanize_age_hours(hours: f64) -> String {
    if hours < 1.0 {
        let mins = (hours * 60.0).round() as i64;
        format!("{mins}m")
    } else if hours < 24.0 {
        format!("{}h", hours.round() as i64)
    } else if hours < 24.0 * 60.0 {
        format!("{}d", (hours / 24.0).round() as i64)
    } else {
        format!("{}mo", (hours / (24.0 * 30.0)).round() as i64)
    }
}

/// Copy text to the system clipboard. Uses `pbcopy` on macOS and
/// `xclip -selection clipboard` elsewhere.
pub fn copy_to_clipboard(text: &str) -> Result<()> {
    use std::io::Write;
    use std::process::Stdio;

    let mut cmd = if cfg!(target_os = "macos") {
        Command::new("pbcopy")
    } else {
        let mut c = Command::new("xclip");
        c.args(["-selection", "clipboard"]);
        c
    };
    let mut child = cmd
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| Error::Other(format!("failed to spawn clipboard helper: {e}")))?;
    child
        .stdin
        .as_mut()
        .ok_or_else(|| Error::Other("clipboard helper closed stdin".to_string()))?
        .write_all(text.as_bytes())
        .map_err(|e| Error::Other(format!("failed to write to clipboard helper: {e}")))?;
    let status = child
        .wait()
        .map_err(|e| Error::Other(format!("clipboard helper wait failed: {e}")))?;
    if !status.success() {
        return Err(Error::Other("clipboard helper exited non-zero".to_string()));
    }
    Ok(())
}

/// Open a URL in the system browser. Uses `open` on macOS and
/// `xdg-open` elsewhere.
pub fn open_url(url: &str) -> Result<()> {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let status = Command::new(opener)
        .arg(url)
        .status()
        .map_err(|e| Error::Other(format!("failed to launch `{opener}`: {e}")))?;
    if !status.success() {
        return Err(Error::Other(format!("`{opener}` exited non-zero")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_pr_url() {
        let r = parse_url("https://github.com/productiveio/ai-agent/pull/370").unwrap();
        assert_eq!(r.owner, "productiveio");
        assert_eq!(r.repo, "ai-agent");
        assert_eq!(r.number, 370);
    }

    #[test]
    fn parses_owner_repo_number_shorthand() {
        let r = parse_owner_repo_number("productiveio/ai-agent#370").unwrap();
        assert_eq!(r.number, 370);
        assert_eq!(r.repo, "ai-agent");
    }

    #[test]
    fn parses_ssh_git_remote() {
        let (o, r) = parse_git_remote("git@github.com:productiveio/cli-toolbox.git").unwrap();
        assert_eq!(o, "productiveio");
        assert_eq!(r, "cli-toolbox");
    }

    #[test]
    fn parses_https_git_remote_no_dot_git() {
        let (o, r) = parse_git_remote("https://github.com/owner/repo").unwrap();
        assert_eq!(o, "owner");
        assert_eq!(r, "repo");
    }

    #[test]
    fn rejects_non_url_non_number() {
        assert!(parse_pr_ref("garbage", "productiveio").is_err());
    }

    #[test]
    fn humanize_covers_ranges() {
        assert_eq!(humanize_age_hours(0.25), "15m");
        assert_eq!(humanize_age_hours(2.0), "2h");
        assert_eq!(humanize_age_hours(48.0), "2d");
        assert_eq!(humanize_age_hours(24.0 * 90.0), "3mo");
    }
}
