use colored::Colorize;

use crate::api::SemaphoreClient;
use crate::config::{Config, ProjectConfig};
use crate::error::{Result, TbSemError};

pub async fn init_with_org(token: &str, org_id: &str) -> Result<()> {
    eprintln!("Verifying token...");

    let config = Config {
        token: token.to_string(),
        org_id: org_id.to_string(),
        timezone: crate::config::default_timezone(),
        projects: Default::default(),
    };

    let client = SemaphoreClient::new(&config);

    // Verify by listing projects
    let projects = client.list_projects().await?;
    eprintln!("Connected! Found {} projects.", projects.len());

    // Auto-add all projects
    let mut project_map = std::collections::HashMap::new();
    for p in &projects {
        let name = p.metadata.name.clone();
        project_map.insert(
            name,
            ProjectConfig {
                id: p.metadata.id.clone(),
            },
        );
    }

    let config = Config {
        token: token.to_string(),
        org_id: org_id.to_string(),
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

