use std::fmt;
use std::io::IsTerminal;

use colored::Colorize;
use inquire::{InquireError, MultiSelect, Password, PasswordDisplayMode};

use crate::api::BugsnagClient;
use crate::config::{Config, ProjectConfig};
use crate::error::{Result, TbBugError};

struct ProjectOption {
    slug: String,
    id: String,
    open_error_count: u64,
}

impl fmt::Display for ProjectOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:<24} {:>6} open errors", self.slug, self.open_error_count)
    }
}

pub async fn init(
    token: Option<&str>,
    org_id: Option<&str>,
    project_slugs: Option<&str>,
) -> Result<()> {
    let interactive = std::io::stdin().is_terminal();

    // Load existing config for pre-filling
    let existing = Config::load().ok();

    // Resolve token
    let token = match token {
        Some(t) => t.to_string(),
        None if interactive => {
            let mut prompt = Password::new("Bugsnag auth token:")
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
                        return Err(TbBugError::Config("Token is required".into()));
                    }
                }
                Ok(t) => t,
                Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
                    println!("Cancelled.");
                    return Ok(());
                }
                Err(e) => return Err(TbBugError::Config(e.to_string())),
            }
        }
        None => {
            return Err(TbBugError::Config(
                "Token is required. Use --token or run interactively in a terminal.".into(),
            ));
        }
    };

    let tmp_config = Config {
        token: token.clone(),
        org_id: String::new(),
        projects: Default::default(),
    };
    let client = BugsnagClient::new(&tmp_config, true)?;

    // Resolve org
    let org_id = match org_id {
        Some(id) => id.to_string(),
        None => {
            let orgs = client.list_organizations().await?;
            match orgs.len() {
                0 => {
                    return Err(TbBugError::Config(
                        "No organizations found for this token".into(),
                    ));
                }
                1 => {
                    println!("Organization: {} ({})", orgs[0].name, orgs[0].id);
                    orgs[0].id.clone()
                }
                _ => {
                    eprintln!("Multiple organizations found. Pass --org with one of:");
                    for o in &orgs {
                        eprintln!("  --org {}  ({})", o.id, o.name);
                    }
                    return Err(TbBugError::Config(
                        "Multiple organizations found, --org is required".into(),
                    ));
                }
            }
        }
    };

    // Fetch available projects
    let api_projects = client.list_projects(&org_id).await?;

    // Resolve requested projects
    let mut projects = std::collections::HashMap::new();
    if let Some(slugs) = project_slugs {
        // Non-interactive: resolve slugs from flag
        for slug in slugs.split(',').map(|s| s.trim()) {
            if let Some(p) = api_projects.iter().find(|p| p.slug == slug) {
                projects.insert(slug.to_string(), ProjectConfig { id: p.id.clone() });
            } else {
                eprintln!("Warning: project '{}' not found, skipping", slug);
            }
        }
    } else if interactive {
        // Interactive: multi-select
        let mut options: Vec<ProjectOption> = api_projects
            .iter()
            .map(|p| ProjectOption {
                slug: p.slug.clone(),
                id: p.id.clone(),
                open_error_count: p.open_error_count,
            })
            .collect();
        options.sort_by(|a, b| b.open_error_count.cmp(&a.open_error_count));

        // Pre-check previously configured projects
        let defaults: Vec<usize> = if let Some(ref cfg) = existing {
            options
                .iter()
                .enumerate()
                .filter(|(_, o)| cfg.projects.contains_key(&o.slug))
                .map(|(i, _)| i)
                .collect()
        } else {
            vec![]
        };

        match MultiSelect::new("Select projects:", options)
            .with_default(&defaults)
            .with_page_size(15)
            .with_help_message("Space to toggle, Enter to confirm, type to filter")
            .prompt()
        {
            Ok(selected) => {
                for p in selected {
                    projects.insert(p.slug.clone(), ProjectConfig { id: p.id.clone() });
                }
            }
            Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
                println!("Cancelled.");
                return Ok(());
            }
            Err(e) => return Err(TbBugError::Config(e.to_string())),
        }
    }

    let config = Config {
        token,
        org_id,
        projects,
    };
    config.save()?;
    println!("Config written to {:?}", Config::config_path()?);

    if config.projects.is_empty() {
        println!("\nNo projects configured. Use `tb-bug config init` to select interactively.");
    } else {
        println!("\nConfigured projects:");
        for (name, proj) in &config.projects {
            println!("  {:<24} {}", name, proj.id);
        }
    }

    Ok(())
}

pub fn show(config: &Config) {
    println!("Token:   {}", config.masked_token());
    println!("Org ID:  {}", config.org_id);
    println!("\nProjects:");
    if config.projects.is_empty() {
        println!("  (none configured)");
    } else {
        for (name, proj) in &config.projects {
            println!("  {:<20} {}", name, proj.id);
        }
    }
}

pub fn set(key: &str, value: &str) -> Result<()> {
    let path = Config::config_path()?;
    let mut table: toml::Table = if path.exists() {
        let content =
            std::fs::read_to_string(&path).map_err(|e| TbBugError::Config(e.to_string()))?;
        toml::from_str(&content).map_err(|e| TbBugError::Config(e.to_string()))?
    } else {
        toml::Table::new()
    };

    match key {
        "token" | "org_id" => {
            table.insert(key.to_string(), toml::Value::String(value.to_string()));
        }
        _ => {
            return Err(TbBugError::Config(format!(
                "Unknown config key '{}'. Valid keys: token, org_id",
                key
            )));
        }
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| TbBugError::Config(e.to_string()))?;
    }
    std::fs::write(&path, toml::to_string_pretty(&table).unwrap())
        .map_err(|e| TbBugError::Config(e.to_string()))?;
    println!("Set {} = {}", key.bold(), value);
    Ok(())
}

