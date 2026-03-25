use std::path::Path;

use colored::Colorize;

use crate::config::Config;
use crate::error::Result;
use crate::health;

pub fn run(config: &Config, project_root: &Path) -> Result<()> {
    let mut issues = 0;

    // --- System checks ---
    println!("{}", "System".bold());

    let docker_ok = health::docker_is_running();
    if docker_ok {
        println!("  {} Docker", "✓".green());
    } else {
        println!("  {} Docker — not running", "✗".red());
        issues += 1;
    }

    let caddy_ok = health::caddy_is_running();
    if caddy_ok {
        println!("  {} Caddy (localhost:2019)", "✓".green());
    } else {
        println!(
            "  {} Caddy — not responding on localhost:2019",
            "✗".red()
        );
        println!("      Run: ./scripts/setup-caddy.sh");
        issues += 1;
    }

    match health::aws_sso_status() {
        health::AwsSsoStatus::Valid(Some(remaining)) => {
            let time_str = health::format_duration(&remaining);
            if remaining.as_secs() < 1800 {
                println!("  {} AWS SSO ({} remaining)", "!".yellow(), time_str);
            } else {
                println!("  {} AWS SSO ({} remaining)", "✓".green(), time_str);
            }
        }
        health::AwsSsoStatus::Valid(None) => {
            println!("  {} AWS SSO (valid, expiry unknown)", "✓".green());
        }
        health::AwsSsoStatus::Expired => {
            println!("  {} AWS SSO — expired or invalid", "!".yellow());
            println!("      Run: aws sso login");
        }
        health::AwsSsoStatus::NotInstalled => {
            println!("  {} AWS CLI not installed", "!".yellow());
        }
    }

    // --- Infrastructure ---
    println!();
    println!("{}", "Infrastructure".bold());

    let compose_file = project_root.join(&config.infra.compose_file);
    let infra_running = health::compose_is_running(
        &config.infra.compose_project,
        &compose_file.to_string_lossy(),
    );

    for (name, svc) in &config.infra.services {
        if infra_running && health::port_is_open(svc.port) {
            println!("  {} {} (port {})", "✓".green(), name, svc.port);
        } else {
            println!("  {} {} (port {}) — not running", "✗".red(), name, svc.port);
            issues += 1;
        }
    }

    // --- Services ---
    println!();
    println!("{}", "Services".bold());

    let repos_dir = project_root.join("repos");

    for (name, svc) in &config.services {
        let mut svc_issues = Vec::new();

        // Repo cloned?
        if let Some(repo) = &svc.repo {
            let repo_path = repos_dir.join(repo);
            if !repo_path.exists() {
                svc_issues.push("repo not cloned".into());
            } else {
                // Secrets present?
                for secret in &svc.secrets {
                    if !repo_path.join(secret).exists() {
                        svc_issues.push(format!("missing {}", secret));
                    }
                }
            }
        }

        // Port conflict with non-devctl process?
        if let Some(port) = svc.port
            && health::port_is_open(port) {
                // Port is in use — could be devctl or something else, just note it
            }

        if svc_issues.is_empty() {
            println!("  {} {}", "✓".green(), name);
        } else {
            println!(
                "  {} {} — {}",
                "✗".red(),
                name,
                svc_issues.join(", ")
            );
            issues += 1;
        }
    }

    // --- Summary ---
    println!();
    if issues == 0 {
        println!("{}", "Everything looks good!".green().bold());
    } else {
        println!(
            "{} {} issue(s) found.",
            "!".yellow().bold(),
            issues
        );
    }

    Ok(())
}
