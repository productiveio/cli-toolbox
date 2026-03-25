use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{Error, Result};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub infra: InfraConfig,
    pub docker: DockerConfig,
    #[serde(default)]
    pub services: BTreeMap<String, ServiceConfig>,
}

#[derive(Debug, Deserialize)]
pub struct InfraConfig {
    pub compose_file: String,
    pub compose_project: String,
    #[serde(default)]
    pub services: BTreeMap<String, InfraServiceConfig>,
}

#[derive(Debug, Deserialize)]
pub struct InfraServiceConfig {
    pub port: u16,
    #[serde(default)]
    pub volume: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DockerConfig {
    pub compose_file: String,
    pub compose_project: String,
    pub container: String,
}

#[derive(Debug, Deserialize)]
pub struct ServiceConfig {
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub hostname: Option<String>,
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(default)]
    pub cmd: Option<String>,
    #[serde(default)]
    pub infra: Vec<String>,
    #[serde(default)]
    pub secrets: Vec<String>,
    #[serde(default)]
    pub companion: Option<String>,
    #[serde(default)]
    pub init: Vec<String>,
    #[serde(default)]
    pub start: Vec<String>,
}

/// Walk up from `start` looking for `devctl.toml`.
/// Returns (config, project_root) on success.
pub fn find_and_load(start: &Path) -> Result<(Config, PathBuf)> {
    let config_path = find_config_file(start)?;
    let project_root = config_path
        .parent()
        .ok_or_else(|| Error::Config("devctl.toml has no parent directory".into()))?
        .to_path_buf();

    let content = std::fs::read_to_string(&config_path)?;
    let config: Config = toml::from_str(&content)?;
    Ok((config, project_root))
}

/// Walk up the directory tree to find `devctl.toml`.
fn find_config_file(start: &Path) -> Result<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        let candidate = dir.join("devctl.toml");
        if candidate.exists() {
            return Ok(candidate);
        }
        if !dir.pop() {
            return Err(Error::Config(
                "devctl.toml not found (searched up from current directory)".into(),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_config() {
        let toml_str = r#"
[infra]
compose_file = "docker/infra-compose.yml"
compose_project = "productive-infra"

[docker]
compose_file = "docker/dev-compose.yml"
compose_project = "productive-dev"
container = "productive-dev-workspace"

[services.api]
port = 3000
hostname = "api.productive.io.localhost"
repo = "api"
cmd = "bundle exec rails server -b 0.0.0.0 -p 3000"
infra = ["mysql", "redis"]
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.services.len(), 1);
        assert_eq!(config.services["api"].port, Some(3000));
        assert_eq!(config.services["api"].infra, vec!["mysql", "redis"]);
    }
}
