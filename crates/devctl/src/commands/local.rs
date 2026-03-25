use std::path::{Path, PathBuf};
use std::process::Command;

use colored::Colorize;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::health;
use crate::state::{ServiceState, State};

pub fn start(
    config: &Config,
    project_root: &Path,
    service: &str,
    dir_override: Option<&str>,
    background: bool,
) -> Result<()> {
    let svc = config
        .services
        .get(service)
        .ok_or_else(|| Error::Config(format!("Unknown service: '{}'", service)))?;

    let cmd = svc
        .cmd
        .as_deref()
        .ok_or_else(|| Error::Config(format!("Service '{}' has no cmd defined", service)))?;

    let repo = svc
        .repo
        .as_deref()
        .ok_or_else(|| Error::Config(format!("Service '{}' has no repo defined", service)))?;

    // Determine service directory
    let svc_dir: PathBuf = if let Some(dir) = dir_override {
        PathBuf::from(dir)
    } else {
        project_root.join("repos").join(repo)
    };

    if !svc_dir.exists() {
        return Err(Error::Config(format!(
            "Service directory not found: {}",
            svc_dir.display()
        )));
    }

    // Check port conflicts
    if let Some(port) = svc.port
        && health::port_is_open(port)
    {
        let owner = health::port_owner(port)
            .map(|(pid, cmd)| format!("{} (PID {})", cmd, pid))
            .unwrap_or_else(|| "unknown".into());
        return Err(Error::Other(format!(
            "Port {} is already in use by {}",
            port, owner
        )));
    }

    // Check secrets
    for secret in &svc.secrets {
        if !svc_dir.join(secret).exists() {
            return Err(Error::Config(format!(
                "Missing secret: {}/{}. Run `devctl init {}` first.",
                svc_dir.display(),
                secret,
                service
            )));
        }
    }

    // Auto-start infra if needed
    if !svc.infra.is_empty() && !health::infra_is_running(config, project_root) {
        println!("{}", "Starting infrastructure...".blue());
        crate::commands::infra::up(config, project_root)?;
    }

    // Run start steps (git pull, deps, migrate)
    if !svc.start.is_empty() {
        println!("{}", "Running setup steps...".blue());
        for step in &svc.start {
            // git pull: skip if working tree is dirty
            if step.starts_with("git pull") {
                let output = Command::new("git")
                    .args(["status", "--porcelain"])
                    .current_dir(&svc_dir)
                    .output()?;
                if !output.stdout.is_empty() {
                    println!("  {} git pull (dirty working tree, skipping)", "!".yellow());
                    continue;
                }
            }

            // git restore: clean up generated files after migrations
            if step.starts_with("git restore") {
                let status = Command::new("bash")
                    .args(["-lc", step])
                    .current_dir(&svc_dir)
                    .status()?;
                if !status.success() {
                    println!("  {} {} (non-fatal)", "!".yellow(), step);
                }
                continue;
            }

            println!("  {}", step);
            let status = Command::new("bash")
                .args(["-lc", step])
                .current_dir(&svc_dir)
                .status()?;

            if !status.success() {
                return Err(Error::Other(format!("Setup step failed: {}", step)));
            }
        }
    }

    // Clean stale PID files
    let pid_file = svc_dir.join("tmp/pids/server.pid");
    if pid_file.exists() {
        std::fs::remove_file(&pid_file)?;
        println!("  Cleaned stale PID file");
    }

    // Start the service
    let now = chrono::Utc::now().to_rfc3339();

    if background {
        // Background mode: redirect output to log file
        let log_dir = project_root.join(".devctl/logs");
        std::fs::create_dir_all(&log_dir)?;
        let log_file = log_dir.join(format!("{}.log", service));
        let log = std::fs::File::create(&log_file)?;

        println!(
            "{} {} (background, logs: {})",
            "Starting".blue(),
            service.bold(),
            log_file.display()
        );

        let child = Command::new("bash")
            .args(["-lc", cmd])
            .current_dir(&svc_dir)
            .stdout(log.try_clone()?)
            .stderr(log)
            .spawn()?;

        // Update state with PID
        let mut state = State::load(project_root)?;
        state.services.insert(
            service.to_string(),
            ServiceState {
                mode: "local".into(),
                started_at: now,
                dir: Some(svc_dir.to_string_lossy().into()),
                pid: Some(child.id()),
            },
        );
        state.save(project_root)?;

        println!("{} {} started (PID {})", "✓".green(), service, child.id());
        if let Some(hostname) = &svc.hostname {
            println!("  https://{}", hostname);
        }
    } else {
        // Foreground mode: inherit terminal
        println!(
            "{} {} (foreground, Ctrl+C to stop)",
            "Starting".blue(),
            service.bold()
        );

        // Update state before starting (no PID for foreground)
        let mut state = State::load(project_root)?;
        state.services.insert(
            service.to_string(),
            ServiceState {
                mode: "local".into(),
                started_at: now,
                dir: Some(svc_dir.to_string_lossy().into()),
                pid: None,
            },
        );
        state.save(project_root)?;

        let status = Command::new("bash")
            .args(["-lc", cmd])
            .current_dir(&svc_dir)
            .status()?;

        // Clean up state after exit
        let mut state = State::load(project_root)?;
        state.services.remove(service);
        state.save(project_root)?;

        if !status.success() {
            return Err(Error::Other(format!(
                "{} exited with code {}",
                service,
                status.code().unwrap_or(-1)
            )));
        }
    }

    Ok(())
}

/// Stop a locally running service by PID.
pub fn stop(project_root: &Path, service: &str) -> Result<()> {
    let mut state = State::load(project_root)?;

    let svc_state = state
        .services
        .get(service)
        .ok_or_else(|| Error::Other(format!("Service '{}' is not tracked in state", service)))?;

    if svc_state.mode != "local" {
        return Err(Error::Other(format!(
            "Service '{}' is in {} mode, not local",
            service, svc_state.mode
        )));
    }

    if let Some(pid) = svc_state.pid {
        println!("Stopping {} (PID {})...", service, pid);
        let _ = Command::new("kill").arg(pid.to_string()).status();
        std::thread::sleep(std::time::Duration::from_secs(2));
        println!("{} stopped.", service.green());
    } else {
        println!(
            "{} {} was running in foreground (no PID tracked)",
            "!".yellow(),
            service
        );
    }

    state.services.remove(service);
    state.save(project_root)?;
    Ok(())
}
