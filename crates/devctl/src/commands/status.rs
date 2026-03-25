use std::path::Path;

use colored::Colorize;

use crate::config::Config;
use crate::docker;
use crate::error::Result;
use crate::health;
use crate::state::State;

pub fn run(config: &Config, project_root: &Path) -> Result<()> {
    let state = State::load(project_root)?;

    // Prerequisite checks
    let docker_ok = health::docker_is_running();
    let caddy_ok = health::caddy_is_running();

    if !docker_ok {
        println!("{} Docker is not running", "✗".red());
    }
    if !caddy_ok {
        println!("{} Caddy is not running (localhost:2019)", "!".yellow());
    }

    // If container is running, get overmind status for accurate service state
    let container_up = docker::container_is_running(config);
    let overmind = if container_up {
        docker::overmind_status(config)
    } else {
        Default::default()
    };

    // Service table header
    println!();
    println!(
        "  {:<20} {:<10} {:<10} {:<30}",
        "SERVICE", "MODE", "STATE", "URL"
    );
    println!(
        "  {:<20} {:<10} {:<10} {:<30}",
        "───────", "────", "─────", "───"
    );

    for (name, svc) in &config.services {
        let mode = if let Some(svc_state) = state.services.get(name) {
            svc_state.mode.clone()
        } else {
            "-".to_string()
        };

        let state_str = determine_service_state(name, svc.port, &mode, &overmind, container_up);

        let url = svc.hostname.as_deref().unwrap_or("-").to_string();

        println!("  {:<20} {:<10} {:<22} {}", name, mode, state_str, url);
    }

    // Infra status
    let compose_file = project_root.join(&config.infra.compose_file);
    let infra_running = health::compose_is_running(
        &config.infra.compose_project,
        &compose_file.to_string_lossy(),
    );

    println!();
    println!("  {:<20} {:<10}", "INFRA", "STATE");
    println!("  {:<20} {:<10}", "─────", "─────");

    let infra_containers = if infra_running {
        health::compose_container_states(
            &config.infra.compose_project,
            &compose_file.to_string_lossy(),
        )
    } else {
        Default::default()
    };

    for (name, _svc) in &config.infra.services {
        let state_str = match infra_containers.get(name.as_str()).map(|s| s.as_str()) {
            Some(s) if s.starts_with("Up") => "running".green().to_string(),
            Some(s) => s.yellow().to_string(),
            None => "stopped".red().to_string(),
        };
        println!("  {:<20} {}", name, state_str);
    }

    println!();
    Ok(())
}

fn determine_service_state(
    name: &str,
    port: Option<u16>,
    mode: &str,
    overmind: &std::collections::BTreeMap<String, String>,
    container_up: bool,
) -> String {
    // Docker mode: use overmind as source of truth
    if mode == "docker" && container_up {
        return match overmind.get(name).map(|s| s.as_str()) {
            Some("running") => "running".green().to_string(),
            Some("dead") => "crashed".red().to_string(),
            Some(other) => other.yellow().to_string(),
            None => "not in procfile".dimmed().to_string(),
        };
    }

    // No mode set: check if something is actually listening on the port
    // but only if the container is NOT running (to avoid false positives
    // from Docker's static port bindings)
    if mode == "-" {
        if let Some(port) = port {
            if !container_up && health::port_is_open(port) {
                // Something external is using this port
                return "running (external)".yellow().to_string();
            }
        }
        return "-".dimmed().to_string();
    }

    // Local mode (future): probe port
    if let Some(port) = port {
        if health::port_is_open(port) {
            return "running".green().to_string();
        }
    }

    "stopped".red().to_string()
}
