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

/// Check if a Docker compose project has running containers.
pub fn compose_is_running(project: &str, compose_file: &str) -> bool {
    Command::new("docker")
        .args(["compose", "-p", project, "-f", compose_file, "ps", "--quiet"])
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

/// Check if AWS SSO session is valid by calling sts get-caller-identity.
pub fn aws_sso_is_valid() -> bool {
    Command::new("aws")
        .args(["sts", "get-caller-identity", "--no-cli-pager"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
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
