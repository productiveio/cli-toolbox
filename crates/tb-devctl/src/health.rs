use std::net::TcpStream;
use std::process::Command;
use std::time::Duration;

/// Check if a TCP port is listening on localhost.
pub fn port_is_open(port: u16) -> bool {
    TcpStream::connect_timeout(
        &format!("127.0.0.1:{}", port).parse().unwrap(),
        Duration::from_millis(200),
    )
    .is_ok()
}

/// Check if Docker daemon is running.
pub fn docker_is_running() -> bool {
    Command::new("docker")
        .args(["info"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Check if Caddy admin API is responding (localhost:2019).
pub fn caddy_is_running() -> bool {
    port_is_open(2019)
}

/// Check if the shared infrastructure is running.
pub fn infra_is_running(config: &crate::config::Config, project_root: &std::path::Path) -> bool {
    let compose_file = project_root.join(&config.infra.compose_file);
    compose_is_running(
        &config.infra.compose_project,
        &compose_file.to_string_lossy(),
    )
}

/// Check if a Docker compose project has running containers.
pub fn compose_is_running(project: &str, compose_file: &str) -> bool {
    Command::new("docker")
        .args([
            "compose",
            "-p",
            project,
            "-f",
            compose_file,
            "ps",
            "--quiet",
        ])
        .output()
        .is_ok_and(|o| !o.stdout.is_empty())
}

/// Get container states from a docker compose project.
/// Returns a map of service_name → status string (e.g., "Up 7 hours (healthy)").
pub fn compose_container_states(
    project: &str,
    compose_file: &str,
) -> std::collections::BTreeMap<String, String> {
    let mut result = std::collections::BTreeMap::new();

    let output = Command::new("docker")
        .args([
            "compose",
            "-p",
            project,
            "-f",
            compose_file,
            "ps",
            "--format",
            "{{.Service}}\t{{.Status}}",
        ])
        .output();

    if let Ok(output) = output {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if let Some((service, status)) = line.split_once('\t') {
                result.insert(service.to_string(), status.to_string());
            }
        }
    }

    result
}

/// AWS SSO session status.
pub enum AwsSsoStatus {
    /// Valid session with optional time remaining
    Valid(Option<std::time::Duration>),
    /// Session expired or not authenticated
    Expired,
    /// AWS CLI not installed
    NotInstalled,
}

/// Check AWS SSO session validity and remaining time.
pub fn aws_sso_status() -> AwsSsoStatus {
    // First check if aws CLI works
    let ok = Command::new("aws")
        .args(["sts", "get-caller-identity", "--no-cli-pager"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    match ok {
        Err(_) => return AwsSsoStatus::NotInstalled,
        Ok(s) if !s.success() => return AwsSsoStatus::Expired,
        _ => {}
    }

    // Session is valid — try to find expiry from SSO cache
    let remaining = sso_session_remaining();
    AwsSsoStatus::Valid(remaining)
}

/// Convenience check for simple valid/invalid.
pub fn aws_sso_is_valid() -> bool {
    matches!(aws_sso_status(), AwsSsoStatus::Valid(_))
}

/// Read SSO session expiry from ~/.aws/sso/cache/*.json.
/// Returns remaining duration if found.
fn sso_session_remaining() -> Option<std::time::Duration> {
    let cache_dir = dirs::home_dir()?.join(".aws/sso/cache");
    if !cache_dir.exists() {
        return None;
    }

    let mut newest_expiry: Option<chrono::DateTime<chrono::Utc>> = None;
    let mut newest_mtime = std::time::SystemTime::UNIX_EPOCH;

    let Ok(entries) = std::fs::read_dir(&cache_dir) else {
        return None;
    };
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "json") {
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            // Only consider files with an accessToken (SSO session files)
            if !content.contains("accessToken") {
                continue;
            }
            let Some(mtime) = entry.metadata().ok().and_then(|m| m.modified().ok()) else {
                continue;
            };
            if mtime > newest_mtime {
                newest_mtime = mtime;
                // Parse expiresAt from JSON
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content)
                    && let Some(expires_at) = json.get("expiresAt").and_then(|v| v.as_str())
                    && let Ok(dt) = expires_at.parse::<chrono::DateTime<chrono::Utc>>()
                {
                    newest_expiry = Some(dt);
                }
            }
        }
    }

    let expiry = newest_expiry?;
    let now = chrono::Utc::now();
    if expiry > now {
        Some((expiry - now).to_std().ok()?)
    } else {
        None // Already expired
    }
}

/// Format a duration as human-readable time remaining.
pub fn format_duration(d: &std::time::Duration) -> String {
    let total_secs = d.as_secs();
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    if hours > 0 {
        format!("{}h {}m", hours, mins)
    } else {
        format!("{}m", mins)
    }
}

/// Result of checking a single requirement.
pub struct RequirementStatus {
    pub ok: bool,
    /// Human-readable detail for the issue line (shown on failure).
    pub detail: Option<String>,
}

/// Check whether a local requirement is satisfied.
pub fn check_requirement(req: &str, repo_path: Option<&std::path::Path>) -> RequirementStatus {
    match req {
        "ruby" => check_ruby(repo_path),
        "node" => check_node(repo_path),
        "python3" => check_python(repo_path),
        "chromium" => check_chromium(),
        _ => check_command(req),
    }
}

// ---------------------------------------------------------------------------
// Ruby
// ---------------------------------------------------------------------------

fn check_ruby(repo_path: Option<&std::path::Path>) -> RequirementStatus {
    let home = dirs::home_dir().unwrap_or_default();

    let manager = if home.join(".rvm").exists() {
        Some("rvm")
    } else if command_exists("rbenv") {
        Some("rbenv")
    } else if command_exists("asdf") && asdf_has_plugin("ruby") {
        Some("asdf")
    } else {
        None
    };

    if manager.is_none() && !command_exists("ruby") {
        return fail("no version manager found (install rvm or rbenv)");
    }

    let version_check =
        repo_path.and_then(|p| check_runtime_version(p, ".ruby-version", manager, "ruby"));
    runtime_result(
        version_check,
        manager,
        manager.is_some() || command_exists("ruby"),
    )
}

// ---------------------------------------------------------------------------
// Node
// ---------------------------------------------------------------------------

fn check_node(repo_path: Option<&std::path::Path>) -> RequirementStatus {
    let home = dirs::home_dir().unwrap_or_default();

    // Detect version manager — n is NOT supported
    let manager = if home.join(".nvm").exists() || std::env::var("NVM_DIR").is_ok() {
        Some("nvm")
    } else if command_exists("fnm") {
        Some("fnm")
    } else if command_exists("volta") {
        Some("volta")
    } else if command_exists("asdf") && asdf_has_plugin("nodejs") {
        Some("asdf")
    } else {
        None
    };

    // n detected as the only tool → hard fail
    if manager.is_none() && command_exists("n") {
        return fail("n is not supported (install nvm or fnm for multi-version)");
    }

    if manager.is_none() && !command_exists("node") {
        return fail("no version manager found (install nvm or fnm)");
    }

    let version_check = repo_path.and_then(|p| {
        check_runtime_version(p, ".node-version", manager, "node")
            .or_else(|| check_runtime_version(p, ".nvmrc", manager, "node"))
    });
    runtime_result(
        version_check,
        manager,
        manager.is_some() || command_exists("node"),
    )
}

// ---------------------------------------------------------------------------
// Python
// ---------------------------------------------------------------------------

fn check_python(repo_path: Option<&std::path::Path>) -> RequirementStatus {
    let manager = if command_exists("pyenv") {
        Some("pyenv")
    } else if command_exists("asdf") && asdf_has_plugin("python") {
        Some("asdf")
    } else {
        None
    };

    if manager.is_none() && !command_exists("python3") {
        return fail("not found");
    }

    let version_check =
        repo_path.and_then(|p| check_runtime_version(p, ".python-version", manager, "python3"));
    runtime_result(version_check, manager, true)
}

// ---------------------------------------------------------------------------
// Chromium
// ---------------------------------------------------------------------------

fn check_chromium() -> RequirementStatus {
    let home = dirs::home_dir().unwrap_or_default();
    let chrome_dir = home.join(".cache/puppeteer/chrome");

    // Check for at least one Chrome binary in the Puppeteer cache
    if chrome_dir.is_dir()
        && let Ok(entries) = std::fs::read_dir(&chrome_dir)
    {
        for entry in entries.flatten() {
            let sub = entry.path();
            if sub.is_dir() && has_chrome_binary(&sub) {
                return RequirementStatus {
                    ok: true,
                    detail: None,
                };
            }
        }
    }

    // Fallback: system chromium
    if command_exists("chromium") {
        return RequirementStatus {
            ok: true,
            detail: None,
        };
    }

    fail("not found (run: npx puppeteer install chrome)")
}

// ---------------------------------------------------------------------------
// Shared runtime result builder
// ---------------------------------------------------------------------------

/// Build a RequirementStatus from a version check result.
/// Used by all three runtime checks (ruby, node, python).
fn runtime_result(
    version_check: Option<(String, bool)>,
    manager: Option<&str>,
    fallback_ok: bool,
) -> RequirementStatus {
    match version_check {
        Some((_version, true)) => RequirementStatus {
            ok: true,
            detail: None,
        },
        Some((version, false)) => RequirementStatus {
            ok: false,
            detail: Some(format!(
                "{} not installed ({})",
                version,
                manager.unwrap_or("no version manager")
            )),
        },
        None => RequirementStatus {
            ok: fallback_ok,
            detail: None,
        },
    }
}

fn fail(detail: &str) -> RequirementStatus {
    RequirementStatus {
        ok: false,
        detail: Some(detail.into()),
    }
}

/// Check if a Puppeteer chrome version directory contains an actual Chrome binary.
fn has_chrome_binary(version_dir: &std::path::Path) -> bool {
    // Structure: <version_dir>/chrome-mac-arm64/Google Chrome for Testing.app/...
    // or: <version_dir>/chrome-linux64/chrome
    if let Ok(entries) = std::fs::read_dir(version_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                // macOS: look for .app bundle
                if let Ok(inner) = std::fs::read_dir(&p) {
                    for inner_entry in inner.flatten() {
                        let name = inner_entry.file_name();
                        let name_str = name.to_string_lossy();
                        if name_str.ends_with(".app") || name_str == "chrome" {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Generic command check
// ---------------------------------------------------------------------------

fn check_command(cmd: &str) -> RequirementStatus {
    if command_exists(cmd) {
        RequirementStatus {
            ok: true,
            detail: None,
        }
    } else {
        fail("not found")
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn command_exists(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn asdf_has_plugin(plugin: &str) -> bool {
    Command::new("asdf")
        .args(["list", plugin])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Read a version file from the repo and check if the version is installed.
/// Returns (wanted_version, is_installed).
fn check_runtime_version(
    repo_path: &std::path::Path,
    version_filename: &str,
    manager: Option<&str>,
    runtime: &str,
) -> Option<(String, bool)> {
    let wanted = read_version_file(repo_path, version_filename)?;

    let installed = match (runtime, manager) {
        // Ruby
        ("ruby", Some("rvm")) => {
            let home = dirs::home_dir()?;
            home.join(".rvm/rubies")
                .join(format!("ruby-{}", wanted))
                .exists()
        }
        ("ruby", Some("rbenv")) => {
            let home = dirs::home_dir()?;
            home.join(".rbenv/versions").join(&wanted).exists()
        }
        ("ruby", Some("asdf")) => {
            let home = dirs::home_dir()?;
            home.join(".asdf/installs/ruby").join(&wanted).exists()
        }

        // Node
        ("node", Some("nvm")) => {
            let nvm_dir = std::env::var("NVM_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| dirs::home_dir().unwrap_or_default().join(".nvm"));
            let v = if wanted.starts_with('v') {
                wanted.clone()
            } else {
                format!("v{}", wanted)
            };
            nvm_dir.join("versions/node").join(&v).exists()
        }
        ("node", Some("fnm")) => {
            let home = dirs::home_dir().unwrap_or_default();
            let v = if wanted.starts_with('v') {
                wanted.clone()
            } else {
                format!("v{}", wanted)
            };
            home.join(".local/share/fnm/node-versions")
                .join(&v)
                .exists()
                || home.join(".fnm/node-versions").join(&v).exists()
        }
        ("node", Some("volta")) => {
            let home = dirs::home_dir().unwrap_or_default();
            let v = wanted.strip_prefix('v').unwrap_or(&wanted);
            home.join(".volta/tools/image/node").join(v).exists()
        }
        ("node", Some("asdf")) => {
            let home = dirs::home_dir().unwrap_or_default();
            home.join(".asdf/installs/nodejs").join(&wanted).exists()
        }

        // Python
        ("python3", Some("pyenv")) => {
            let home = dirs::home_dir()?;
            home.join(".pyenv/versions").join(&wanted).exists()
        }
        ("python3", Some("asdf")) => {
            let home = dirs::home_dir()?;
            home.join(".asdf/installs/python").join(&wanted).exists()
        }

        // Fallback: compare active version
        _ => check_current_version(runtime, &wanted),
    };

    Some((wanted, installed))
}

/// Read a version file, trim whitespace.
fn read_version_file(repo_path: &std::path::Path, filename: &str) -> Option<String> {
    let content = std::fs::read_to_string(repo_path.join(filename)).ok()?;
    let trimmed = content.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Check if the currently active version of a command matches the wanted version.
fn check_current_version(cmd: &str, wanted: &str) -> bool {
    let output = Command::new(cmd).arg("--version").output().ok();
    if let Some(output) = output {
        let version_str = String::from_utf8_lossy(&output.stdout);
        let clean = wanted.strip_prefix('v').unwrap_or(wanted);
        version_str.contains(clean)
    } else {
        false
    }
}

/// Get the PID and command of the process listening on a port.
/// Returns None if no process is found.
pub fn port_owner(port: u16) -> Option<(u32, String)> {
    let output = Command::new("lsof")
        .args(["-i", &format!(":{}", port), "-sTCP:LISTEN", "-n", "-P"])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Skip header line, parse first result
    let line = stdout.lines().nth(1)?;
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 2 {
        let pid: u32 = parts[1].parse().ok()?;
        let cmd = parts[0].to_string();
        Some((pid, cmd))
    } else {
        None
    }
}
