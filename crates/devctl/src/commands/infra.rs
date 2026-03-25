use std::path::Path;
use std::process::Command;

use colored::Colorize;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::health;

pub fn up(config: &Config, project_root: &Path) -> Result<()> {
    if !health::docker_is_running() {
        return Err(Error::Other("Docker is not running. Start Docker Desktop first.".into()));
    }

    let compose_file = project_root.join(&config.infra.compose_file);

    // Auto-create volumes
    for svc in config.infra.services.values() {
        if let Some(vol) = &svc.volume {
            let exists = Command::new("docker")
                .args(["volume", "inspect", vol])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|s| s.success());

            if !exists {
                println!("  Creating volume: {}", vol.bold());
                let status = Command::new("docker")
                    .args(["volume", "create", vol])
                    .stdout(std::process::Stdio::null())
                    .status()?;
                if !status.success() {
                    return Err(Error::Other(format!("Failed to create volume: {}", vol)));
                }
            }
        }
    }

    println!("{}", "Starting infrastructure...".blue());
    let status = Command::new("docker")
        .args([
            "compose",
            "-p",
            &config.infra.compose_project,
            "-f",
            &compose_file.to_string_lossy(),
            "up",
            "-d",
        ])
        .status()?;

    if !status.success() {
        return Err(Error::Other("docker compose up failed".into()));
    }

    println!("{}", "Infrastructure started.".green());
    for (name, svc) in &config.infra.services {
        println!("  {} → port {}", name.bold(), svc.port);
    }
    Ok(())
}

pub fn down(config: &Config, project_root: &Path) -> Result<()> {
    let compose_file = project_root.join(&config.infra.compose_file);

    println!("{}", "Stopping infrastructure...".yellow());
    let status = Command::new("docker")
        .args([
            "compose",
            "-p",
            &config.infra.compose_project,
            "-f",
            &compose_file.to_string_lossy(),
            "down",
        ])
        .status()?;

    if !status.success() {
        return Err(Error::Other("docker compose down failed".into()));
    }

    println!("{}", "Infrastructure stopped.".green());
    Ok(())
}

pub fn status(config: &Config, project_root: &Path) -> Result<()> {
    if health::infra_is_running(config, project_root) {
        println!("{}", "Infrastructure is running.".green());
    } else {
        println!("{}", "Infrastructure is not running.".red());
        println!("  Start with: devctl infra up");
        return Ok(());
    }

    // Show per-service port status
    for (name, svc) in &config.infra.services {
        let port_status = if health::port_is_open(svc.port) {
            "●".green()
        } else {
            "○".red()
        };
        println!("  {} {} (port {})", port_status, name, svc.port);
    }

    Ok(())
}
