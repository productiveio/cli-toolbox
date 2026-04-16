use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

fn default_org() -> String {
    "productiveio".to_string()
}

fn default_username_override() -> String {
    String::new()
}

fn default_interval_minutes() -> u64 {
    5
}

fn default_productive_org_slug() -> String {
    "109-productive".to_string()
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GithubConfig {
    #[serde(default = "default_org")]
    pub org: String,

    /// Override `gh auth` username. Empty string means: derive from `gh`.
    #[serde(default = "default_username_override")]
    pub username_override: String,
}

impl Default for GithubConfig {
    fn default() -> Self {
        Self {
            org: default_org(),
            username_override: default_username_override(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RefreshConfig {
    #[serde(default = "default_interval_minutes")]
    pub interval_minutes: u64,
}

impl Default for RefreshConfig {
    fn default() -> Self {
        Self {
            interval_minutes: default_interval_minutes(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ProductiveConfig {
    #[serde(default = "default_productive_org_slug")]
    pub org_slug: String,
}

impl Default for ProductiveConfig {
    fn default() -> Self {
        Self {
            org_slug: default_productive_org_slug(),
        }
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub github: GithubConfig,

    #[serde(default)]
    pub refresh: RefreshConfig,

    #[serde(default)]
    pub productive: ProductiveConfig,
}

impl Config {
    pub fn config_path() -> Result<PathBuf> {
        toolbox_core::config::config_path("tb-pr").map_err(|e| Error::Config(e.to_string()))
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        let config = toolbox_core::config::load_standalone::<Config>(&path)
            .map_err(|e| Error::Config(e.to_string()))?
            .unwrap_or_default();
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        toolbox_core::config::save_config(&path, self).map_err(|e| Error::Config(e.to_string()))
    }

    /// Returns `~/.cache/tb-pr/`, creating the directory.
    pub fn cache_dir(&self) -> Result<PathBuf> {
        let base = dirs::cache_dir()
            .ok_or_else(|| Error::Config("cannot determine cache directory".to_string()))?;
        let dir = base.join("tb-pr");
        std::fs::create_dir_all(&dir).map_err(|e| Error::Config(e.to_string()))?;
        Ok(dir)
    }

    pub fn refresh_interval(&self) -> Duration {
        Duration::from_secs(self.refresh.interval_minutes * 60)
    }
}
