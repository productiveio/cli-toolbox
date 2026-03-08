use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

const REPO: &str = "productiveio/cli-toolbox";
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60); // 24 hours

#[derive(Serialize, Deserialize)]
struct CachedCheck {
    latest_version: String,
    checked_at: DateTime<Utc>,
}

/// Fetch the latest version via `gh` CLI, bypassing cache. Writes result to cache.
/// Returns `None` if `gh` is unavailable or the fetch fails.
pub fn fetch_latest_version(tool_name: &str) -> Option<String> {
    let version = fetch_via_gh(tool_name)?;
    write_cache(tool_name, &version);
    Some(version)
}

/// Check for a newer version using the cache (24h TTL).
/// Returns the latest version string only if an update is available.
/// Returns `None` if cache is missing/expired and `gh` is unavailable, or if already on latest.
pub fn check_cached(tool_name: &str, current_version: &str) -> Option<String> {
    let cache_path = cache_path(tool_name);
    let cached = read_cache(&cache_path);

    let latest = if let Some(ref c) = cached {
        let age = Utc::now().signed_duration_since(c.checked_at);
        if age.to_std().ok()? < CHECK_INTERVAL {
            c.latest_version.clone()
        } else {
            fetch_latest_version(tool_name)?
        }
    } else {
        fetch_latest_version(tool_name)?
    };

    if is_newer(&latest, current_version) {
        Some(latest)
    } else {
        None
    }
}

/// Format the `--version` output line.
///
/// - With latest:    `tb-prod 0.1.4 (latest: 0.1.5 — upgrade available)`
/// - Already latest: `tb-prod 0.1.4 (latest)`
/// - gh unavailable: `tb-prod 0.1.4 (install gh to check for updates)`
pub fn format_version_line(tool_name: &str, current: &str, latest: Option<&str>) -> String {
    match latest {
        Some(v) if is_newer(v, current) => {
            format!(
                "{} {} (latest: {} \u{2014} upgrade available)",
                tool_name, current, v
            )
        }
        Some(_) => {
            format!("{} {} (latest)", tool_name, current)
        }
        None => {
            format!(
                "{} {} (install gh to check for updates)",
                tool_name, current
            )
        }
    }
}

/// Print the `--version` output: current version + latest (or install hint).
/// Always fetches fresh and caches the result.
pub fn print_version(tool_name: &str, current_version: &str) {
    let latest = fetch_latest_version(tool_name);
    println!(
        "{}",
        format_version_line(tool_name, current_version, latest.as_deref())
    );
}

/// Print an update message to stderr if a cached check shows a newer version.
/// Used by `prime` and `doctor` commands. Silent on failure.
pub fn print_update_hint(tool_name: &str, current_version: &str) {
    if let Some(latest) = check_cached(tool_name, current_version) {
        eprintln!("Update available: {} \u{2192} {}", current_version, latest);
    }
}

fn fetch_via_gh(tool_name: &str) -> Option<String> {
    let jq_filter = format!(
        "[.[] | select(.draft == false and .prerelease == false and (.tag_name | startswith(\"{}-v\")))][0].tag_name",
        tool_name
    );

    let output = Command::new("gh")
        .args([
            "api",
            &format!("repos/{}/releases", REPO),
            "--jq",
            &jq_filter,
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let tag = String::from_utf8(output.stdout).ok()?;
    let tag = tag.trim();
    let prefix = format!("{}-v", tool_name);
    tag.strip_prefix(&prefix).map(|v| v.to_string())
}

fn write_cache(tool_name: &str, version: &str) {
    let cache_path = cache_path(tool_name);
    let cached = CachedCheck {
        latest_version: version.to_string(),
        checked_at: Utc::now(),
    };
    if let Ok(json) = serde_json::to_string(&cached) {
        if let Some(parent) = cache_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&cache_path, json);
    }
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

    #[test]
    fn test_format_version_line_upgrade() {
        let line = format_version_line("tb-prod", "0.1.4", Some("0.1.5"));
        assert_eq!(
            line,
            "tb-prod 0.1.4 (latest: 0.1.5 \u{2014} upgrade available)"
        );
    }

    #[test]
    fn test_format_version_line_latest() {
        let line = format_version_line("tb-prod", "0.1.4", Some("0.1.4"));
        assert_eq!(line, "tb-prod 0.1.4 (latest)");
    }

    #[test]
    fn test_format_version_line_no_gh() {
        let line = format_version_line("tb-prod", "0.1.4", None);
        assert_eq!(line, "tb-prod 0.1.4 (install gh to check for updates)");
    }
}
