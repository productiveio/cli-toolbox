use crate::api::BugsnagClient;
use crate::config::{Config, ProjectConfig};
use crate::error::{Result, TbBugError};

pub async fn init(token: &str, org_id: Option<&str>) -> Result<()> {
    let org_id = match org_id {
        Some(id) => id.to_string(),
        None => {
            // Auto-detect org from API
            let tmp_config = Config {
                token: token.to_string(),
                org_id: String::new(),
                projects: Default::default(),
            };
            let client = BugsnagClient::new(&tmp_config, true)?;
            let orgs = client.list_organizations().await?;
            match orgs.len() {
                0 => return Err(TbBugError::Config("No organizations found for this token".into())),
                1 => {
                    println!("Auto-detected organization: {} ({})", orgs[0].name, orgs[0].id);
                    orgs[0].id.clone()
                }
                _ => {
                    eprintln!("Multiple organizations found. Pass --org with one of:");
                    for o in &orgs {
                        eprintln!("  --org {}  ({})", o.id, o.name);
                    }
                    return Err(TbBugError::Config("Multiple organizations found, --org is required".into()));
                }
            }
        }
    };

    let config = Config {
        token: token.to_string(),
        org_id,
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
