use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{TbBugError, Result};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ProjectConfig {
    pub id: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub token: String,
    pub org_id: String,
    #[serde(default)]
    pub projects: HashMap<String, ProjectConfig>,
}

impl Config {
    pub fn config_path() -> Result<PathBuf> {
        let dir = dirs::config_dir()
            .ok_or_else(|| TbBugError::Config("cannot determine config directory".into()))?;
        Ok(dir.join("tb-bug/config.toml"))
    }

    /// Load config from (first match wins):
    ///   1. secrets.toml [bugsnag] section (monorepo root)
    ///   2. ~/.config/tb-bug/config.toml (standalone)
    ///
    /// Token can be overridden by BUGSNAG_AUTH_TOKEN env var.
    pub fn load() -> Result<Self> {
        let mut config: Option<Config> = None;

        // Try monorepo secrets.toml with [bugsnag] section
        let secrets_path = PathBuf::from("secrets.toml");
        if secrets_path.exists() {
            let content = std::fs::read_to_string(&secrets_path)?;
            let table: toml::Table = toml::from_str(&content)?;
            if let Some(section) = table.get("bugsnag") {
                config = Some(section.clone().try_into().map_err(|e: toml::de::Error| {
                    TbBugError::Config(format!("invalid [bugsnag] section: {}", e))
                })?);
            }
        }

        // Fall back to standalone config
        if config.is_none() {
            let path = Self::config_path().unwrap_or_default();
            if path.exists() {
                let content = std::fs::read_to_string(&path)?;
                config = Some(toml::from_str(&content)?);
            }
        }

        let mut config = config.ok_or_else(|| {
            TbBugError::Config(
                "No config found. Run `tb-bug config init --token <TOKEN> --org <ORG_ID>` or create ~/.config/tb-bug/config.toml".into(),
            )
        })?;

        // Env var overrides file token
        if let Ok(token) = std::env::var("BUGSNAG_AUTH_TOKEN") {
            config.token = token;
        }

        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    pub fn resolve_project<'a>(&'a self, name: &'a str) -> Result<&'a str> {
        if let Some(proj) = self.projects.get(name) {
            return Ok(&proj.id);
        }
        // Check if it looks like a UUID
        if name.len() >= 20 {
            return Ok(name);
        }
        let available = self.available_projects().join(", ");
        Err(TbBugError::Config(format!(
            "Unknown project '{}'. Available: {}",
            name, available
        )))
    }

    pub fn available_projects(&self) -> Vec<&str> {
        self.projects.keys().map(|s| s.as_str()).collect()
    }

    pub fn masked_token(&self) -> String {
        if self.token.len() > 8 {
            format!("****...{}", &self.token[self.token.len() - 4..])
        } else {
            "****".to_string()
        }
    }
}
