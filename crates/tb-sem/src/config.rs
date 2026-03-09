use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{Result, TbSemError};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ProjectConfig {
    pub id: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub token: String,
    pub org_id: String,
    #[serde(default = "default_timezone")]
    pub timezone: String,
    #[serde(default)]
    pub projects: HashMap<String, ProjectConfig>,
}

pub fn default_timezone() -> String {
    iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".to_string())
}

impl Config {
    pub fn config_path() -> Result<PathBuf> {
        toolbox_core::config::config_path("tb-sem").map_err(|e| TbSemError::Config(e.to_string()))
    }

    /// Load config from (first match wins):
    ///   1. secrets.toml [semaphore] section (monorepo root)
    ///   2. ~/.config/tb-sem/config.toml (standalone)
    ///
    /// Token can be overridden by SEMAPHORE_API_TOKEN env var.
    pub fn load() -> Result<Self> {
        // 1. Try secrets.toml [semaphore] section
        let config: Option<Config> = toolbox_core::config::load_secrets_section("semaphore")
            .map_err(|e| TbSemError::Config(e.to_string()))?;

        // 2. Fall back to standalone config
        let config = match config {
            Some(c) => c,
            None => {
                let path = Self::config_path()?;
                toolbox_core::config::load_standalone(&path)
                    .map_err(|e| TbSemError::Config(e.to_string()))?
                    .ok_or_else(|| TbSemError::Config(
                        "No config found. Run `tb-sem config init --token <TOKEN> --org <ORG>` or create ~/.config/tb-sem/config.toml".into(),
                    ))?
            }
        };

        // 3. Env var override
        let mut config = config;
        if let Ok(token) = std::env::var("SEMAPHORE_API_TOKEN") {
            config.token = token;
        }

        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        toolbox_core::config::save_config(&path, self)
            .map_err(|e| TbSemError::Config(e.to_string()))
    }

    pub fn base_url(&self) -> String {
        format!("https://{}.semaphoreci.com/api/v1alpha", self.org_id)
    }

    /// Resolve a project name to its project_id.
    /// Accepts either an alias from config or a raw UUID.
    pub fn resolve_project(&self, name: &str) -> Result<&str> {
        if let Some(proj) = self.projects.get(name) {
            return Ok(&proj.id);
        }
        // Check if it looks like a UUID (contains dashes, len >= 36)
        if name.contains('-') && name.len() >= 36 {
            return Err(TbSemError::Config(format!(
                "Project UUID '{}' not in config. Run `tb-sem config init` to register it.",
                name
            )));
        }
        Err(TbSemError::Config(format!(
            "Unknown project '{}'. Available: {}",
            name,
            self.projects.keys().cloned().collect::<Vec<_>>().join(", ")
        )))
    }

    pub fn masked_token(&self) -> String {
        toolbox_core::config::masked_token(&self.token)
    }

    pub fn timezone(&self) -> Result<chrono_tz::Tz> {
        self.timezone
            .parse()
            .map_err(|_| TbSemError::Config(format!("Invalid timezone '{}'. Run `tb-sem config set timezone <IANA_TIMEZONE>`", self.timezone)))
    }
}
