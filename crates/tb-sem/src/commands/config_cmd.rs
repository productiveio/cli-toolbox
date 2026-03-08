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
                branch: "main".to_string(), // default, user can update
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
        eprintln!("  {:<20} {} (branch: {})", name, &proj.id, proj.branch);
    }
    eprintln!(
        "\nNote: Default branch is 'main' for all projects. \
         Edit {} to set correct branches.",
        path.display()
    );

    Ok(())
}

pub fn show() -> Result<()> {
    let config = Config::load()?;

    println!("Organization: {}", config.org_id);
    println!("Token: {}", config.masked_token());
    println!("Timezone: {}", config.timezone);
    println!("\nProjects:");
    for (name, proj) in &config.projects {
        println!("  {:<20} {} (branch: {})", name, &proj.id, proj.branch);
    }

    Ok(())
}

pub async fn add(name: &str, branch: Option<&str>) -> Result<()> {
    let mut config = Config::load()?;
    let client = SemaphoreClient::new(&config);

    let projects = client.list_projects().await?;
    let found = projects
        .iter()
        .find(|p| p.metadata.name.eq_ignore_ascii_case(name));

    let Some(project) = found else {
        return Err(TbSemError::Config(format!(
            "Project '{}' not found. Available: {}",
            name,
            projects
                .iter()
                .map(|p| p.metadata.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    };

    let branch = branch.unwrap_or("main").to_string();
    println!(
        "Found project '{}' ({}). Branch: {}",
        project.metadata.name, &project.metadata.id, branch
    );

    config.projects.insert(
        name.to_string(),
        ProjectConfig {
            id: project.metadata.id.clone(),
            branch,
        },
    );
    config.save()?;
    println!("Added.");

    Ok(())
}

pub fn remove(name: &str) -> Result<()> {
    let mut config = Config::load()?;

    if config.projects.remove(name).is_none() {
        return Err(TbSemError::Config(format!(
            "Project '{}' not in config.",
            name
        )));
    }

    config.save()?;
    println!("Removed '{}'.", name);

    Ok(())
}
