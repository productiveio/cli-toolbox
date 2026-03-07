use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{SemiError, Result};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ProjectConfig {
    pub id: String,
    pub branch: String,
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
    iana_time_zone::get_timezone().unwrap_or_else(|_| "Europe/Zagreb".to_string())
}

impl Config {
    /// Standard config file path: ~/.config/tb-sem/config.toml
    pub fn config_path() -> Result<PathBuf> {
        let dir = dirs::config_dir()
            .ok_or_else(|| SemiError::Config("cannot determine config directory".into()))?;
        Ok(dir.join("tb-sem/config.toml"))
    }

    /// Load config from (first match wins):
    ///   1. secrets.toml [semaphore] section (monorepo root)
    ///   2. ~/.config/tb-sem/config.toml (standalone)
    /// Token can be overridden by SEMAPHORE_API_TOKEN env var.
    pub fn load() -> Result<Self> {
        let mut config: Option<Config> = None;

        // Try monorepo secrets.toml with [semaphore] section
        let secrets_path = PathBuf::from("secrets.toml");
        if secrets_path.exists() {
            let content = std::fs::read_to_string(&secrets_path)?;
            let table: toml::Table = toml::from_str(&content)?;
            if let Some(section) = table.get("semaphore") {
                config = Some(section.clone().try_into().map_err(|e: toml::de::Error| {
                    SemiError::Config(format!("invalid [semaphore] section: {}", e))
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
            SemiError::Config(
                "No config found. Run `tb-sem config init --token <TOKEN>` or create ~/.config/tb-sem/config.toml".into(),
            )
        })?;

        // Env var overrides file token
        if let Ok(token) = std::env::var("SEMAPHORE_API_TOKEN") {
            config.token = token;
        }

        Ok(config)
    }

    /// Save config to the standard config path.
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    pub fn base_url(&self) -> String {
        format!("https://{}.semaphoreci.com/api/v1alpha", self.org_id)
    }

    /// Resolve a project name to (project_id, default_branch).
    /// Accepts either an alias from config or a raw UUID.
    pub fn resolve_project(&self, name: &str) -> Result<(&str, &str)> {
        if let Some(proj) = self.projects.get(name) {
            return Ok((&proj.id, &proj.branch));
        }
        // Check if it looks like a UUID (contains dashes, len >= 36)
        if name.contains('-') && name.len() >= 36 {
            // Raw UUID, no default branch
            return Err(SemiError::Config(format!(
                "Project UUID '{}' not in config. Use `tb-sem config add` to register it.",
                name
            )));
        }
        Err(SemiError::Config(format!(
            "Unknown project '{}'. Available: {}",
            name,
            self.projects.keys().cloned().collect::<Vec<_>>().join(", ")
        )))
    }

    pub fn masked_token(&self) -> String {
        if self.token.len() > 8 {
            format!("****...{}", &self.token[self.token.len() - 4..])
        } else {
            "****".to_string()
        }
    }

    pub fn timezone(&self) -> chrono_tz::Tz {
        self.timezone.parse().unwrap_or(chrono_tz::Europe::Zagreb)
    }
}
