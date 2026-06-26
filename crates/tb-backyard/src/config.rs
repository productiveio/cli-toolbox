use std::path::PathBuf;

use base64::Engine;
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

        // 2. Fall back to ~/.config/tb-backyard/config.toml
        let from_file = match from_secrets {
            Some(c) => Some(c),
            None => {
                let path = Self::config_path()?;
                toolbox_core::config::load_standalone(&path)
                    .map_err(|e| TbBackyardError::Config(e.to_string()))?
            }
        };

        // 3. Token: BACKYARD_TOKEN override → PRODUCTIVE_AUTH_TOKEN envelope →
        //    config file. Backyard accepts a Productive PAT (X-Auth-Token); the
        //    common `PRODUCTIVE_AUTH_TOKEN` is a base64 JSON envelope, so the
        //    inner `personal_access_token` is extracted from it.
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
                return Err(TbBackyardError::Config(
                    "No config found. Set PRODUCTIVE_AUTH_TOKEN/BACKYARD_TOKEN, run `tb-backyard config init --token <TOKEN>`, or create ~/.config/tb-backyard/config.toml".into(),
                ));
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

    /// Resolve the auth token from the environment. Prefers an explicit raw
    /// `BACKYARD_TOKEN`, then the inner PAT decoded from the
    /// `PRODUCTIVE_AUTH_TOKEN` base64 envelope. Returns None if neither yields
    /// a usable token (caller falls back to the config file).
    fn resolve_env_token() -> Option<String> {
        if let Ok(t) = std::env::var("BACKYARD_TOKEN")
            && !t.is_empty()
        {
            return Some(t);
        }
        if let Ok(raw) = std::env::var("PRODUCTIVE_AUTH_TOKEN") {
            return decode_pat_envelope(&raw);
        }
        None
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

/// `PRODUCTIVE_AUTH_TOKEN` is a base64-encoded JSON envelope
/// (`{organization_id, person_id, user_id, user_email, personal_access_token}`);
/// the real credential is the inner `personal_access_token`. Returns None when
/// the value isn't a decodable envelope, so the caller can fall through to
/// other token sources.
fn decode_pat_envelope(raw: &str) -> Option<String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(raw.trim())
        .ok()?;
    let envelope: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    envelope
        .get("personal_access_token")?
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(json: &str) -> String {
        base64::engine::general_purpose::STANDARD.encode(json)
    }

    #[test]
    fn decode_pat_envelope_extracts_inner_token() {
        let raw = envelope(
            r#"{"organization_id":"109","personal_access_token":"abc123","user_id":"53237"}"#,
        );
        assert_eq!(decode_pat_envelope(&raw).as_deref(), Some("abc123"));
        // Tolerates surrounding whitespace (env vars sometimes carry a newline).
        assert_eq!(
            decode_pat_envelope(&format!("  {raw}\n")).as_deref(),
            Some("abc123")
        );
    }

    #[test]
    fn decode_pat_envelope_rejects_non_envelope() {
        // Not base64 at all.
        assert_eq!(decode_pat_envelope("not-a-token"), None);
        // Valid base64, but the JSON has no personal_access_token.
        assert_eq!(decode_pat_envelope(&envelope(r#"{"user_id":"1"}"#)), None);
        // Present but empty → treated as absent so the caller falls through.
        assert_eq!(
            decode_pat_envelope(&envelope(r#"{"personal_access_token":""}"#)),
            None
        );
    }
}
