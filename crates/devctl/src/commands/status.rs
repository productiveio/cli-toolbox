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

    match health::aws_sso_status() {
        health::AwsSsoStatus::Valid(Some(remaining)) if remaining.as_secs() < 1800 => {
            println!(
                "{} AWS SSO expires in {}",
                "!".yellow(),
                health::format_duration(&remaining)
            );
        }
        health::AwsSsoStatus::Expired => {
            println!("{} AWS SSO expired (run: aws sso login)", "!".yellow());
        }
        _ => {} // Valid with plenty of time, or not installed — don't clutter
    }

    // If container is running, get overmind status for accurate service state
    let container_up = docker::container_is_running(config);
    let overmind = if container_up {
        docker::overmind_status(config)
    } else {
        Default::default()
    };

    // Collect rows first, then format with correct alignment
    let mut rows: Vec<(String, String, String, String, String)> = Vec::new();

    for (name, svc) in &config.services {
        let mode = if let Some(svc_state) = state.services.get(name) {
            svc_state.mode.clone()
        } else {
            "-".to_string()
        };

        let (state_text, state_color) =
            determine_service_state(name, svc.port, &mode, &overmind, container_up);
        let url = svc.hostname.as_deref().unwrap_or("-").to_string();

        rows.push((name.clone(), mode, state_text, state_color, url));
    }

    // Print service table with padding applied before colorization
    println!();
    println!("  {:<20} {:<10} {:<10} URL", "SERVICE", "MODE", "STATE");
    println!("  {:<20} {:<10} {:<10} ───", "───────", "────", "─────");

    for (name, mode, state_text, state_color, url) in &rows {
        let padded_state = format!("{:<10}", state_text);
        let colored_state = match state_color.as_str() {
            "green" => padded_state.green().to_string(),
            "red" => padded_state.red().to_string(),
            "yellow" => padded_state.yellow().to_string(),
            _ => padded_state.dimmed().to_string(),
        };
        println!("  {:<20} {:<10} {} {}", name, mode, colored_state, url);
    }

    // Infra status
    let infra_running = health::infra_is_running(config, project_root);
    let infra_compose = project_root.join(&config.infra.compose_file);

    let infra_containers = if infra_running {
        health::compose_container_states(
            &config.infra.compose_project,
            &infra_compose.to_string_lossy(),
        )
    } else {
        Default::default()
    };

    println!();
    println!("  {:<20} {:<10}", "INFRA", "STATE");
    println!("  {:<20} {:<10}", "─────", "─────");

    for name in config.infra.services.keys() {
        let is_up = infra_containers
            .get(name.as_str())
            .is_some_and(|s| s.starts_with("Up"));
        let padded = format!("{:<10}", if is_up { "running" } else { "stopped" });
        let colored = if is_up {
            padded.green().to_string()
        } else {
            padded.red().to_string()
        };
        println!("  {:<20} {}", name, colored);
    }

    println!();
    Ok(())
}

/// Returns (display_text, color_name) for a service state.
fn determine_service_state(
    name: &str,
    port: Option<u16>,
    mode: &str,
    overmind: &std::collections::BTreeMap<String, String>,
    container_up: bool,
) -> (String, String) {
    // Docker mode: use overmind as source of truth
    if mode == "docker" && container_up {
        return match overmind.get(name).map(|s| s.as_str()) {
            Some("running") => ("running".into(), "green".into()),
            Some("dead") => ("crashed".into(), "red".into()),
            Some(other) => (other.into(), "yellow".into()),
            None => ("no proc".into(), "dim".into()),
        };
    }

    // No mode set
    if mode == "-" {
        if let Some(port) = port
            && !container_up
            && health::port_is_open(port)
        {
            return ("external".into(), "yellow".into());
        }
        return ("-".into(), "dim".into());
    }

    // Local mode (future): probe port
    if let Some(port) = port
        && health::port_is_open(port)
    {
        return ("running".into(), "green".into());
    }

    ("stopped".into(), "red".into())
}
