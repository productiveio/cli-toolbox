use std::path::Path;

use colored::Colorize;

use crate::config::Config;
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
        let mode;
        let state_str;

        if let Some(svc_state) = state.services.get(name) {
            mode = svc_state.mode.clone();
        } else {
            mode = "-".to_string();
        }

        // Determine actual running state by probing the port
        if let Some(port) = svc.port {
            if health::port_is_open(port) {
                state_str = "running".green().to_string();
            } else if mode != "-" {
                state_str = "stopped".red().to_string();
            } else {
                state_str = "stopped".dimmed().to_string();
            }
        } else {
            // No port (e.g., sidekiq) — can't probe
            if mode != "-" {
                state_str = "running".green().to_string();
            } else {
                state_str = "-".dimmed().to_string();
            }
        }

        let url = svc
            .hostname
            .as_deref()
            .unwrap_or("-")
            .to_string();

        println!(
            "  {:<20} {:<10} {:<22} {}",
            name, mode, state_str, url
        );
    }

    // Infra status
    let compose_file = project_root.join(&config.infra.compose_file);
    let infra_running = health::compose_is_running(
        &config.infra.compose_project,
        &compose_file.to_string_lossy(),
    );

    println!();
    println!(
        "  {:<20} {:<10}",
        "INFRA", "STATE"
    );
    println!(
        "  {:<20} {:<10}",
        "─────", "─────"
    );

    for (name, svc) in &config.infra.services {
        let state_str = if infra_running && health::port_is_open(svc.port) {
            "running".green().to_string()
        } else {
            "stopped".red().to_string()
        };
        println!("  {:<20} {}", name, state_str);
    }

    println!();
    Ok(())
}
