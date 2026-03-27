use std::path::Path;

use colored::Colorize;

use crate::config::Config;
use crate::error::Result;
use crate::health;

struct ServiceResult {
    name: String,
    companion_of: Option<String>,
    docker_ok: bool,
    local_ok: bool,
    issues: Vec<String>,
}

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
        println!("  {} Caddy — not responding on localhost:2019", "✗".red());
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

    let infra_running = health::infra_is_running(config, project_root);

    for (name, svc) in &config.infra.services {
        if infra_running && health::port_is_open(svc.port) {
            println!("  {} {} (port {})", "✓".green(), name, svc.port);
        } else {
            println!("  {} {} (port {}) — not running", "✗".red(), name, svc.port);
            issues += 1;
        }
    }

    // --- Services: collect results ---
    let repos_dir = project_root.join("repos");
    let companions = config.companion_map();
    let mut results: Vec<ServiceResult> = Vec::new();

    for (name, svc) in &config.services {
        // Companion services — don't check independently
        if let Some(parent) = companions.get(name.as_str()) {
            results.push(ServiceResult {
                name: name.clone(),
                companion_of: Some(parent.clone()),
                docker_ok: true,
                local_ok: true,
                issues: Vec::new(),
            });
            continue;
        }

        let repo_path = svc.repo.as_ref().map(|r| repos_dir.join(r));
        let repo_exists = repo_path.as_ref().is_some_and(|p| p.exists());

        // Repo not cloned — fail both, single issue, skip rest
        if repo_path.is_some() && !repo_exists {
            results.push(ServiceResult {
                name: name.clone(),
                companion_of: None,
                docker_ok: false,
                local_ok: false,
                issues: vec!["repo not cloned".into()],
            });
            continue;
        }

        let mut svc_issues: Vec<String> = Vec::new();
        let mut docker_issues = false;
        let mut local_issues = false;

        // Secrets check (affects both docker and local)
        if let Some(ref path) = repo_path {
            for secret in &svc.secrets {
                if !path.join(secret).exists() {
                    svc_issues.push(format!("missing {}", secret));
                    docker_issues = true;
                    local_issues = true;
                }
            }
        }

        // Local requirements check (affects local only)
        for req in &svc.requires {
            let check_path = if repo_exists {
                repo_path.as_deref()
            } else {
                None
            };
            let status = health::check_requirement(req, check_path);
            if !status.ok {
                let msg = format!(
                    "{} — {}",
                    req,
                    status.detail.unwrap_or_else(|| "not found".into())
                );
                svc_issues.push(msg);
                local_issues = true;
            }
        }

        results.push(ServiceResult {
            name: name.clone(),
            companion_of: None,
            docker_ok: !docker_issues,
            local_ok: !local_issues,
            issues: svc_issues,
        });
    }

    // --- Services: render table ---
    println!();
    println!("{}", "Services".bold());

    let max_name_len = results
        .iter()
        .map(|r| r.name.len())
        .max()
        .unwrap_or(7)
        .max(7); // minimum "SERVICE" width

    // Header
    println!(
        "  {:<width$}  LOCAL    DOCKER",
        "SERVICE",
        width = max_name_len
    );

    for result in &results {
        if let Some(parent) = &result.companion_of {
            println!(
                "  {:<width$}  (companion of {})",
                result.name,
                parent,
                width = max_name_len
            );
        } else {
            // Pad manually to avoid ANSI codes breaking alignment
            let local_sym = if result.local_ok { "✓" } else { "✗" };
            let docker_sym = if result.docker_ok { "✓" } else { "✗" };
            let local = if result.local_ok {
                local_sym.green().to_string()
            } else {
                local_sym.red().to_string()
            };
            let docker = if result.docker_ok {
                docker_sym.green().to_string()
            } else {
                docker_sym.red().to_string()
            };
            // "LOCAL   " is 8 chars, symbol is 1 visible char, so pad 7 after
            println!(
                "  {:<width$}  {}       {}",
                result.name,
                local,
                docker,
                width = max_name_len
            );
        }
    }

    // --- Issues section ---
    let failing: Vec<&ServiceResult> = results.iter().filter(|r| !r.issues.is_empty()).collect();

    if !failing.is_empty() {
        println!();
        println!("{}", "Issues".bold());

        for (i, result) in failing.iter().enumerate() {
            if i > 0 {
                println!();
            }
            println!("  {}", result.name);
            for issue in &result.issues {
                println!("    {} {}", "✗".red(), issue);
            }
            issues += 1;
        }
    }

    // --- Summary ---
    println!();
    if issues == 0 {
        println!("{}", "Everything looks good!".green().bold());
    } else {
        println!("{} {} issue(s) found.", "!".yellow().bold(), issues);
    }

    Ok(())
}
