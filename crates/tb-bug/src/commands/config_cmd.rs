use crate::api::BugsnagClient;
use crate::config::{Config, ProjectConfig};
use crate::error::{Result, TbBugError};

pub async fn init(token: &str, org_id: Option<&str>, project_slugs: Option<&str>) -> Result<()> {
    let tmp_config = Config {
        token: token.to_string(),
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
        for slug in slugs.split(',').map(|s| s.trim()) {
            if let Some(p) = api_projects.iter().find(|p| p.slug == slug) {
                projects.insert(slug.to_string(), ProjectConfig { id: p.id.clone() });
            } else {
                eprintln!("Warning: project '{}' not found, skipping", slug);
            }
        }
    }

    let config = Config {
        token: token.to_string(),
        org_id,
        projects,
    };
    config.save()?;
    println!("Config written to {:?}", Config::config_path()?);

    if config.projects.is_empty() {
        // Show available projects so user knows what to add
        println!("\nAvailable projects (pass --projects to add during init):");
        let mut sorted = api_projects;
        sorted.sort_by(|a, b| b.open_error_count.cmp(&a.open_error_count));
        for p in &sorted {
            println!("  {:<24} {:>6} open errors", p.slug, p.open_error_count);
        }
        println!("\nExample: tb-bug config init --token <TOKEN> --projects api,app,ai-agent");
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

