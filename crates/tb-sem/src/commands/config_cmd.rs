use std::io::IsTerminal;

use colored::Colorize;
use toolbox_core::prompt::PromptResult;

use crate::api::SemaphoreClient;
use crate::config::{Config, ProjectConfig};
use crate::error::{Result, TbSemError};

fn build_project_options(
    api_projects: &[crate::api::Project],
    existing: Option<&Config>,
) -> (Vec<String>, Vec<usize>) {
    let options: Vec<String> = api_projects
        .iter()
        .map(|p| p.metadata.name.clone())
        .collect();

    let defaults: Vec<usize> = if let Some(cfg) = existing {
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

    (options, defaults)
}

fn resolve_selected_projects(
    selected: Vec<String>,
    api_projects: &[crate::api::Project],
) -> std::collections::HashMap<String, ProjectConfig> {
    let mut map = std::collections::HashMap::new();
    for name in selected {
        if let Some(p) = api_projects.iter().find(|p| p.metadata.name == name) {
            map.insert(
                name,
                ProjectConfig {
                    id: p.metadata.id.clone(),
                },
            );
        }
    }
    map
}

pub async fn init(token: Option<&str>, org_id: Option<&str>) -> Result<()> {
    let existing = Config::load().ok();

    // Resolve token
    let token = match toolbox_core::prompt::prompt_token(
        "Semaphore API token:",
        token,
        existing.as_ref().map(|c| c.token.as_str()),
    ) {
        Ok(PromptResult::Ok(t)) => t,
        Ok(PromptResult::Cancelled) => {
            println!("Cancelled.");
            return Ok(());
        }
        Err(e) => return Err(TbSemError::Config(e)),
    };

    // Resolve org
    let default_org = existing.as_ref().map(|c| c.org_id.as_str()).unwrap_or("");
    let org_id =
        match toolbox_core::prompt::prompt_text("Organization (subdomain):", org_id, default_org) {
            Ok(PromptResult::Ok(o)) => o,
            Ok(PromptResult::Cancelled) => {
                println!("Cancelled.");
                return Ok(());
            }
            Err(e) => return Err(TbSemError::Config(e)),
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
    let project_map = if std::io::stdin().is_terminal() {
        let (options, defaults) = build_project_options(&api_projects, existing.as_ref());

        match toolbox_core::prompt::prompt_multi_select("Select projects:", options, &defaults) {
            Ok(PromptResult::Ok(selected)) => resolve_selected_projects(selected, &api_projects),
            Ok(PromptResult::Cancelled) => {
                println!("Cancelled.");
                return Ok(());
            }
            Err(e) => return Err(TbSemError::Config(e)),
        }
    } else {
        // Non-interactive: add all projects (legacy behavior)
        let all: Vec<String> = api_projects
            .iter()
            .map(|p| p.metadata.name.clone())
            .collect();
        resolve_selected_projects(all, &api_projects)
    };

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

pub async fn set(
    key: &str,
    value: Option<&str>,
    add: Option<&str>,
    remove: Option<&str>,
) -> Result<()> {
    if key == "project" {
        return set_project(value, add, remove).await;
    }

    if add.is_some() || remove.is_some() {
        return Err(TbSemError::Config(
            "--add and --remove are only valid with key 'project'".into(),
        ));
    }

    let value =
        value.ok_or_else(|| TbSemError::Config(format!("Value is required for key '{}'", key)))?;

    match key {
        "token" | "org_id" | "timezone" => {}
        _ => {
            return Err(TbSemError::Config(format!(
                "Unknown config key '{}'. Valid keys: token, org_id, timezone, project",
                key
            )));
        }
    }

    let path = Config::config_path()?;
    toolbox_core::config::patch_toml(&path, key, value)
        .map_err(|e| TbSemError::Config(e.to_string()))?;
    println!("Set {} = {}", key.bold(), value);
    Ok(())
}

async fn set_project(value: Option<&str>, add: Option<&str>, remove: Option<&str>) -> Result<()> {
    let mut cfg = Config::load()?;

    // `config set project <name>` — same as --add
    if let Some(name) = value.or(add) {
        let client = SemaphoreClient::new(&cfg);
        let api_projects = client.list_projects().await?;
        let project = api_projects
            .iter()
            .find(|p| p.metadata.name == name)
            .ok_or_else(|| TbSemError::Config(format!("Project '{}' not found", name)))?;
        cfg.projects.insert(
            name.to_string(),
            ProjectConfig {
                id: project.metadata.id.clone(),
            },
        );
        cfg.save()?;
        println!("Added project: {}", name.bold());
        return Ok(());
    }

    if let Some(name) = remove {
        if cfg.projects.remove(name).is_some() {
            cfg.save()?;
            println!("Removed project: {}", name.bold());
        } else {
            return Err(TbSemError::Config(format!(
                "Project '{}' not in config. Configured: {}",
                name,
                cfg.projects.keys().cloned().collect::<Vec<_>>().join(", ")
            )));
        }
        return Ok(());
    }

    // Interactive multi-select
    let client = SemaphoreClient::new(&cfg);
    let api_projects = client.list_projects().await?;
    let (options, defaults) = build_project_options(&api_projects, Some(&cfg));

    match toolbox_core::prompt::prompt_multi_select("Select projects:", options, &defaults) {
        Ok(PromptResult::Ok(selected)) => {
            cfg.projects = resolve_selected_projects(selected, &api_projects);
            cfg.save()?;
            println!("Updated projects:");
            for (name, proj) in &cfg.projects {
                println!("  {:<20} {}", name, proj.id);
            }
            if cfg.projects.is_empty() {
                println!("  (none)");
            }
        }
        Ok(PromptResult::Cancelled) => {
            println!("Cancelled.");
        }
        Err(e) => return Err(TbSemError::Config(e)),
    }

    Ok(())
}
