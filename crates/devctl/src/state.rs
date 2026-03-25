use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Result;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub services: BTreeMap<String, ServiceState>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ServiceState {
    pub mode: String,
    pub started_at: String,
    #[serde(default)]
    pub dir: Option<String>,
    #[serde(default)]
    pub pid: Option<u32>,
}

impl State {
    /// Load state from `.devctl/state.json` under the project root.
    /// Returns empty state if file doesn't exist.
    pub fn load(project_root: &Path) -> Result<Self> {
        let path = state_path(project_root);
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)?;
        let state: Self = serde_json::from_str(&content)?;
        Ok(state)
    }

    /// Save state to `.devctl/state.json` under the project root.
    pub fn save(&self, project_root: &Path) -> Result<()> {
        let path = state_path(project_root);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }
}

fn state_path(project_root: &Path) -> PathBuf {
    project_root.join(".devctl").join("state.json")
}
