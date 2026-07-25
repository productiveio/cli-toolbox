use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::api::{BackyardClient, PaginatedResponse};
use crate::cache::CacheTtl;
use crate::error::{Result, TbBackyardError};
use crate::types::Project;

/// Default Backyard host — used when no `url` is configured and auth comes
/// purely from the environment.
pub const DEFAULT_URL: &str = "https://backyard.productive.io";

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub url: String,
    pub token: String,
    #[serde(default)]
    pub project: Option<String>,
}

impl Config {
    pub fn load() -> Result<Self> {
        // 1. Try secrets.toml [backyard] section
        let from_secrets: Option<Config> = toolbox_core::config::load_secrets_section("backyard")
            .map_err(|e| TbBackyardError::Config(e.to_string()))?;

        // 2. Fall back to the standalone config file at the platform config dir
        //    (see toolbox_core::config::config_path — `~/Library/Application
        //    Support/tb-backyard` on macOS, `~/.config/tb-backyard` on Linux).
        let from_file = match from_secrets {
            Some(c) => Some(c),
            None => {
                let path = Self::config_path()?;
                toolbox_core::config::load_standalone(&path)
                    .map_err(|e| TbBackyardError::Config(e.to_string()))?
            }
        };

        // 3. Token: PRODUCTIVE_AUTH_TOKEN (a raw Productive PAT) overrides the
        //    config-file token. Backyard authenticates with the Productive PAT
        //    via X-Auth-Token; the env var is a fallback for testing against a
        //    token or host other than the configured one.
        let env_token = Self::resolve_env_token();

        // A config file is optional: an env-supplied token is enough to run
        // against the default host. Only error when neither is present.
        let mut config = match (from_file, &env_token) {
            (Some(c), _) => c,
            (None, Some(_)) => Config {
                url: DEFAULT_URL.into(),
                token: String::new(),
                project: None,
            },
            (None, None) => {
                let cfg = Self::config_path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| "the config file".into());
                return Err(TbBackyardError::Config(format!(
                    "No config found. Set PRODUCTIVE_AUTH_TOKEN, run `tb-backyard config init --token <TOKEN>`, or create {cfg}",
                )));
            }
        };

        if let Some(token) = env_token {
            config.token = token;
        }
        if let Ok(url) = std::env::var("BACKYARD_URL") {
            config.url = url;
        }
        if config.url.is_empty() {
            config.url = DEFAULT_URL.into();
        }

        // Normalize: strip trailing slash
        config.url = config.url.trim_end_matches('/').to_string();

        Ok(config)
    }

    /// Resolve the auth token from the environment: the raw Productive PAT in
    /// `PRODUCTIVE_AUTH_TOKEN`. Returns None when unset or empty, so the caller
    /// falls back to the config file.
    fn resolve_env_token() -> Option<String> {
        std::env::var("PRODUCTIVE_AUTH_TOKEN")
            .ok()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
    }

    pub fn config_path() -> Result<PathBuf> {
        toolbox_core::config::config_path("tb-backyard")
            .map_err(|e| TbBackyardError::Config(e.to_string()))
    }

    pub fn base_api_url(&self) -> String {
        format!("{}/spa_api/ai", self.url)
    }

    pub fn masked_token(&self) -> String {
        toolbox_core::config::masked_token(&self.token)
    }
}

/// Resolve `--project` flag to a numeric project ID.
/// Accepts a project name (case-insensitive) or numeric ID.
pub async fn resolve_project(
    client: &BackyardClient,
    flag: Option<&str>,
    default: Option<&str>,
) -> Result<Option<i64>> {
    let input = flag.or(default);
    let Some(input) = input else {
        return Ok(None);
    };

    // If it's a number, use directly
    if let Ok(id) = input.parse::<i64>() {
        return Ok(Some(id));
    }

    // Fetch project list and match by name
    let resp: PaginatedResponse<Project> = client.get("/projects", CacheTtl::Long).await?;
    let projects = resp.data;
    let matches: Vec<&Project> = projects
        .iter()
        .filter(|p| p.name.eq_ignore_ascii_case(input))
        .collect();

    match matches.len() {
        1 => Ok(Some(matches[0].id)),
        0 => {
            let names: Vec<String> = projects
                .iter()
                .map(|p| format!("  {} (id: {})", p.name, p.id))
                .collect();
            Err(TbBackyardError::Config(format!(
                "Project '{}' not found. Available projects:\n{}",
                input,
                names.join("\n"),
            )))
        }
        _ => {
            let names: Vec<String> = matches
                .iter()
                .map(|p| format!("  {} (id: {})", p.name, p.id))
                .collect();
            Err(TbBackyardError::Config(format!(
                "Ambiguous project '{}'. Matches:\n{}\nUse numeric ID to disambiguate.",
                input,
                names.join("\n"),
            )))
        }
    }
}
