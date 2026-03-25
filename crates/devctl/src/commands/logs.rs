use std::path::Path;
use std::process::Command;

use crate::config::Config;
use crate::docker;
use crate::error::{Error, Result};

pub fn run(config: &Config, project_root: &Path, service: &str) -> Result<()> {
    // Infra services → docker compose logs
    if config.infra.services.contains_key(service) {
        let compose_file = project_root.join(&config.infra.compose_file);
        let status = Command::new("docker")
            .args([
                "compose",
                "-p",
                &config.infra.compose_project,
                "-f",
                &compose_file.to_string_lossy(),
                "logs",
                "-f",
                "--tail",
                "100",
                service,
            ])
            .status()?;

        if !status.success() {
            return Err(Error::Other(format!("Failed to get logs for {}", service)));
        }
        return Ok(());
    }

    // App services → overmind tmux pane capture (non-interactive)
    if !docker::container_is_running(config) {
        return Err(Error::Other(
            "Dev container is not running.".into(),
        ));
    }

    // Find the overmind tmux socket
    let output = Command::new("docker")
        .args([
            "exec",
            "-u",
            "dev",
            &config.docker.container,
            "bash",
            "-c",
            "basename $(ls -d /tmp/overmind-workspace-*/ 2>/dev/null | head -1) 2>/dev/null || echo ''",
        ])
        .output()?;

    let socket = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if socket.is_empty() {
        return Err(Error::Other(
            "Overmind not running in container.".into(),
        ));
    }

    // Capture last 100 lines from tmux pane
    let output = Command::new("docker")
        .args([
            "exec",
            "-u",
            "dev",
            &config.docker.container,
            "tmux",
            "-L",
            &socket,
            "capture-pane",
            "-t",
            &format!("workspace:{}", service),
            "-p",
            "-S",
            "-100",
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Other(format!(
            "Failed to capture logs for '{}': {}",
            service,
            stderr.trim()
        )));
    }

    print!("{}", String::from_utf8_lossy(&output.stdout));
    Ok(())
}
