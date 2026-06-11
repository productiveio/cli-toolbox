use reqwest::{Client, Method, RequestBuilder};
use serde::{Deserialize, Serialize};

use crate::cache::{Cache, CacheTtl};
use crate::config::Config;
use crate::error::{Result, TbLfError};

pub struct DevPortalClient {
    client: Client,
    base_url: String,
    devportal_url: String,
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
            devportal_url: config.url.clone(),
            token: config.token.clone(),
            cache: Cache::new("tb-lf")?,
            no_cache,
        })
    }

    /// Shared HTTP cycle: build URL, attach auth + Accept headers, apply
    /// caller-supplied body/headers via `configure`, send, then either return
    /// the body string or map non-2xx into a typed `TbLfError::Api`.
    /// Callers are responsible for caching (only `get_raw` opts in).
    async fn raw_request<F>(
        &self,
        method: Method,
        base: &str,
        path: &str,
        configure: F,
    ) -> Result<String>
    where
        F: FnOnce(RequestBuilder) -> RequestBuilder,
    {
        let url = format!("{}{}", base, path);
        let request = self
            .client
            .request(method, &url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/json");
        let resp = configure(request).send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(api_error(status, body));
        }

        Ok(resp.text().await?)
    }

    pub async fn get_raw(&self, path: &str, ttl: CacheTtl) -> Result<String> {
        let url = format!("{}{}", self.base_url, path);

        if !self.no_cache
            && let Some(cached) = self.cache.get(&url, &ttl)
        {
            return Ok(cached);
        }

        let body = self
            .raw_request(Method::GET, &self.base_url, path, |b| b)
            .await?;
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
        let resp = self
            .raw_request(Method::PATCH, &self.base_url, path, |b| b.json(body))
            .await?;
        Ok(serde_json::from_str(&resp)?)
    }

    pub async fn delete(&self, path: &str) -> Result<()> {
        self.raw_request(Method::DELETE, &self.base_url, path, |b| b)
            .await?;
        Ok(())
    }

    /// POST a multipart form to a DevPortal endpoint. `path` is appended to
    /// the bare DevPortal URL (not the `/spa_api/ai` API base), e.g. pass
    /// `/spa_api/shares`.
    pub async fn post_multipart<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        form: reqwest::multipart::Form,
    ) -> Result<T> {
        let url = format!("{}{}", self.devportal_url, path);
        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Accept", "application/json")
            .multipart(form)
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

    /// GET against the bare DevPortal URL (not the `/spa_api/ai` API base).
    /// Uncached — alias state changes mid-session via PATCH/DELETE in the
    /// same orchestration, so re-reads must be fresh.
    pub async fn devportal_get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let resp = self
            .raw_request(Method::GET, &self.devportal_url, path, |b| b)
            .await?;
        Ok(serde_json::from_str(&resp)?)
    }

    /// POST JSON against the bare DevPortal URL.
    pub async fn devportal_post_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &impl Serialize,
    ) -> Result<T> {
        let resp = self
            .raw_request(Method::POST, &self.devportal_url, path, |b| b.json(body))
            .await?;
        Ok(serde_json::from_str(&resp)?)
    }

    /// PATCH JSON against the bare DevPortal URL.
    pub async fn devportal_patch_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &impl Serialize,
    ) -> Result<T> {
        let resp = self
            .raw_request(Method::PATCH, &self.devportal_url, path, |b| b.json(body))
            .await?;
        Ok(serde_json::from_str(&resp)?)
    }

    /// DELETE against the bare DevPortal URL.
    pub async fn devportal_delete(&self, path: &str) -> Result<()> {
        self.raw_request(Method::DELETE, &self.devportal_url, path, |b| b)
            .await?;
        Ok(())
    }

    pub fn cache(&self) -> &Cache {
        &self.cache
    }

    /// Bare Backyard base URL (e.g. `https://backyard.productive.io`) —
    /// used to build absolute `/u/<user_id>/<slug>` URLs to print after a
    /// successful alias write.
    pub fn devportal_url(&self) -> &str {
        &self.devportal_url
    }
}
