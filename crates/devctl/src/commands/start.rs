use std::path::Path;

use colored::Colorize;

use crate::config::Config;
use crate::docker;
use crate::error::{Error, Result};
use crate::health;
use crate::state::{ServiceState, State};

pub fn docker(
    config: &Config,
    project_root: &Path,
    services: &[String],
) -> Result<()> {
    // --- Prerequisite: Docker running ---
    if !health::docker_is_running() {
        return Err(Error::Other(
            "Docker is not running. Start Docker Desktop first.".into(),
        ));
    }

    // --- Validate services ---
    for svc in services {
        if !config.services.contains_key(svc) {
            return Err(Error::Config(format!(
                "Unknown service: '{}'. Check devctl.toml.",
                svc
            )));
        }
    }

    // --- Check port conflicts ---
    println!("{}", "Checking ports...".blue());
    let mut conflicts = Vec::new();
    for svc_name in services {
        let svc = &config.services[svc_name];
        if let Some(port) = svc.port
            && health::port_is_open(port) {
                let owner = health::port_owner(port)
                    .map(|(pid, cmd)| format!("{} (PID {})", cmd, pid))
                    .unwrap_or_else(|| "unknown".into());
                conflicts.push(format!("  {} (port {}) — occupied by {}", svc_name, port, owner));
            }
    }
    // Also check companion ports
    for svc_name in services {
        if let Some(companion) = &config.services[svc_name].companion
            && let Some(comp_svc) = config.services.get(companion)
                && let Some(port) = comp_svc.port
                    && health::port_is_open(port) {
                        let owner = health::port_owner(port)
                            .map(|(pid, cmd)| format!("{} (PID {})", cmd, pid))
                            .unwrap_or_else(|| "unknown".into());
                        conflicts.push(format!(
                            "  {} (port {}) — occupied by {}",
                            companion, port, owner
                        ));
                    }
    }
    if !conflicts.is_empty() {
        eprintln!("{}", "Port conflicts detected:".red());
        for c in &conflicts {
            eprintln!("{}", c);
        }
        return Err(Error::Other(
            "Stop conflicting processes before starting.".into(),
        ));
    }

    // --- Ensure repos are cloned ---
    println!("{}", "Checking repos...".blue());
    let repos_dir = project_root.join("repos");
    for svc_name in services {
        let svc = &config.services[svc_name];
        if let Some(repo) = &svc.repo
            && !repos_dir.join(repo).exists() {
                return Err(Error::Config(format!(
                    "Repo not cloned: repos/{}. Run: git clone https://github.com/productiveio/{}.git repos/{}",
                    repo, repo, repo
                )));
            }
    }

    // --- Check secrets ---
    println!("{}", "Checking secrets...".blue());
    let mut missing = Vec::new();
    for svc_name in services {
        let svc = &config.services[svc_name];
        if let Some(repo) = &svc.repo {
            for secret in &svc.secrets {
                let secret_path = repos_dir.join(repo).join(secret);
                if !secret_path.exists() {
                    missing.push(format!("  {}: {} (missing)", svc_name, secret));
                }
            }
        }
    }
    if !missing.is_empty() {
        eprintln!("{}", "Missing secrets:".red());
        for m in &missing {
            eprintln!("{}", m);
        }
        return Err(Error::Other(
            "Pull secrets before starting. See devctl.toml init steps.".into(),
        ));
    }

    // --- Auto-start infra if needed ---
    let infra_compose = project_root.join(&config.infra.compose_file);
    let infra_needed = services.iter().any(|svc_name| {
        !config.services[svc_name].infra.is_empty()
    });

    if infra_needed {
        let infra_running = health::compose_is_running(
            &config.infra.compose_project,
            &infra_compose.to_string_lossy(),
        );
        if !infra_running {
            println!("{}", "Starting infrastructure...".blue());
            crate::commands::infra::up(config, project_root)?;
        } else {
            println!("  Infrastructure already running.");
        }
    }

    // --- Stop existing container if running ---
    if docker::container_is_running(config) {
        println!("{}", "Stopping existing container...".yellow());
        docker::stop_container(config, project_root)?;
    }

    // --- Capture env vars ---
    println!("{}", "Capturing environment...".blue());
    capture_env(project_root)?;

    // --- Generate Procfile ---
    println!("{}", "Generating Procfile...".blue());
    docker::generate_procfile(config, services, project_root)?;

    // --- Start container ---
    println!("{}", "Starting container...".blue());
    docker::start_container(config, project_root, services)?;

    // --- Wait for healthy ---
    print!("{}", "Waiting for container to be ready".blue());
    docker::wait_for_healthy(config)?;
    println!(" {}", "ready!".green());

    // --- Update state ---
    let now = chrono::Utc::now().to_rfc3339();
    let mut state = State::load(project_root)?;
    // Clear previous docker services
    state.services.retain(|_, s| s.mode != "docker");
    for svc_name in services {
        state.services.insert(
            svc_name.clone(),
            ServiceState {
                mode: "docker".into(),
                started_at: now.clone(),
                dir: config.services[svc_name]
                    .repo
                    .as_ref()
                    .map(|r| format!("repos/{}", r)),
                pid: None,
            },
        );
        // Track companions too
        if let Some(companion) = &config.services[svc_name].companion {
            state.services.insert(
                companion.clone(),
                ServiceState {
                    mode: "docker".into(),
                    started_at: now.clone(),
                    dir: config.services.get(companion)
                        .and_then(|s| s.repo.as_ref())
                        .map(|r| format!("repos/{}", r)),
                    pid: None,
                },
            );
        }
    }
    state.save(project_root)?;

    // --- Report ---
    println!();
    println!("{}", "Services started:".green());
    for svc_name in services {
        let svc = &config.services[svc_name];
        if let Some(hostname) = &svc.hostname {
            println!("  https://{}  → port {}", hostname, svc.port.unwrap_or(0));
        }
    }
    println!();
    println!("Branch switch: cd repos/<repo> && git checkout <branch>");
    println!("Then: devctl stop && devctl start <services> --docker");

    Ok(())
}

/// Capture host environment variables to .env.session file.
fn capture_env(project_root: &Path) -> Result<()> {
    let env_dir = project_root.join(".docker-sessions/.dev");
    std::fs::create_dir_all(&env_dir)?;
    let env_file = env_dir.join(".env.session");

    let mut lines = vec!["# Auto-captured from host environment".to_string()];

    let vars = [
        "ANTHROPIC_API_KEY",
        "PRODUCTIVE_AUTH_TOKEN",
        "GITHUB_PERSONAL_ACCESS_TOKEN",
        "BUGSNAG_AUTH_TOKEN",
        "SEMAPHORE_API_TOKEN",
        "GRAFANA_SERVICE_ACCOUNT_TOKEN",
    ];

    for var in &vars {
        let val = std::env::var(var).unwrap_or_default();
        lines.push(format!("{}={}", var, val));
    }

    // GH_TOKEN fallback
    let gh_token = std::env::var("GH_TOKEN")
        .or_else(|_| std::env::var("GITHUB_PERSONAL_ACCESS_TOKEN"))
        .unwrap_or_default();
    lines.push(format!("GH_TOKEN={}", gh_token));

    // AWS
    let aws_region = std::env::var("AWS_DEFAULT_REGION").unwrap_or_else(|_| "eu-central-1".into());
    lines.push(format!("AWS_DEFAULT_REGION={}", aws_region));

    for var in &["AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY", "AWS_SESSION_TOKEN", "AWS_PROFILE"] {
        if let Ok(val) = std::env::var(var)
            && !val.is_empty() {
                lines.push(format!("{}={}", var, val));
            }
    }

    std::fs::write(&env_file, lines.join("\n") + "\n")?;
    Ok(())
}
