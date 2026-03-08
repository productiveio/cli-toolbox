use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{Result, TbBugError};

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
        toolbox_core::config::config_path("tb-bug").map_err(|e| TbBugError::Config(e.to_string()))
    }

    /// Load config from (first match wins):
    ///   1. secrets.toml [bugsnag] section (monorepo root)
    ///   2. ~/.config/tb-bug/config.toml (standalone)
    ///
    /// Token can be overridden by BUGSNAG_AUTH_TOKEN env var.
    pub fn load() -> Result<Self> {
        // 1. Try secrets.toml [bugsnag] section
        let config: Option<Config> = toolbox_core::config::load_secrets_section("bugsnag")
            .map_err(|e| TbBugError::Config(e.to_string()))?;

        // 2. Fall back to standalone config
        let config = match config {
            Some(c) => c,
            None => {
                let path = Self::config_path()?;
                toolbox_core::config::load_standalone(&path)
                    .map_err(|e| TbBugError::Config(e.to_string()))?
                    .ok_or_else(|| TbBugError::Config(
                        "No config found. Run `tb-bug config init --token <TOKEN>` or create ~/.config/tb-bug/config.toml".into(),
                    ))?
            }
        };

        // 3. Env var override
        let mut config = config;
        if let Ok(token) = std::env::var("BUGSNAG_AUTH_TOKEN") {
            config.token = token;
        }

        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        toolbox_core::config::save_config(&path, self)
            .map_err(|e| TbBugError::Config(e.to_string()))
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
        toolbox_core::config::masked_token(&self.token)
    }
}
