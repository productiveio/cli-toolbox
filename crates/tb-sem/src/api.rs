use chrono::{DateTime, TimeZone, Utc};
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::error::{Result, TbSemError};

#[derive(Clone)]
pub struct SemaphoreClient {
    client: Client,
    base_url: String,
    token: String,
}

// Raw epoch timestamp from list endpoints
#[derive(Debug, Deserialize, Clone)]
pub struct EpochTimestamp {
    pub seconds: i64,
    #[allow(dead_code)]
    pub nanos: i64,
}

impl EpochTimestamp {
    pub fn to_datetime(&self) -> DateTime<Utc> {
        Utc.timestamp_opt(self.seconds, 0)
            .single()
            .unwrap_or_else(Utc::now)
    }
}

/// Parse an ISO 8601 string to DateTime<Utc>.
pub fn parse_iso(iso: &str) -> Option<DateTime<Utc>> {
    iso.parse::<DateTime<Utc>>().ok()
}

// Workflow from list_workflows
#[derive(Debug, Deserialize)]
pub struct Workflow {
    pub wf_id: String,
    pub triggered_by: i32, // 0 = push/hook, 1 = schedule
    pub initial_ppl_id: String,
    pub created_at: EpochTimestamp,
    pub commit_sha: String,
    pub branch_name: String,
    pub project_id: String,
}

// Wrapper for get_pipeline response
#[derive(Debug, Deserialize)]
struct PipelineResponse {
    pipeline: Pipeline,
    #[serde(default)]
    blocks: Vec<Block>,
}

// Pipeline from get_pipeline (timestamps are ISO strings)
#[derive(Debug, Deserialize)]
pub struct Pipeline {
    pub ppl_id: String,
    pub name: String,
    pub state: String,
    #[serde(default)]
    pub result: String,
    #[serde(default)]
    pub result_reason: String,
    pub created_at: String,
    #[serde(default)]
    pub running_at: Option<String>,
    #[serde(default)]
    pub done_at: Option<String>,
    pub branch_name: String,
    pub commit_sha: String,
    #[serde(default)]
    pub commit_message: String,
    pub wf_id: String,
    pub project_id: String,
    #[serde(default)]
    pub promotion_of: Option<String>,
    #[serde(skip)]
    pub blocks: Vec<Block>,
}

impl Pipeline {
    /// Normalize result to lowercase.
    pub fn result_normalized(&self) -> String {
        self.result.to_lowercase()
    }

    /// Normalize state to lowercase.
    pub fn state_normalized(&self) -> String {
        self.state.to_lowercase()
    }

    pub fn created_at_dt(&self) -> Option<DateTime<Utc>> {
        parse_iso(&self.created_at)
    }

    pub fn done_at_dt(&self) -> Option<DateTime<Utc>> {
        self.done_at.as_deref().and_then(parse_iso)
    }

    pub fn running_at_dt(&self) -> Option<DateTime<Utc>> {
        self.running_at.as_deref().and_then(parse_iso)
    }

    pub fn is_promotion(&self) -> bool {
        self.promotion_of.as_ref().is_some_and(|s| !s.is_empty())
    }

    /// Find the test job: first failed job, then "e2e" job, then any "test" job.
    pub fn find_test_job(&self) -> Option<&Job> {
        let all_jobs = || self.blocks.iter().flat_map(|b| &b.jobs);

        all_jobs()
            .find(|j| j.is_failed())
            .or_else(|| all_jobs().find(|j| j.name.to_lowercase().contains("e2e")))
            .or_else(|| all_jobs().find(|j| j.name.to_lowercase().contains("test")))
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Block {
    pub name: String,
    pub state: String,
    pub result: String,
    #[serde(default)]
    pub jobs: Vec<Job>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Job {
    pub name: String,
    pub job_id: String,
    pub status: String,
    pub result: String,
    pub index: i32,
}

impl Job {
    pub fn is_failed(&self) -> bool {
        self.result.eq_ignore_ascii_case("failed")
    }
}

// Pipeline from list_pipelines (timestamps are epoch objects)
#[derive(Debug, Deserialize)]
pub struct PipelineListItem {
    pub ppl_id: String,
    pub name: String,
    pub state: String,
    #[serde(default)]
    pub result: String,
    #[serde(default)]
    pub result_reason: String,
    pub created_at: EpochTimestamp,
    pub done_at: EpochTimestamp,
    pub branch_name: String,
    pub commit_sha: String,
    pub wf_id: String,
    pub project_id: String,
    #[serde(default)]
    pub promotion_of: String,
}

impl PipelineListItem {
    pub fn result_normalized(&self) -> String {
        self.result.to_lowercase()
    }

    pub fn is_promotion(&self) -> bool {
        !self.promotion_of.is_empty()
    }

    pub fn created_at_dt(&self) -> DateTime<Utc> {
        self.created_at.to_datetime()
    }

    pub fn done_at_dt(&self) -> DateTime<Utc> {
        self.done_at.to_datetime()
    }
}

// Promotion
#[derive(Debug, Deserialize)]
pub struct Promotion {
    pub name: Option<String>,
    pub pipeline_id: Option<String>,
    pub status: Option<String>,
}

// Organization
#[derive(Debug, Deserialize)]
pub struct Organization {
    pub org_id: String,
    pub org_username: String,
    #[serde(default)]
    pub org_name: Option<String>,
}

// Project
#[derive(Debug, Deserialize)]
pub struct Project {
    pub metadata: ProjectMetadata,
}

#[derive(Debug, Deserialize)]
pub struct ProjectMetadata {
    pub id: String,
    pub name: String,
}

// Log response (raw API returns {events: [...]})
#[derive(Debug, Deserialize)]
pub struct LogResponse {
    pub events: Vec<LogEvent>,
}

#[derive(Debug, Deserialize)]
pub struct LogEvent {
    pub event: String,
    pub timestamp: i64,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub directive: Option<String>,
    #[serde(default)]
    pub exit_code: Option<i32>,
}

impl SemaphoreClient {
    pub fn new(config: &Config) -> Self {
        Self {
            client: Client::new(),
            base_url: config.base_url(),
            token: config.token.clone(),
        }
    }

    async fn post<B: Serialize, T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Token {}", self.token))
            .json(body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(TbSemError::Api {
                status,
                message: body,
            });
        }

        Ok(resp.json().await?)
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Token {}", self.token))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(TbSemError::Api {
                status,
                message: body,
            });
        }

        Ok(resp.json().await?)
    }

    async fn get_paginated<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        max_pages: usize,
    ) -> Result<Vec<T>> {
        let next_re = Regex::new(r#"<([^>]+)>;\s*rel="next""#).unwrap();
        let mut all_items: Vec<T> = Vec::new();
        let mut url = format!("{}{}", self.base_url, path);

        for _ in 0..max_pages {
            let resp = self
                .client
                .get(&url)
                .header("Authorization", format!("Token {}", self.token))
                .send()
                .await?;

            if !resp.status().is_success() {
                let status = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                return Err(TbSemError::Api {
                    status,
                    message: body,
                });
            }

            let link_header = resp
                .headers()
                .get("link")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());

            let page: Vec<T> = resp.json().await?;
            let page_empty = page.is_empty();
            all_items.extend(page);

            if page_empty {
                break;
            }

            match link_header.as_deref().and_then(|h| next_re.captures(h)) {
                Some(caps) => {
                    // Link header may contain http:// URLs that redirect to https://,
                    // which causes reqwest to strip the Authorization header.
                    url = caps[1].replace("http://", "https://");
                }
                None => break,
            }
        }

        Ok(all_items)
    }

    pub async fn list_organizations(&self) -> Result<Vec<Organization>> {
        self.get("/orgs").await
    }

    pub async fn list_projects(&self) -> Result<Vec<Project>> {
        self.get("/projects").await
    }

    pub async fn list_workflows(
        &self,
        project_id: &str,
        branch: Option<&str>,
        created_after: Option<i64>,
        created_before: Option<i64>,
    ) -> Result<Vec<Workflow>> {
        self.list_workflows_pages(project_id, branch, created_after, created_before, 10)
            .await
    }

    pub async fn list_workflows_pages(
        &self,
        project_id: &str,
        branch: Option<&str>,
        created_after: Option<i64>,
        created_before: Option<i64>,
        max_pages: usize,
    ) -> Result<Vec<Workflow>> {
        let mut path = format!("/plumber-workflows?project_id={}", project_id);
        if let Some(b) = branch {
            path.push_str(&format!("&branch_name={}", b));
        }
        if let Some(ts) = created_after {
            path.push_str(&format!("&created_after={}", ts));
        }
        if let Some(ts) = created_before {
            path.push_str(&format!("&created_before={}", ts));
        }
        self.get_paginated(&path, max_pages).await
    }

    pub async fn get_pipeline(&self, pipeline_id: &str, detailed: bool) -> Result<Pipeline> {
        let mut path = format!("/pipelines/{}", pipeline_id);
        if detailed {
            path.push_str("?detailed=true");
        }
        let resp: PipelineResponse = self.get(&path).await?;
        let mut pipeline = resp.pipeline;
        pipeline.blocks = resp.blocks;
        Ok(pipeline)
    }

    pub async fn list_pipelines_for_workflow(
        &self,
        workflow_id: &str,
    ) -> Result<Vec<PipelineListItem>> {
        self.get(&format!("/pipelines?wf_id={}", workflow_id)).await
    }

    pub async fn list_promotions(&self, pipeline_id: &str) -> Result<Vec<Promotion>> {
        self.get(&format!("/promotions?pipeline_id={}", pipeline_id))
            .await
    }

    pub async fn get_job_logs(&self, job_id: &str) -> Result<Vec<LogEvent>> {
        let resp: LogResponse = self.get(&format!("/logs/{}", job_id)).await?;
        Ok(resp.events)
    }

    pub async fn run_workflow(&self, request: &RunWorkflowRequest) -> Result<RunWorkflowResponse> {
        self.post("/plumber-workflows", request).await
    }
}

#[derive(Debug, Serialize)]
pub struct RunWorkflowRequest {
    pub project_id: String,
    pub reference: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pipeline_file: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RunWorkflowResponse {
    pub workflow_id: String,
    pub pipeline_id: String,
}
