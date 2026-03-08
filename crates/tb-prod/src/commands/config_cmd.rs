use colored::Colorize;

use crate::config::Config;
use crate::error::{Result, TbProdError};

pub async fn init(token: Option<&str>, org_id: Option<&str>) -> Result<()> {
    use inquire::{InquireError, Password, PasswordDisplayMode};
    use std::io::IsTerminal;

    let interactive = std::io::stdin().is_terminal();
    let existing = Config::load().ok();

    // Resolve token
    let token = match token {
        Some(t) => t.to_string(),
        None if interactive => {
            let mut prompt = Password::new("Productive API token:")
                .with_display_mode(PasswordDisplayMode::Masked)
                .without_confirmation();
            if existing.is_some() {
                prompt = prompt.with_help_message("Press Enter to keep existing token");
            }
            match prompt.prompt() {
                Ok(t) if t.is_empty() => {
                    if let Some(ref cfg) = existing {
                        cfg.token.clone()
                    } else {
                        return Err(TbProdError::Config("Token is required".into()));
                    }
                }
                Ok(t) => t,
                Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
                    println!("Cancelled.");
                    return Ok(());
                }
                Err(e) => return Err(TbProdError::Config(e.to_string())),
            }
        }
        None => {
            return Err(TbProdError::Config(
                "Token is required. Use --token or run interactively in a terminal.".into(),
            ));
        }
    };

    let (org_id, person_id) = match org_id {
        Some(id) => (id.to_string(), None),
        None => {
            // Auto-detect org and person via organization_memberships
            let client = reqwest::Client::new();
            let resp = client
                .get("https://api.productive.io/api/v2/organization_memberships?include=organization,person")
                .header("Content-Type", "application/vnd.api+json")
                .header("X-Auth-Token", &token)
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                return Err(TbProdError::Api {
                    status,
                    message: body,
                });
            }

            let json: serde_json::Value = resp.json().await?;
            let included = json["included"].as_array();

            // Find the organization
            let org = included
                .and_then(|items| {
                    items
                        .iter()
                        .find(|i| i["type"].as_str() == Some("organizations"))
                })
                .ok_or_else(|| {
                    TbProdError::Config("No organization found for this token".into())
                })?;

            let org_id = org["id"]
                .as_str()
                .ok_or_else(|| TbProdError::Config("Organization has no id".into()))?;
            let org_name = org["attributes"]["name"].as_str().unwrap_or("?");
            println!("Organization: {} (id: {})", org_name, org_id);

            // Find the person
            let person = included
                .and_then(|items| items.iter().find(|i| i["type"].as_str() == Some("people")));

            let person_id = if let Some(p) = person {
                let pid = p["id"].as_str().unwrap_or("?");
                let first = p["attributes"]["first_name"].as_str().unwrap_or("");
                let last = p["attributes"]["last_name"].as_str().unwrap_or("");
                println!("Person: {} {} (id: {})", first, last, pid);
                Some(pid.to_string())
            } else {
                None
            };

            (org_id.to_string(), person_id)
        }
    };

    let config = Config {
        token,
        org_id,
        person_id,
        api_base_url: None,
    };
    config.save()?;
    println!("Config saved to {:?}", Config::config_path()?);
    Ok(())
}

pub fn show(config: &Config) {
    println!("token:      {}", config.masked_token());
    println!("org_id:     {}", config.org_id);
    println!(
        "person_id:  {}",
        config.person_id.as_deref().unwrap_or("(not set)")
    );
    println!("base_url:   {}", config.base_url());
}

pub fn set(key: &str, value: &str) -> Result<()> {
    let path = Config::config_path()?;
    let mut table: toml::Table = if path.exists() {
        let content =
            std::fs::read_to_string(&path).map_err(|e| TbProdError::Config(e.to_string()))?;
        toml::from_str(&content).map_err(|e| TbProdError::Config(e.to_string()))?
    } else {
        toml::Table::new()
    };

    match key {
        "token" | "org_id" | "person_id" | "api_base_url" => {
            table.insert(key.to_string(), toml::Value::String(value.to_string()));
        }
        _ => {
            return Err(TbProdError::Config(format!(
                "Unknown config key '{}'. Valid keys: token, org_id, person_id, api_base_url",
                key
            )));
        }
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| TbProdError::Config(e.to_string()))?;
    }
    std::fs::write(&path, toml::to_string_pretty(&table).unwrap())
        .map_err(|e| TbProdError::Config(e.to_string()))?;
    println!("Set {} = {}", key.bold(), value);
    Ok(())
}
