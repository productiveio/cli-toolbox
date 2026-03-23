use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

fn default_claude_home() -> String {
    "~/.claude".to_string()
}

fn default_ttl_minutes() -> u64 {
    60
}

fn default_limit() -> usize {
    10
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    #[serde(default = "default_claude_home")]
    pub claude_home: String,

    #[serde(default = "default_ttl_minutes")]
    pub ttl_minutes: u64,

    #[serde(default = "default_limit")]
    pub default_limit: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            claude_home: default_claude_home(),
            ttl_minutes: default_ttl_minutes(),
            default_limit: default_limit(),
        }
    }
}

impl Config {
    pub fn config_path() -> Result<PathBuf> {
        toolbox_core::config::config_path("tb-session")
            .map_err(|e| Error::Config(e.to_string()))
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
        toolbox_core::config::save_config(&path, self)
            .map_err(|e| Error::Config(e.to_string()))
    }

    /// Resolves `~` in `claude_home` to an absolute PathBuf.
    pub fn claude_home_path(&self) -> PathBuf {
        if self.claude_home.starts_with('~') {
            if let Some(home) = dirs::home_dir() {
                let stripped = self.claude_home.trim_start_matches('~');
                let stripped = stripped.trim_start_matches('/');
                if stripped.is_empty() {
                    return home;
                }
                return home.join(stripped);
            }
        }
        PathBuf::from(&self.claude_home)
    }

    /// Returns `{claude_home}/projects/`.
    pub fn projects_dir(&self) -> PathBuf {
        self.claude_home_path().join("projects")
    }

    /// Returns `~/.cache/tb-session/index.db`, creating the parent dir.
    pub fn db_path(&self) -> Result<PathBuf> {
        let cache_dir = dirs::cache_dir().ok_or_else(|| {
            Error::Config("cannot determine cache directory".to_string())
        })?;
        let db_dir = cache_dir.join("tb-session");
        std::fs::create_dir_all(&db_dir).map_err(|e| Error::Config(e.to_string()))?;
        Ok(db_dir.join("index.db"))
    }

    /// Returns the TTL as a `Duration`.
    pub fn ttl(&self) -> Duration {
        Duration::from_secs(self.ttl_minutes * 60)
    }
}
