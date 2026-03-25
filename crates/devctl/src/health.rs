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

    for entry in std::fs::read_dir(&cache_dir).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "json") {
            let content = std::fs::read_to_string(&path).ok()?;
            // Only consider files with an accessToken (SSO session files)
            if !content.contains("accessToken") {
                continue;
            }
            let mtime = entry.metadata().ok()?.modified().ok()?;
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
