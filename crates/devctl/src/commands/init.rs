use std::path::Path;
use std::process::Command;

use colored::Colorize;

use crate::config::Config;
use crate::docker;
use crate::error::{Error, Result};

pub fn run(config: &Config, project_root: &Path, service: &str) -> Result<()> {
    let svc = config.services.get(service).ok_or_else(|| {
        Error::Config(format!("Unknown service: '{}'", service))
    })?;

    if svc.init.is_empty() {
        println!("No init steps defined for '{}'.", service);
        return Ok(());
    }

    let repo = svc.repo.as_deref().ok_or_else(|| {
        Error::Config(format!("Service '{}' has no repo defined", service))
    })?;

    // Determine execution context
    let container_up = docker::container_is_running(config);

    println!("{} {}", "Initializing".blue(), service.bold());
    println!("  Steps: {}", svc.init.len());
    println!();

    for (i, step) in svc.init.iter().enumerate() {
        println!(
            "  [{}/{}] {}",
            i + 1,
            svc.init.len(),
            step.bold()
        );

        if container_up {
            // Run inside Docker container
            let status = Command::new("docker")
                .args([
                    "exec",
                    "-u",
                    "dev",
                    "-w",
                    &format!("/workspace/{}", repo),
                    &config.docker.container,
                    "bash",
                    "-lc",
                    step,
                ])
                .status()?;

            if !status.success() {
                return Err(Error::Other(format!(
                    "Init step failed: {} (exit {})",
                    step,
                    status.code().unwrap_or(-1)
                )));
            }
        } else {
            // Run on host in repos/<repo>
            let repo_dir = project_root.join("repos").join(repo);
            if !repo_dir.exists() {
                return Err(Error::Config(format!(
                    "Repo not found: repos/{}",
                    repo
                )));
            }

            let status = Command::new("bash")
                .args(["-lc", step])
                .current_dir(&repo_dir)
                .status()?;

            if !status.success() {
                return Err(Error::Other(format!(
                    "Init step failed: {} (exit {})",
                    step,
                    status.code().unwrap_or(-1)
                )));
            }
        }
    }

    println!();
    println!("{} {} initialized.", "✓".green(), service);
    Ok(())
}
