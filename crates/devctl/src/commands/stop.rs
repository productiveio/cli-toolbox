use std::path::Path;

use colored::Colorize;

use crate::config::Config;
use crate::docker;
use crate::error::{Error, Result};
use crate::state::State;

pub fn run(config: &Config, project_root: &Path) -> Result<()> {
    if !docker::container_is_running(config) {
        println!("{}", "Dev container is not running.".yellow());
        return Ok(());
    }

    println!("{}", "Stopping dev container...".yellow());
    docker::stop_container(config, project_root)?;

    // Clear docker services from state
    let mut state = State::load(project_root)?;
    state.services.retain(|_, s| s.mode != "docker");
    state.save(project_root)?;

    println!("{}", "Dev container stopped.".green());
    Ok(())
}

/// Restart a specific service inside the running container via overmind.
pub fn restart_service(config: &Config, service: &str) -> Result<()> {
    if !docker::container_is_running(config) {
        return Err(Error::Other(
            "Dev container is not running. Start with: devctl start <services> --docker".into(),
        ));
    }

    println!("Restarting {}...", service.bold());
    let status = std::process::Command::new("docker")
        .args(["exec", &config.docker.container, "overmind", "restart", service])
        .status()?;

    if !status.success() {
        return Err(Error::Other(format!("Failed to restart {}", service)));
    }

    println!("{} restarted.", service.green());
    Ok(())
}
