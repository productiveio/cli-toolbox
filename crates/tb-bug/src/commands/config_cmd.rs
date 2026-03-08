use std::fmt;

use colored::Colorize;
use toolbox_core::prompt::PromptResult;

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
        write!(
            f,
            "{:<24} {:>6} open errors",
            self.slug, self.open_error_count
        )
    }
}

fn build_project_options(
    api_projects: &[crate::api::Project],
    existing: Option<&Config>,
) -> (Vec<ProjectOption>, Vec<usize>) {
    let mut options: Vec<ProjectOption> = api_projects
        .iter()
        .map(|p| ProjectOption {
            slug: p.slug.clone(),
            id: p.id.clone(),
            open_error_count: p.open_error_count,
        })
        .collect();
    options.sort_by(|a, b| b.open_error_count.cmp(&a.open_error_count));

    let defaults: Vec<usize> = if let Some(cfg) = existing {
        options
            .iter()
            .enumerate()
            .filter(|(_, o)| cfg.projects.contains_key(&o.slug))
            .map(|(i, _)| i)
            .collect()
    } else {
        vec![]
    };

    (options, defaults)
}

pub async fn init(
    token: Option<&str>,
    org_id: Option<&str>,
    project_slugs: Option<&str>,
) -> Result<()> {
    // Load existing config for pre-filling
    let existing = Config::load().ok();

    // Resolve token
    let token = match toolbox_core::prompt::prompt_token(
        "Bugsnag auth token:",
        token,
        existing.as_ref().map(|c| c.token.as_str()),
    ) {
        Ok(PromptResult::Ok(t)) => t,
        Ok(PromptResult::Cancelled) => {
            println!("Cancelled.");
            return Ok(());
        }
        Err(e) => return Err(TbBugError::Config(e)),
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
    } else {
        let (options, defaults) = build_project_options(&api_projects, existing.as_ref());
        if !options.is_empty() {
            match toolbox_core::prompt::prompt_multi_select("Select projects:", options, &defaults)
            {
                Ok(PromptResult::Ok(selected)) => {
                    for p in selected {
                        projects.insert(p.slug.clone(), ProjectConfig { id: p.id.clone() });
                    }
                }
                Ok(PromptResult::Cancelled) => {
                    println!("Cancelled.");
                    return Ok(());
                }
                Err(e) => return Err(TbBugError::Config(e)),
            }
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

pub async fn set(
    key: &str,
    value: Option<&str>,
    add: Option<&str>,
    remove: Option<&str>,
    config: &Config,
    client: &crate::api::BugsnagClient,
) -> Result<()> {
    if key == "project" {
        return set_project(value, add, remove, config, client).await;
    }

    if add.is_some() || remove.is_some() {
        return Err(TbBugError::Config(
            "--add and --remove are only valid with key 'project'".into(),
        ));
    }

    // Scalar keys require a value
    let value =
        value.ok_or_else(|| TbBugError::Config(format!("Value is required for key '{}'", key)))?;

    match key {
        "token" | "org_id" => {}
        _ => {
            return Err(TbBugError::Config(format!(
                "Unknown config key '{}'. Valid keys: token, org_id, project",
                key
            )));
        }
    }

    let path = Config::config_path()?;
    toolbox_core::config::patch_toml(&path, key, value)
        .map_err(|e| TbBugError::Config(e.to_string()))?;
    println!("Set {} = {}", key.bold(), value);
    Ok(())
}

async fn set_project(
    value: Option<&str>,
    add: Option<&str>,
    remove: Option<&str>,
    config: &Config,
    client: &crate::api::BugsnagClient,
) -> Result<()> {
    let mut cfg = Config::load()?;

    // `config set project <slug>` — same as --add
    if let Some(slug) = value.or(add) {
        let api_projects = client.list_projects(&config.org_id).await?;
        let project = api_projects
            .iter()
            .find(|p| p.slug == slug)
            .ok_or_else(|| TbBugError::Config(format!("Project '{}' not found", slug)))?;
        cfg.projects.insert(
            slug.to_string(),
            ProjectConfig {
                id: project.id.clone(),
            },
        );
        cfg.save()?;
        println!("Added project: {}", slug.bold());
        return Ok(());
    }

    if let Some(slug) = remove {
        if cfg.projects.remove(slug).is_some() {
            cfg.save()?;
            println!("Removed project: {}", slug.bold());
        } else {
            return Err(TbBugError::Config(format!(
                "Project '{}' not in config. Configured: {}",
                slug,
                cfg.available_projects().join(", ")
            )));
        }
        return Ok(());
    }

    // Interactive multi-select
    let api_projects = client.list_projects(&config.org_id).await?;
    let (options, defaults) = build_project_options(&api_projects, Some(&cfg));

    match toolbox_core::prompt::prompt_multi_select("Select projects:", options, &defaults) {
        Ok(PromptResult::Ok(selected)) => {
            cfg.projects.clear();
            for p in selected {
                cfg.projects
                    .insert(p.slug.clone(), ProjectConfig { id: p.id.clone() });
            }
            cfg.save()?;
            println!("Updated projects:");
            for (name, proj) in &cfg.projects {
                println!("  {:<24} {}", name, proj.id);
            }
            if cfg.projects.is_empty() {
                println!("  (none)");
            }
        }
        Ok(PromptResult::Cancelled) => {
            println!("Cancelled.");
        }
        Err(e) => return Err(TbBugError::Config(e)),
    }

    Ok(())
}
