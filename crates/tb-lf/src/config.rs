use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::api::DevPortalClient;
use crate::cache::CacheTtl;
use crate::error::{TbLfError, Result};
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
        let mut config: Option<Config> = None;

        // Try secrets.toml [devportal] section
        let secrets_path = PathBuf::from("secrets.toml");
        if secrets_path.exists() {
            let content = std::fs::read_to_string(&secrets_path)?;
            let table: toml::Table = toml::from_str(&content)?;
            if let Some(section) = table.get("devportal") {
                config = Some(section.clone().try_into().map_err(|e: toml::de::Error| {
                    TbLfError::Config(format!("invalid [devportal] section: {}", e))
                })?);
            }
        }

        // Fall back to ~/.config/tb-lf/config.toml
        if config.is_none() {
            let path = Self::config_path()?;
            if path.exists() {
                let content = std::fs::read_to_string(&path)?;
                config = Some(toml::from_str(&content)?);
            }
        }

        let mut config = config.ok_or_else(|| {
            TbLfError::Config(
                "No config found. Add [devportal] to secrets.toml or create ~/.config/tb-lf/config.toml".into(),
            )
        })?;

        // Env var overrides
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
        let dir = dirs::config_dir()
            .ok_or_else(|| TbLfError::Config("cannot determine config directory".into()))?;
        Ok(dir.join("tb-lf/config.toml"))
    }

    pub fn base_api_url(&self) -> String {
        format!("{}/spa_api/ai", self.url)
    }

    pub fn masked_token(&self) -> String {
        if self.token.len() > 8 {
            format!("****...{}", &self.token[self.token.len() - 4..])
        } else {
            "****".to_string()
        }
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
    let projects: Vec<Project> = client.get("/projects", CacheTtl::Long).await?;
    let matches: Vec<&Project> = projects
        .iter()
        .filter(|p| p.name.eq_ignore_ascii_case(input))
        .collect();

    match matches.len() {
        1 => Ok(Some(matches[0].id)),
        0 => {
            let names: Vec<String> = projects.iter().map(|p| format!("  {} (id: {})", p.name, p.id)).collect();
            Err(TbLfError::Config(format!(
                "Project '{}' not found. Available projects:\n{}",
                input,
                names.join("\n"),
            )))
        }
        _ => {
            let names: Vec<String> = matches.iter().map(|p| format!("  {} (id: {})", p.name, p.id)).collect();
            Err(TbLfError::Config(format!(
                "Ambiguous project '{}'. Matches:\n{}\nUse numeric ID to disambiguate.",
                input,
                names.join("\n"),
            )))
        }
    }
}
