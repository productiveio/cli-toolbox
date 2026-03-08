use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::api::{DevPortalClient, PaginatedResponse};
use crate::cache::CacheTtl;
use crate::error::{Result, TbLfError};
use crate::types::Project;

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub url: String,
    pub token: String,
    #[serde(default)]
    pub project: Option<String>,
}

impl Config {
    pub fn load() -> Result<Self> {
        // 1. Try secrets.toml [devportal] section
        let config: Option<Config> = toolbox_core::config::load_secrets_section("devportal")
            .map_err(|e| TbLfError::Config(e.to_string()))?;

        // 2. Fall back to ~/.config/tb-lf/config.toml
        let config = match config {
            Some(c) => c,
            None => {
                let path = Self::config_path()?;
                toolbox_core::config::load_standalone(&path)
                    .map_err(|e| TbLfError::Config(e.to_string()))?
                    .ok_or_else(|| TbLfError::Config(
                        "No config found. Run `tb-lf config init --url <URL> --token <TOKEN>` or create ~/.config/tb-lf/config.toml".into(),
                    ))?
            }
        };

        // 3. Env var overrides
        let mut config = config;
        if let Ok(url) = std::env::var("DEVPORTAL_URL") {
            config.url = url;
        }
        if let Ok(token) = std::env::var("DEVPORTAL_TOKEN") {
            config.token = token;
        }

        // Normalize: strip trailing slash
        config.url = config.url.trim_end_matches('/').to_string();

        Ok(config)
    }

    pub fn config_path() -> Result<PathBuf> {
        toolbox_core::config::config_path("tb-lf").map_err(|e| TbLfError::Config(e.to_string()))
    }

    pub fn base_api_url(&self) -> String {
        format!("{}/spa_api/ai", self.url)
    }

    pub fn masked_token(&self) -> String {
        toolbox_core::config::masked_token(&self.token)
    }
}

/// Resolve `--project` flag to a numeric project ID.
/// Accepts a project name (case-insensitive) or numeric ID.
pub async fn resolve_project(
    client: &DevPortalClient,
    flag: Option<&str>,
    default: Option<&str>,
) -> Result<Option<i64>> {
    let input = flag.or(default);
    let Some(input) = input else {
        return Ok(None);
    };

    // If it's a number, use directly
    if let Ok(id) = input.parse::<i64>() {
        return Ok(Some(id));
    }

    // Fetch project list and match by name
    let resp: PaginatedResponse<Project> = client.get("/projects", CacheTtl::Long).await?;
    let projects = resp.data;
    let matches: Vec<&Project> = projects
        .iter()
        .filter(|p| p.name.eq_ignore_ascii_case(input))
        .collect();

    match matches.len() {
        1 => Ok(Some(matches[0].id)),
        0 => {
            let names: Vec<String> = projects
                .iter()
                .map(|p| format!("  {} (id: {})", p.name, p.id))
                .collect();
            Err(TbLfError::Config(format!(
                "Project '{}' not found. Available projects:\n{}",
                input,
                names.join("\n"),
            )))
        }
        _ => {
            let names: Vec<String> = matches
                .iter()
                .map(|p| format!("  {} (id: {})", p.name, p.id))
                .collect();
            Err(TbLfError::Config(format!(
                "Ambiguous project '{}'. Matches:\n{}\nUse numeric ID to disambiguate.",
                input,
                names.join("\n"),
            )))
        }
    }
}
