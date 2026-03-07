use crate::config::{Config, ProjectConfig};
use crate::error::Result;

pub fn init(token: &str, org_id: &str) -> Result<()> {
    let config = Config {
        token: token.to_string(),
        org_id: org_id.to_string(),
        projects: Default::default(),
    };
    config.save()?;
    println!("Config written to {:?}", Config::config_path()?);
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

pub fn add_project(config: &mut Config, name: &str, project_id: &str) -> Result<()> {
    config.projects.insert(
        name.to_string(),
        ProjectConfig {
            id: project_id.to_string(),
        },
    );
    config.save()?;
    println!("Added project '{}' ({})", name, project_id);
    Ok(())
}

pub fn remove_project(config: &mut Config, name: &str) -> Result<()> {
    if config.projects.remove(name).is_some() {
        config.save()?;
        println!("Removed project '{}'", name);
    } else {
        let available = config.available_projects().join(", ");
        println!("Project '{}' not found. Available: {}", name, available);
    }
    Ok(())
}
