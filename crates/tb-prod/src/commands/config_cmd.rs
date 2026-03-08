use colored::Colorize;
use toolbox_core::prompt::PromptResult;

use crate::config::Config;
use crate::error::{Result, TbProdError};

pub async fn init(token: Option<&str>, org_id: Option<&str>) -> Result<()> {
    let existing = Config::load().ok();

    // Resolve token
    let token = match toolbox_core::prompt::prompt_token(
        "Productive API token:",
        token,
        existing.as_ref().map(|c| c.token.as_str()),
    ) {
        Ok(PromptResult::Ok(t)) => t,
        Ok(PromptResult::Cancelled) => {
            println!("Cancelled.");
            return Ok(());
        }
        Err(e) => return Err(TbProdError::Config(e)),
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
    match key {
        "token" | "org_id" | "person_id" | "api_base_url" => {}
        _ => {
            return Err(TbProdError::Config(format!(
                "Unknown config key '{}'. Valid keys: token, org_id, person_id, api_base_url",
                key
            )));
        }
    }

    let path = Config::config_path()?;
    toolbox_core::config::patch_toml(&path, key, value)
        .map_err(|e| TbProdError::Config(e.to_string()))?;
    println!("Set {} = {}", key.bold(), value);
    Ok(())
}
