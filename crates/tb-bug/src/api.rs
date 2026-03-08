use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::cache::{Cache, CacheTtl};
use crate::config::Config;
use crate::error::{Result, TbBugError};

pub struct BugsnagClient {
    client: Client,
    token: String,
    cache: Cache,
    no_cache: bool,
}

// --- API response types ---

#[derive(Debug, Deserialize, Clone)]
pub struct Organization {
    pub id: String,
    pub name: String,
    pub slug: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub slug: String,
    #[serde(default)]
    pub r#type: String,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub open_error_count: u64,
    #[serde(default)]
    pub release_stages: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Error {
    pub id: String,
    pub error_class: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub context: String,
    pub severity: String,
    pub status: String,
    #[serde(default)]
    pub events: u64,
    #[serde(default)]
    pub users: u64,
    pub first_seen: String,
    pub last_seen: String,
    #[serde(default)]
    pub comment_count: u64,
    #[serde(default)]
    pub release_stages: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EventSummary {
    pub id: String,
    #[serde(default)]
    pub error_id: String,
    pub received_at: String,
    pub severity: String,
    #[serde(default)]
    pub context: String,
    #[serde(default)]
    pub unhandled: bool,
    #[serde(default)]
    pub app: Option<AppInfo>,
    #[serde(default)]
    pub device: Option<DeviceInfo>,
    #[serde(default)]
    pub user: Option<UserInfo>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AppInfo {
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub release_stage: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DeviceInfo {
    #[serde(default)]
    pub hostname: Option<String>,
    #[serde(default)]
    pub os_name: Option<String>,
    #[serde(default)]
    pub os_version: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UserInfo {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StabilityTrend {
    #[serde(default)]
    pub release_stage_name: String,
    #[serde(default)]
    pub timeline_points: Vec<StabilityBucket>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StabilityBucket {
    pub bucket_start: String,
    pub bucket_end: String,
    #[serde(default)]
    pub total_sessions_count: u64,
    #[serde(default)]
    pub unhandled_sessions_count: u64,
    #[serde(default)]
    pub unhandled_rate: f64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Release {
    pub id: String,
    #[serde(default)]
    pub app_version: String,
    #[serde(default)]
    pub release_stage: Option<ReleaseStage>,
    #[serde(default)]
    pub release_time: Option<String>,
    #[serde(default)]
    pub errors_introduced_count: u64,
    #[serde(default)]
    pub errors_seen_count: u64,
    #[serde(default)]
    pub total_sessions_count: Option<u64>,
    #[serde(default)]
    pub unhandled_sessions_count: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ReleaseStage {
    pub name: String,
}

/// Response metadata from paginated requests.
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub total_count: Option<u64>,
}

// --- Client implementation ---

const BASE_URL: &str = "https://api.bugsnag.com";

fn extract_next_link(header: &str) -> Option<&str> {
    for part in header.split(',') {
        if part.contains("rel=\"next\"") {
            let start = part.find('<')? + 1;
            let end = part.find('>')?;
            return Some(&part[start..end]);
        }
    }
    None
}

impl BugsnagClient {
    pub fn new(config: &Config, no_cache: bool) -> Result<Self> {
        Ok(Self {
            client: Client::new(),
            token: config.token.clone(),
            cache: Cache::new("tb-bug")?,
            no_cache,
        })
    }

    /// Send a GET request with one retry on 429 rate limit.
    async fn send_with_retry(&self, url: &str) -> Result<reqwest::Response> {
        let resp = self
            .client
            .get(url)
            .header("Authorization", format!("token {}", self.token))
            .header("X-Version", "2")
            .send()
            .await?;

        if resp.status().as_u16() != 429 {
            return Ok(resp);
        }

        let retry_after = resp
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(5);
        tokio::time::sleep(std::time::Duration::from_secs(retry_after)).await;

        Ok(self
            .client
            .get(url)
            .header("Authorization", format!("token {}", self.token))
            .header("X-Version", "2")
            .send()
            .await?)
    }

    async fn get_raw(&self, url: &str, ttl: CacheTtl) -> Result<String> {
        if !self.no_cache
            && let Some(cached) = self.cache.get(url, &ttl)
        {
            return Ok(cached);
        }

        let resp = self.send_with_retry(url).await?;

        let status = resp.status().as_u16();
        if status == 204 {
            return Err(TbBugError::Api {
                status,
                message: "No content".into(),
            });
        }
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(TbBugError::Api {
                status,
                message: body,
            });
        }

        let body = resp.text().await?;
        self.cache.set(url, &body);
        Ok(body)
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str, ttl: CacheTtl) -> Result<T> {
        let url = format!("{}{}", BASE_URL, path);
        let body = self.get_raw(&url, ttl).await?;
        Ok(serde_json::from_str(&body)?)
    }

    async fn get_paginated<T: serde::de::DeserializeOwned + Serialize>(
        &self,
        path: &str,
        max_pages: usize,
        ttl: CacheTtl,
    ) -> Result<PaginatedResponse<T>> {
        let initial_url = format!("{}{}", BASE_URL, path);

        // Check cache for the full merged result (keyed on initial URL)
        if !self.no_cache
            && let Some(cached) = self.cache.get(&initial_url, &ttl)
        {
            let items: Vec<T> = serde_json::from_str(&cached)?;
            return Ok(PaginatedResponse {
                items,
                total_count: None,
            });
        }

        let mut all_items: Vec<T> = Vec::new();
        let mut url = initial_url.clone();
        let mut total_count: Option<u64> = None;

        for _ in 0..max_pages {
            let resp = self.send_with_retry(&url).await?;

            if !resp.status().is_success() {
                let status = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                return Err(TbBugError::Api {
                    status,
                    message: body,
                });
            }

            if total_count.is_none() {
                total_count = resp
                    .headers()
                    .get("x-total-count")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse().ok());
            }

            let link_header = resp
                .headers()
                .get("link")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());

            let body = resp.text().await?;

            let page: Vec<T> = serde_json::from_str(&body)?;
            let page_empty = page.is_empty();
            all_items.extend(page);

            if page_empty {
                break;
            }

            match link_header.as_deref().and_then(extract_next_link) {
                Some(next) => {
                    url = next.replace("http://", "https://");
                }
                None => break,
            }
        }

        // Cache the full merged result under the initial URL
        if let Ok(serialized) = serde_json::to_string(&all_items) {
            self.cache.set(&initial_url, &serialized);
        }

        Ok(PaginatedResponse {
            items: all_items,
            total_count,
        })
    }

    // --- Public API methods ---

    pub async fn list_organizations(&self) -> Result<Vec<Organization>> {
        self.get("/user/organizations", CacheTtl::Long).await
    }

    pub async fn list_projects(&self, org_id: &str) -> Result<Vec<Project>> {
        let resp: PaginatedResponse<Project> = self
            .get_paginated(
                &format!("/organizations/{}/projects?per_page=100", org_id),
                5,
                CacheTtl::Long,
            )
            .await?;
        Ok(resp.items)
    }

    pub async fn list_errors(
        &self,
        project_id: &str,
        filters: &[(&str, &str)],
        sort: Option<&str>,
        direction: Option<&str>,
        per_page: usize,
    ) -> Result<PaginatedResponse<Error>> {
        let mut path = format!("/projects/{}/errors?per_page={}", project_id, per_page);
        if let Some(s) = sort {
            path.push_str(&format!("&sort={}", s));
        }
        if let Some(d) = direction {
            path.push_str(&format!("&direction={}", d));
        }
        for (field, value) in filters {
            path.push_str(&format!(
                "&filters[{}][][type]=eq&filters[{}][][value]={}",
                urlencoding::encode(field),
                urlencoding::encode(field),
                urlencoding::encode(value),
            ));
        }
        self.get_paginated(&path, 1, CacheTtl::Short).await
    }

    pub async fn list_events(
        &self,
        project_id: &str,
        error_id: &str,
        limit: usize,
    ) -> Result<Vec<EventSummary>> {
        let per_page = limit.max(10); // API minimum seems to be 10
        let resp: PaginatedResponse<EventSummary> = self
            .get_paginated(
                &format!(
                    "/projects/{}/errors/{}/events?per_page={}&direction=desc",
                    project_id, error_id, per_page
                ),
                1,
                CacheTtl::Short,
            )
            .await?;
        let mut items = resp.items;
        items.truncate(limit);
        Ok(items)
    }

    pub async fn get_latest_event(&self, error_id: &str) -> Result<serde_json::Value> {
        self.get(
            &format!("/errors/{}/latest_event", error_id),
            CacheTtl::Medium,
        )
        .await
    }

    pub async fn get_stability(&self, project_id: &str) -> Result<StabilityTrend> {
        self.get(
            &format!("/projects/{}/stability_trend", project_id),
            CacheTtl::Medium,
        )
        .await
    }

    pub async fn get_latest_release(
        &self,
        project_id: &str,
        stage: &str,
    ) -> Result<Option<Release>> {
        let resp: PaginatedResponse<Release> = self
            .get_paginated(
                &format!(
                    "/projects/{}/releases?per_page=1&release_stage={}",
                    project_id, stage
                ),
                1,
                CacheTtl::Medium,
            )
            .await?;
        Ok(resp.items.into_iter().next())
    }

    pub async fn list_releases(
        &self,
        project_id: &str,
        per_page: usize,
    ) -> Result<PaginatedResponse<Release>> {
        self.get_paginated(
            &format!("/projects/{}/releases?per_page={}", project_id, per_page),
            3,
            CacheTtl::Medium,
        )
        .await
    }

    pub async fn get_trends(&self, project_id: &str, buckets: u32) -> Result<serde_json::Value> {
        self.get(
            &format!("/projects/{}/trend?buckets_count={}", project_id, buckets),
            CacheTtl::Medium,
        )
        .await
    }

    pub async fn get_error_detail(
        &self,
        project_id: &str,
        error_id: &str,
    ) -> Result<serde_json::Value> {
        self.get(
            &format!("/projects/{}/errors/{}", project_id, error_id),
            CacheTtl::Medium,
        )
        .await
    }

    pub async fn get_event_detail(
        &self,
        project_id: &str,
        event_id: &str,
    ) -> Result<serde_json::Value> {
        self.get(
            &format!("/projects/{}/events/{}", project_id, event_id),
            CacheTtl::Medium,
        )
        .await
    }

    pub fn clear_cache(&self) -> Result<()> {
        self.cache.clear()?;
        Ok(())
    }
}
