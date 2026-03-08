use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{Result, TbProdError};

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub token: String,
    pub org_id: String,
    /// The person_id of the current user (for default assignee filtering).
    #[serde(default)]
    pub person_id: Option<String>,
    /// Override the API base URL (defaults to https://api.productive.io/api/v2).
    #[serde(default)]
    pub api_base_url: Option<String>,
}

impl Config {
    pub fn config_path() -> Result<PathBuf> {
        toolbox_core::config::config_path("tb-prod").map_err(|e| TbProdError::Config(e.to_string()))
    }

    pub fn load() -> Result<Self> {
        // 1. Try secrets.toml [productive] section
        let config: Option<Config> = toolbox_core::config::load_secrets_section("productive")
            .map_err(|e| TbProdError::Config(e.to_string()))?;

        // 2. Fall back to standalone config
        let config = match config {
            Some(c) => c,
            None => {
                let path = Self::config_path()?;
                toolbox_core::config::load_standalone(&path)
                    .map_err(|e| TbProdError::Config(e.to_string()))?
                    .ok_or_else(|| TbProdError::Config(
                        "No config found. Run `tb-prod config init` or create secrets.toml with [productive] section".into(),
                    ))?
            }
        };

        // 3. Env var overrides
        let mut config = config;
        if let Ok(token) = std::env::var("PRODUCTIVE_API_TOKEN") {
            config.token = token;
        }
        if let Ok(org_id) = std::env::var("PRODUCTIVE_ORG_ID") {
            config.org_id = org_id;
        }
        if let Ok(person_id) = std::env::var("PRODUCTIVE_PERSON_ID") {
            config.person_id = Some(person_id);
        }

        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        toolbox_core::config::save_config(&path, self)
            .map_err(|e| TbProdError::Config(e.to_string()))
    }

    pub fn base_url(&self) -> &str {
        self.api_base_url
            .as_deref()
            .unwrap_or("https://api.productive.io/api/v2")
    }

    pub fn masked_token(&self) -> String {
        toolbox_core::config::masked_token(&self.token)
    }
}
