use std::path::Path;
use std::process::Command;

use crate::config::{Config, ServiceConfig};
use crate::error::{Error, Result};

/// Generate a Procfile for overmind from the selected services.
/// Writes to `.docker-sessions/.dev/Procfile.dev`.
pub fn generate_procfile(
    config: &Config,
    services: &[String],
    project_root: &Path,
) -> Result<()> {
    let procfile_dir = project_root.join(".docker-sessions/.dev");
    std::fs::create_dir_all(&procfile_dir)?;
    let procfile_path = procfile_dir.join("Procfile.dev");

    let mut lines = Vec::new();

    for svc_name in services {
        let svc = config.services.get(svc_name).ok_or_else(|| {
            Error::Config(format!("Unknown service: {}", svc_name))
        })?;

        if let Some(entry) = procfile_entry(svc_name, svc, config, project_root) {
            lines.push(entry);
        }

        // Add companion (e.g., sidekiq for api)
        if let Some(companion) = &svc.companion
            && let Some(comp_svc) = config.services.get(companion)
                && let Some(entry) = procfile_entry(companion, comp_svc, config, project_root) {
                    lines.push(entry);
                }
    }

    std::fs::write(&procfile_path, lines.join("\n") + "\n")?;
    Ok(())
}

/// Build a single Procfile entry, with runtime version wrappers if needed.
fn procfile_entry(
    name: &str,
    svc: &ServiceConfig,
    _config: &Config,
    project_root: &Path,
) -> Option<String> {
    let repo = svc.repo.as_deref()?;
    let cmd = svc.cmd.as_deref()?;

    let repos_dir = project_root.join("repos");
    let mut wrapper = String::new();

    // Check if repo needs a different Ruby version
    let ruby_version_file = repos_dir.join(repo).join(".ruby-version");
    if ruby_version_file.exists()
        && let Ok(version) = std::fs::read_to_string(&ruby_version_file) {
            let version = version.trim();
            let default_ruby = "3.4.7"; // matches Dockerfile.base ARG
            if version != default_ruby {
                wrapper.push_str(&format!("rvm use {} && ", version));
            }
        }

    // Check if repo needs a different Node version
    let node_version = read_node_version(&repos_dir.join(repo));
    if let Some(version) = node_version {
        let default_node = "22.16.0"; // matches Dockerfile.base ARG
        if version != default_node {
            wrapper.push_str(&format!(". /usr/local/nvm/nvm.sh && nvm use {} && ", version));
        }
    }

    let full_cmd = if wrapper.is_empty() {
        format!("{}: cd /workspace/{} && {}", name, repo, cmd)
    } else {
        format!(
            "{}: bash -lc '{} cd /workspace/{} && {}'",
            name, wrapper, repo, cmd
        )
    };

    Some(full_cmd)
}

/// Read Node version from .node-version or .nvmrc
fn read_node_version(repo_path: &Path) -> Option<String> {
    for filename in &[".node-version", ".nvmrc"] {
        let path = repo_path.join(filename);
        if path.exists()
            && let Ok(version) = std::fs::read_to_string(&path) {
                return Some(version.trim().to_string());
            }
    }
    None
}

/// Query overmind inside the container to get running service names and their status.
/// Returns a map of service_name → "running" | "stopped" | "dead".
pub fn overmind_status(config: &Config) -> std::collections::BTreeMap<String, String> {
    let mut result = std::collections::BTreeMap::new();

    let output = Command::new("docker")
        .args([
            "exec",
            &config.docker.container,
            "overmind",
            "status",
        ])
        .output();

    let Ok(output) = output else {
        return result;
    };

    // overmind status output:
    // PROCESS   PID       STATUS
    // api       5796      running
    // sidekiq   5797      running
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines().skip(1) {
        // Skip header
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let name = parts[0].to_string();
            let status = parts[2].to_string();
            result.insert(name, status);
        }
    }

    result
}

/// Check if the dev container is currently running.
pub fn container_is_running(config: &Config) -> bool {
    Command::new("docker")
        .args([
            "ps",
            "--filter",
            &format!("name={}", config.docker.container),
            "--format",
            "{{.Status}}",
        ])
        .output()
        .is_ok_and(|o| !o.stdout.is_empty())
}

/// Stop the dev container.
pub fn stop_container(config: &Config, project_root: &Path) -> Result<()> {
    let compose_file = project_root.join(&config.docker.compose_file);
    let status = Command::new("docker")
        .args([
            "compose",
            "-p",
            &config.docker.compose_project,
            "-f",
            &compose_file.to_string_lossy(),
            "down",
        ])
        .status()?;

    if !status.success() {
        return Err(Error::Other("Failed to stop dev container".into()));
    }
    Ok(())
}

/// Start the dev container.
pub fn start_container(
    config: &Config,
    project_root: &Path,
    services: &[String],
) -> Result<()> {
    let compose_file = project_root.join(&config.docker.compose_file);

    let selected_repos = services.join(",");

    let status = Command::new("docker")
        .args([
            "compose",
            "-p",
            &config.docker.compose_project,
            "-f",
            &compose_file.to_string_lossy(),
            "up",
            "-d",
        ])
        .env("SELECTED_REPOS", &selected_repos)
        .status()?;
    if !status.success() {
        return Err(Error::Other("Failed to start dev container".into()));
    }
    Ok(())
}

/// Wait for the container healthcheck to pass.
pub fn wait_for_healthy(config: &Config) -> Result<()> {
    let container = &config.docker.container;
    for i in 0..60 {
        let output = Command::new("docker")
            .args(["inspect", "--format", "{{.State.Health.Status}}", container])
            .output()?;

        let status = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if status == "healthy" {
            return Ok(());
        }

        if i % 5 == 0 && i > 0 {
            eprint!(".");
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }

    Err(Error::Other(
        "Container did not become healthy within 2 minutes".into(),
    ))
}
