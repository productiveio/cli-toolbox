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
        let dir = dirs::config_dir()
            .ok_or_else(|| TbProdError::Config("cannot determine config directory".into()))?;
        Ok(dir.join("tb-prod/config.toml"))
    }

    pub fn load() -> Result<Self> {
        let mut config: Option<Config> = None;

        // Try monorepo secrets.toml with [productive] section
        let secrets_path = PathBuf::from("secrets.toml");
        if secrets_path.exists() {
            let content = std::fs::read_to_string(&secrets_path)?;
            let table: toml::Table = toml::from_str(&content)?;
            if let Some(section) = table.get("productive") {
                config = Some(section.clone().try_into().map_err(|e: toml::de::Error| {
                    TbProdError::Config(format!("invalid [productive] section: {}", e))
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
            TbProdError::Config(
                "No config found. Run `tb-prod config init` or create secrets.toml with [productive] section".into(),
            )
        })?;

        // Env var overrides
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
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    pub fn base_url(&self) -> &str {
        self.api_base_url
            .as_deref()
            .unwrap_or("https://api.productive.io/api/v2")
    }

    pub fn masked_token(&self) -> String {
        if self.token.len() > 8 {
            format!("****...{}", &self.token[self.token.len() - 4..])
        } else {
            "****".to_string()
        }
    }
}
