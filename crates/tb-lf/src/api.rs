use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::cache::{Cache, CacheTtl};
use crate::config::Config;
use crate::error::{Result, TbLfError};

pub struct DevPortalClient {
    client: Client,
    base_url: String,
    token: String,
    cache: Cache,
    no_cache: bool,
}

#[derive(Debug, Deserialize)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub meta: PaginationMeta,
}

#[derive(Debug, Deserialize)]
pub struct PaginationMeta {
    pub page: u32,
    pub per_page: u32,
    pub total: u32,
}

fn api_error(status: u16, body: String) -> TbLfError {
    let message = match status {
        401 => "Invalid token. Run `tb-lf config show` to check.".into(),
        404 => "Not found.".into(),
        s if s >= 500 => format!("DevPortal error ({}): {}", s, body),
        _ => body,
    };
    TbLfError::Api { status, message }
}

impl DevPortalClient {
    pub fn new(config: &Config, no_cache: bool) -> Result<Self> {
        Ok(Self {
            client: Client::new(),
            base_url: config.base_api_url(),
            token: config.token.clone(),
            cache: Cache::new("tb-lf")?,
            no_cache,
        })
    }

    pub async fn get_raw(&self, path: &str, ttl: CacheTtl) -> Result<String> {
        let url = format!("{}{}", self.base_url, path);

        if !self.no_cache
            && let Some(cached) = self.cache.get(&url, &ttl)
        {
            return Ok(cached);
        }

        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/json")
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(api_error(status, body));
        }

        let body = resp.text().await?;
        self.cache.set(&url, &body);
        Ok(body)
    }

    pub async fn get<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        ttl: CacheTtl,
    ) -> Result<T> {
        let body = self.get_raw(path, ttl).await?;
        Ok(serde_json::from_str(&body)?)
    }

    /// Build a path with query params, filtering out None values.
    pub fn build_path(base: &str, params: &[(&str, Option<String>)]) -> String {
        let pairs: Vec<String> = params
            .iter()
            .filter_map(|(k, v)| v.as_ref().map(|val| format!("{}={}", k, val)))
            .collect();

        if pairs.is_empty() {
            base.to_string()
        } else {
            format!("{}?{}", base, pairs.join("&"))
        }
    }

    pub async fn patch<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &impl Serialize,
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .patch(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/json")
            .json(body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(api_error(status, body));
        }
        let body = resp.text().await?;
        Ok(serde_json::from_str(&body)?)
    }

    pub async fn delete(&self, path: &str) -> Result<()> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .delete(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/json")
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(api_error(status, body));
        }
        Ok(())
    }

    pub fn cache(&self) -> &Cache {
        &self.cache
    }
}
