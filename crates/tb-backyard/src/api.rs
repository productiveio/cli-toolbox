use reqwest::{Client, Method, RequestBuilder};
use serde::{Deserialize, Serialize};

use crate::cache::{Cache, CacheTtl};
use crate::config::Config;
use crate::error::{Result, TbBackyardError};

pub struct BackyardClient {
    client: Client,
    base_url: String,
    backyard_url: String,
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

fn api_error(status: u16, body: String) -> TbBackyardError {
    let message = match status {
        401 => "Invalid token. Run `tb-backyard config show` to check.".into(),
        404 => "Not found.".into(),
        429 => "Rate limited by Backyard and retries exhausted. Try again shortly.".into(),
        s if s >= 500 => format!("Backyard error ({}): {}", s, body),
        _ => body,
    };
    TbBackyardError::Api { status, message }
}

/// How many times we re-issue a request after a 429 before giving up.
const MAX_RETRY_ATTEMPTS: u32 = 3;
/// Fallback wait when a 429 carries no (or an unparseable) Retry-After.
const DEFAULT_RETRY_SECS: u64 = 5;
/// Ceiling on a single honored Retry-After so a hostile/huge value can't hang
/// the CLI indefinitely.
const MAX_RETRY_SECS: u64 = 60;

/// Seconds to wait from a `Retry-After` header value. Backyard (and the
/// Langfuse proxy behind it) send delay-seconds; parse that, fall back to a
/// default when absent/unparseable, and clamp to a sane ceiling.
fn retry_after_secs(raw: Option<&str>) -> u64 {
    raw.and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(DEFAULT_RETRY_SECS)
        .min(MAX_RETRY_SECS)
}

fn retry_after_delay(headers: &reqwest::header::HeaderMap) -> std::time::Duration {
    let raw = headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok());
    std::time::Duration::from_secs(retry_after_secs(raw))
}

impl BackyardClient {
    pub fn new(config: &Config, no_cache: bool) -> Result<Self> {
        Ok(Self {
            client: Client::new(),
            base_url: config.base_api_url(),
            backyard_url: config.url.clone(),
            token: config.token.clone(),
            cache: Cache::new("tb-backyard")?,
            no_cache,
        })
    }

    /// Shared HTTP cycle: build URL, attach auth + Accept headers, apply
    /// caller-supplied body/headers via `configure`, send, then either return
    /// the body string or map non-2xx into a typed `TbBackyardError::Api`.
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
        let request = configure(
            self.client
                .request(method, &url)
                .header("X-Auth-Token", &self.token)
                .header("Accept", "application/json"),
        );

        // Honor 429 + Retry-After: a real rate limit (from the Langfuse proxy's
        // relayed 429, or Backyard's own Rack::Attack throttle) is transient, so
        // wait it out rather than failing the user's command (INV-1). We always
        // send a clone and keep `request` as the template so the request can be
        // re-issued; our bodies (JSON / empty) are all cloneable.
        let mut attempt = 0u32;
        let resp = loop {
            let attempt_req = request
                .try_clone()
                .ok_or_else(|| TbBackyardError::Other("request body is not retryable".into()))?;
            let resp = attempt_req.send().await?;

            if resp.status().as_u16() == 429 && attempt < MAX_RETRY_ATTEMPTS {
                let delay = retry_after_delay(resp.headers());
                attempt += 1;
                tokio::time::sleep(delay).await;
                continue;
            }
            break resp;
        };

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

    pub async fn post<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &impl Serialize,
    ) -> Result<T> {
        let resp = self
            .raw_request(Method::POST, &self.base_url, path, |b| b.json(body))
            .await?;
        Ok(serde_json::from_str(&resp)?)
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

    /// POST a multipart form to a Backyard endpoint. `path` is appended to
    /// the bare Backyard URL (not the `/spa_api/ai` API base), e.g. pass
    /// `/spa_api/shares`.
    pub async fn post_multipart<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        form: reqwest::multipart::Form,
    ) -> Result<T> {
        let url = format!("{}{}", self.backyard_url, path);
        let resp = self
            .client
            .post(&url)
            .header("X-Auth-Token", &self.token)
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

    /// GET against the bare Backyard URL (not the `/spa_api/ai` API base).
    /// Uncached — alias state changes mid-session via PATCH/DELETE in the
    /// same orchestration, so re-reads must be fresh.
    pub async fn backyard_get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let resp = self
            .raw_request(Method::GET, &self.backyard_url, path, |b| b)
            .await?;
        Ok(serde_json::from_str(&resp)?)
    }

    /// POST JSON against the bare Backyard URL.
    pub async fn backyard_post_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &impl Serialize,
    ) -> Result<T> {
        let resp = self
            .raw_request(Method::POST, &self.backyard_url, path, |b| b.json(body))
            .await?;
        Ok(serde_json::from_str(&resp)?)
    }

    /// PATCH JSON against the bare Backyard URL.
    pub async fn backyard_patch_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &impl Serialize,
    ) -> Result<T> {
        let resp = self
            .raw_request(Method::PATCH, &self.backyard_url, path, |b| b.json(body))
            .await?;
        Ok(serde_json::from_str(&resp)?)
    }

    /// DELETE against the bare Backyard URL.
    pub async fn backyard_delete(&self, path: &str) -> Result<()> {
        self.raw_request(Method::DELETE, &self.backyard_url, path, |b| b)
            .await?;
        Ok(())
    }

    /// Download a single shared file's bytes. Hits the viewer route
    /// `/s/:token/:filename` with the CLI's auth header; the server 302s to a
    /// short-lived presigned URL on a cookie-less origin (INV-1), which we then
    /// fetch without our auth header (redirects don't leak `X-Auth-Token` to
    /// S3). A same-app redirect to `/sign_in` means the auth/publish gate
    /// bounced us, not a servable file.
    pub async fn download_share_file(&self, token: &str, filename: &str) -> Result<Vec<u8>> {
        let mut url = reqwest::Url::parse(&self.backyard_url)
            .map_err(|e| TbBackyardError::Other(format!("invalid backyard url: {e}")))?;
        url.path_segments_mut()
            .map_err(|_| TbBackyardError::Other("backyard url cannot be a base".into()))?
            .pop_if_empty()
            .extend(["s", token, filename]);

        let no_redirect = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let resp = no_redirect
            .get(url.clone())
            .header("X-Auth-Token", &self.token)
            .send()
            .await?;
        let status = resp.status();

        if status.is_redirection() {
            let location = resp
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| {
                    TbBackyardError::Other("share redirect had no Location header".into())
                })?
                .to_string();

            let is_sign_in = reqwest::Url::parse(&location)
                .map(|u| u.path() == "/sign_in")
                .unwrap_or_else(|_| location.starts_with("/sign_in"));
            if is_sign_in {
                return Err(TbBackyardError::Other(
                    "not authorized to download this share — check your token (`tb-backyard config show`) and that the share is published".into(),
                ));
            }

            let dl = self.client.get(&location).send().await?;
            if !dl.status().is_success() {
                let s = dl.status().as_u16();
                return Err(api_error(s, dl.text().await.unwrap_or_default()));
            }
            Ok(dl.bytes().await?.to_vec())
        } else if status.is_success() {
            Ok(resp.bytes().await?.to_vec())
        } else {
            Err(api_error(
                status.as_u16(),
                resp.text().await.unwrap_or_default(),
            ))
        }
    }

    /// Resolve view-gated metadata for a share the caller can *see* (not just
    /// own). Hits the viewer route `/s/:token` with `Accept: application/json`
    /// and the CLI's auth header — the server applies the same visibility gate
    /// as the browser view. A redirect to `/sign_in` means the gate bounced us
    /// (bad token, or a private share this user can't view); a 404 means
    /// draft/deleted/unknown; a 410 means expired.
    pub async fn get_share_metadata(&self, token: &str) -> Result<crate::types::ShareViewMetadata> {
        let mut url = reqwest::Url::parse(&self.backyard_url)
            .map_err(|e| TbBackyardError::Other(format!("invalid backyard url: {e}")))?;
        url.path_segments_mut()
            .map_err(|_| TbBackyardError::Other("backyard url cannot be a base".into()))?
            .pop_if_empty()
            .extend(["s", token]);

        let no_redirect = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let resp = no_redirect
            .get(url)
            .header("X-Auth-Token", &self.token)
            .header("Accept", "application/json")
            .send()
            .await?;
        let status = resp.status();

        if status.is_redirection() {
            return Err(TbBackyardError::Other(format!(
                "not authorized to view share `{}` — check your token (`tb-backyard config show`) and that the share is published",
                token
            )));
        }
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(TbBackyardError::Other(format!(
                "no share `{}` you can view — it may be a draft, deleted, or the token may be wrong",
                token
            )));
        }
        if !status.is_success() {
            return Err(api_error(
                status.as_u16(),
                resp.text().await.unwrap_or_default(),
            ));
        }
        Ok(serde_json::from_str(&resp.text().await?)?)
    }

    pub fn cache(&self) -> &Cache {
        &self.cache
    }

    /// Bare Backyard base URL (e.g. `https://backyard.productive.io`) —
    /// used to build absolute `/u/<user_id>/<slug>` URLs to print after a
    /// successful alias write.
    pub fn backyard_url(&self) -> &str {
        &self.backyard_url
    }
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_RETRY_SECS, MAX_RETRY_SECS, retry_after_secs};

    #[test]
    fn parses_a_numeric_retry_after() {
        assert_eq!(retry_after_secs(Some("9")), 9);
        assert_eq!(retry_after_secs(Some("  12  ")), 12);
    }

    #[test]
    fn falls_back_to_default_when_absent_or_unparseable() {
        assert_eq!(retry_after_secs(None), DEFAULT_RETRY_SECS);
        assert_eq!(retry_after_secs(Some("")), DEFAULT_RETRY_SECS);
        // HTTP-date form (not delay-seconds) is not parsed — use the default.
        assert_eq!(
            retry_after_secs(Some("Wed, 21 Oct 2015 07:28:00 GMT")),
            DEFAULT_RETRY_SECS
        );
    }

    #[test]
    fn clamps_a_hostile_retry_after_to_the_ceiling() {
        assert_eq!(retry_after_secs(Some("100000")), MAX_RETRY_SECS);
    }
}
