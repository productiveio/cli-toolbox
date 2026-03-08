use colored::Colorize;

use crate::api::SemaphoreClient;
use crate::config::{Config, ProjectConfig};
use crate::error::{Result, TbSemError};

pub async fn init(token: Option<&str>, org_id: Option<&str>) -> Result<()> {
    use inquire::{InquireError, MultiSelect, Password, PasswordDisplayMode, Text};
    use std::io::IsTerminal;

    let interactive = std::io::stdin().is_terminal();
    let existing = Config::load().ok();

    // Resolve token
    let token = match token {
        Some(t) => t.to_string(),
        None if interactive => {
            let mut prompt = Password::new("Semaphore API token:")
                .with_display_mode(PasswordDisplayMode::Masked)
                .without_confirmation();
            if existing.is_some() {
                prompt = prompt.with_help_message("Press Enter to keep existing token");
            }
            match prompt.prompt() {
                Ok(t) if t.is_empty() => {
                    if let Some(ref cfg) = existing {
                        cfg.token.clone()
                    } else {
                        return Err(TbSemError::Config("Token is required".into()));
                    }
                }
                Ok(t) => t,
                Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
                    println!("Cancelled.");
                    return Ok(());
                }
                Err(e) => return Err(TbSemError::Config(e.to_string())),
            }
        }
        None => {
            return Err(TbSemError::Config(
                "Token is required. Use --token or run interactively in a terminal.".into(),
            ));
        }
    };

    // Resolve org
    let default_org = "productive";
    let org_id = match org_id {
        Some(o) => o.to_string(),
        None if interactive => {
            let existing_org = existing
                .as_ref()
                .map(|c| c.org_id.as_str())
                .unwrap_or(default_org);
            match Text::new("Organization (subdomain):").with_default(existing_org).prompt() {
                Ok(o) => o,
                Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
                    println!("Cancelled.");
                    return Ok(());
                }
                Err(e) => return Err(TbSemError::Config(e.to_string())),
            }
        }
        None => default_org.to_string(),
    };

    eprintln!("Verifying token...");
    let config = Config {
        token: token.clone(),
        org_id: org_id.clone(),
        timezone: crate::config::default_timezone(),
        projects: Default::default(),
    };
    let client = SemaphoreClient::new(&config);

    let api_projects = client.list_projects().await?;
    eprintln!("Connected! Found {} projects.", api_projects.len());

    // Project selection
    let mut project_map = std::collections::HashMap::new();
    if interactive {
        let options: Vec<String> = api_projects.iter().map(|p| p.metadata.name.clone()).collect();

        // Pre-check previously configured projects
        let defaults: Vec<usize> = if let Some(ref cfg) = existing {
            options
                .iter()
                .enumerate()
                .filter(|(_, name)| cfg.projects.contains_key(name.as_str()))
                .map(|(i, _)| i)
                .collect()
        } else {
            // Default: select all (same as old behavior)
            (0..options.len()).collect()
        };

        match MultiSelect::new("Select projects:", options)
            .with_default(&defaults)
            .with_page_size(15)
            .with_help_message("Space to toggle, Enter to confirm, type to filter")
            .prompt()
        {
            Ok(selected) => {
                for name in selected {
                    if let Some(p) = api_projects.iter().find(|p| p.metadata.name == name) {
                        project_map.insert(
                            name,
                            ProjectConfig {
                                id: p.metadata.id.clone(),
                            },
                        );
                    }
                }
            }
            Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
                println!("Cancelled.");
                return Ok(());
            }
            Err(e) => return Err(TbSemError::Config(e.to_string())),
        }
    } else {
        // Non-interactive: add all projects (legacy behavior)
        for p in &api_projects {
            project_map.insert(
                p.metadata.name.clone(),
                ProjectConfig {
                    id: p.metadata.id.clone(),
                },
            );
        }
    }

    let config = Config {
        token,
        org_id,
        timezone: crate::config::default_timezone(),
        projects: project_map,
    };

    config.save()?;

    let path = Config::config_path()?;
    eprintln!("Config saved to {}", path.display());
    eprintln!("\nProjects:");
    for (name, proj) in &config.projects {
        eprintln!("  {:<20} {}", name, &proj.id);
    }

    Ok(())
}

pub fn show() -> Result<()> {
    let config = Config::load()?;

    println!("Organization: {}", config.org_id);
    println!("Token: {}", config.masked_token());
    println!("Timezone: {}", config.timezone);
    println!("\nProjects:");
    for (name, proj) in &config.projects {
        println!("  {:<20} {}", name, &proj.id);
    }

    Ok(())
}

pub fn set(key: &str, value: &str) -> Result<()> {
    let path = Config::config_path()?;
    let mut table: toml::Table = if path.exists() {
        let content =
            std::fs::read_to_string(&path).map_err(|e| TbSemError::Config(e.to_string()))?;
        toml::from_str(&content).map_err(|e| TbSemError::Config(e.to_string()))?
    } else {
        toml::Table::new()
    };

    match key {
        "token" | "org_id" | "timezone" => {
            table.insert(key.to_string(), toml::Value::String(value.to_string()));
        }
        _ => {
            return Err(TbSemError::Config(format!(
                "Unknown config key '{}'. Valid keys: token, org_id, timezone",
                key
            )));
        }
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| TbSemError::Config(e.to_string()))?;
    }
    std::fs::write(&path, toml::to_string_pretty(&table).unwrap())
        .map_err(|e| TbSemError::Config(e.to_string()))?;
    println!("Set {} = {}", key.bold(), value);
    Ok(())
}

