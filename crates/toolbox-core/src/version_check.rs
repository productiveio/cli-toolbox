use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

const REPO: &str = "productiveio/cli-toolbox";
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60); // 24 hours
const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Serialize, Deserialize)]
struct CachedCheck {
    latest_version: String,
    checked_at: DateTime<Utc>,
}

/// Check GitHub for a newer version of the tool.
///
/// Prints a message to stderr if an update is available.
/// Never errors — silently returns on any failure.
pub async fn check(tool_name: &str, current_version: &str) {
    let _ = check_inner(tool_name, current_version).await;
}

async fn check_inner(tool_name: &str, current_version: &str) -> Option<()> {
    let cache_path = cache_path(tool_name);
    let cached = read_cache(&cache_path);

    let latest = if let Some(ref c) = cached {
        let age = Utc::now().signed_duration_since(c.checked_at);
        if age.to_std().ok()? < CHECK_INTERVAL {
            c.latest_version.clone()
        } else {
            fetch_and_cache(tool_name, &cache_path).await?
        }
    } else {
        fetch_and_cache(tool_name, &cache_path).await?
    };

    if is_newer(&latest, current_version) {
        eprintln!(
            "Update available: {} {} \u{2192} {} (run: scripts/install.sh {})",
            tool_name, current_version, latest, tool_name
        );
    }

    Some(())
}

async fn fetch_and_cache(tool_name: &str, cache_path: &Path) -> Option<String> {
    let url = format!("https://api.github.com/repos/{}/releases", REPO);

    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent("cli-toolbox")
        .build()
        .ok()?;

    let releases: Vec<GithubRelease> = client.get(&url).send().await.ok()?.json().await.ok()?;

    let prefix = format!("{}-v", tool_name);
    let latest_tag = releases
        .iter()
        .filter(|r| !r.draft && !r.prerelease)
        .find(|r| r.tag_name.starts_with(&prefix))?;

    let version = latest_tag.tag_name.strip_prefix(&prefix)?.to_string();

    let cached = CachedCheck {
        latest_version: version.clone(),
        checked_at: Utc::now(),
    };
    if let Ok(json) = serde_json::to_string(&cached) {
        let _ = std::fs::create_dir_all(cache_path.parent()?);
        let _ = std::fs::write(cache_path, json);
    }

    Some(version)
}

fn read_cache(path: &Path) -> Option<CachedCheck> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn cache_path(tool_name: &str) -> PathBuf {
    let dir = dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(tool_name);
    dir.join("version-check.json")
}

/// Simple semver comparison: returns true if `latest` > `current`.
fn is_newer(latest: &str, current: &str) -> bool {
    let parse = |v: &str| -> Option<(u32, u32, u32)> {
        let parts: Vec<&str> = v.split('.').collect();
        if parts.len() != 3 {
            return None;
        }
        Some((
            parts[0].parse().ok()?,
            parts[1].parse().ok()?,
            parts[2].parse().ok()?,
        ))
    };

    match (parse(latest), parse(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_newer() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(is_newer("0.1.1", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
    }
}
