use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::error::{Result, TbProdError};

// --- JSONAPI response types ---

/// A single JSONAPI resource.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Resource {
    pub id: String,
    #[serde(rename = "type")]
    pub resource_type: String,
    #[serde(default)]
    pub attributes: serde_json::Value,
    #[serde(default)]
    pub relationships: serde_json::Value,
}

impl Resource {
    pub fn attr_str(&self, key: &str) -> &str {
        self.attributes.get(key).and_then(|v| v.as_str()).unwrap_or("")
    }

    pub fn attr_i64(&self, key: &str) -> Option<i64> {
        self.attributes.get(key).and_then(|v| v.as_i64())
    }

    pub fn attr_bool(&self, key: &str) -> bool {
        self.attributes.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
    }

    pub fn relationship_id(&self, name: &str) -> Option<&str> {
        self.relationships
            .get(name)
            .and_then(|r| r.get("data"))
            .and_then(|d| d.get("id"))
            .and_then(|id| id.as_str())
    }
}

/// A JSONAPI list response with pagination meta.
#[derive(Debug, Deserialize)]
pub struct JsonApiResponse {
    #[serde(default)]
    pub data: Vec<Resource>,
    #[serde(default)]
    pub included: Vec<Resource>,
    #[serde(default)]
    pub meta: serde_json::Value,
}

/// A JSONAPI single-resource response.
#[derive(Debug, Deserialize)]
pub struct JsonApiSingleResponse {
    pub data: Resource,
    #[serde(default)]
    pub included: Vec<Resource>,
}

// --- Query builder for Productive's indexed filter syntax ---

pub struct Query {
    filters: Vec<(String, String)>,
    params: Vec<(String, String)>,
}

impl Query {
    pub fn new() -> Self {
        Self {
            filters: Vec::new(),
            params: Vec::new(),
        }
    }

    /// Add a simple filter: filter[field]=value
    pub fn filter(mut self, field: &str, value: &str) -> Self {
        self.filters.push((format!("filter[{}]", field), value.to_string()));
        self
    }

    /// Add an array filter: filter[field][]=value
    pub fn filter_array(mut self, field: &str, value: &str) -> Self {
        self.filters.push((format!("filter[{}][]", field), value.to_string()));
        self
    }

    /// Add an indexed filter with operator: filter[N][field][op][]=value
    /// These are combined with AND when $op=and is set.
    pub fn filter_indexed(mut self, index: usize, field: &str, op: &str, value: &str) -> Self {
        self.filters.push((
            format!("filter[{}][{}][{}][]", index, field, op),
            value.to_string(),
        ));
        self
    }

    /// Set the logical operator for indexed filters.
    pub fn filter_op(mut self, op: &str) -> Self {
        self.filters.push(("filter[$op]".to_string(), op.to_string()));
        self
    }

    pub fn include(mut self, includes: &str) -> Self {
        self.params.push(("include".to_string(), includes.to_string()));
        self
    }

    pub fn sort(mut self, sort: &str) -> Self {
        self.params.push(("sort".to_string(), sort.to_string()));
        self
    }

    pub fn page(mut self, number: usize, size: usize) -> Self {
        self.params.push(("page[number]".to_string(), number.to_string()));
        self.params.push(("page[size]".to_string(), size.to_string()));
        self
    }

    pub fn to_query_string(&self) -> String {
        let all: Vec<_> = self
            .filters
            .iter()
            .chain(self.params.iter())
            .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
            .collect();
        if all.is_empty() {
            String::new()
        } else {
            format!("?{}", all.join("&"))
        }
    }
}

// --- Client ---

pub struct ProductiveClient {
    client: Client,
    token: String,
    org_id: String,
    base_url: String,
}

impl ProductiveClient {
    pub fn new(config: &Config) -> Self {
        Self {
            client: Client::new(),
            token: config.token.clone(),
            org_id: config.org_id.clone(),
            base_url: config.base_url().to_string(),
        }
    }

    fn request(&self, method: reqwest::Method, url: &str) -> reqwest::RequestBuilder {
        self.client
            .request(method, url)
            .header("Content-Type", "application/vnd.api+json")
            .header("X-Auth-Token", &self.token)
            .header("X-Organization-Id", &self.org_id)
    }

    /// GET a single JSONAPI resource.
    pub async fn get_one(&self, path: &str) -> Result<JsonApiSingleResponse> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.request(reqwest::Method::GET, &url).send().await?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(TbProdError::Api { status, message: body });
        }
        Ok(resp.json().await?)
    }

    /// GET a single page of a JSONAPI collection.
    pub async fn get_page(
        &self,
        path: &str,
        query: &Query,
        page: usize,
        page_size: usize,
    ) -> Result<JsonApiResponse> {
        let page_query = Query::new().page(page, page_size);
        let qs = query.to_query_string();
        let page_qs = page_query.to_query_string();
        let full_qs = if qs.is_empty() {
            page_qs
        } else {
            format!("{}&{}", qs, &page_qs[1..])
        };
        let url = format!("{}{}{}", self.base_url, path, full_qs);
        let resp = self.request(reqwest::Method::GET, &url).send().await?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(TbProdError::Api { status, message: body });
        }
        Ok(resp.json().await?)
    }

    /// GET a paginated JSONAPI collection. Fetches all pages up to max_pages.
    pub async fn get_all(
        &self,
        path: &str,
        query: &Query,
        max_pages: usize,
    ) -> Result<JsonApiResponse> {
        let mut all_data: Vec<Resource> = Vec::new();
        let mut all_included: Vec<Resource> = Vec::new();
        let mut last_meta = serde_json::Value::Null;

        for page_num in 1..=max_pages {
            let page_query = Query::new().page(page_num, 200);
            let qs = query.to_query_string();
            let page_qs = page_query.to_query_string();
            // Merge query strings
            let full_qs = if qs.is_empty() {
                page_qs
            } else {
                format!("{}&{}", qs, &page_qs[1..]) // skip the ? from page_qs
            };
            let url = format!("{}{}{}", self.base_url, path, full_qs);

            eprintln!("Fetching page {}...", page_num);
            let resp = self.request(reqwest::Method::GET, &url).send().await?;
            let status = resp.status().as_u16();
            if !resp.status().is_success() {
                let body = resp.text().await.unwrap_or_default();
                return Err(TbProdError::Api { status, message: body });
            }

            let page: JsonApiResponse = resp.json().await?;
            let total_pages = page
                .meta
                .get("total_pages")
                .and_then(|v| v.as_u64())
                .unwrap_or(1) as usize;

            all_data.extend(page.data);
            all_included.extend(page.included);
            last_meta = page.meta;

            if page_num >= total_pages {
                break;
            }
        }

        Ok(JsonApiResponse {
            data: all_data,
            included: all_included,
            meta: last_meta,
        })
    }

    /// POST a JSONAPI resource. Returns the created resource.
    pub async fn create(&self, path: &str, body: &serde_json::Value) -> Result<JsonApiSingleResponse> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .request(reqwest::Method::POST, &url)
            .json(body)
            .send()
            .await?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(TbProdError::Api { status, message: body });
        }
        Ok(resp.json().await?)
    }

    /// PATCH a JSONAPI resource. Returns the updated resource.
    pub async fn update(&self, path: &str, body: &serde_json::Value) -> Result<JsonApiSingleResponse> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .request(reqwest::Method::PATCH, &url)
            .json(body)
            .send()
            .await?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(TbProdError::Api { status, message: body });
        }
        Ok(resp.json().await?)
    }

    // --- Convenience methods ---

    pub async fn list_tasks(&self, query: &Query) -> Result<JsonApiResponse> {
        self.get_all("/tasks", query, 10).await
    }

    pub async fn get_task(&self, id: &str) -> Result<JsonApiSingleResponse> {
        let path = format!(
            "/tasks/{}?include=project,assignee,workflow_status,task_list,parent_task,creator",
            id
        );
        self.get_one(&path).await
    }

    pub async fn create_task(&self, payload: &serde_json::Value) -> Result<JsonApiSingleResponse> {
        self.create("/tasks", payload).await
    }

    pub async fn update_task(&self, id: &str, payload: &serde_json::Value) -> Result<JsonApiSingleResponse> {
        self.update(&format!("/tasks/{}", id), payload).await
    }

    pub async fn create_comment(&self, payload: &serde_json::Value) -> Result<JsonApiSingleResponse> {
        self.create("/comments", payload).await
    }

    pub async fn list_task_lists(&self, query: &Query) -> Result<JsonApiResponse> {
        self.get_all("/task_lists", query, 5).await
    }

    pub async fn list_workflow_statuses(&self, query: &Query) -> Result<JsonApiResponse> {
        self.get_all("/workflow_statuses", query, 5).await
    }

    pub async fn list_comments(&self, task_id: &str) -> Result<JsonApiResponse> {
        let query = Query::new().filter_array("task_id", task_id);
        self.get_all("/comments", &query, 5).await
    }

    pub async fn get_subtasks(&self, parent_id: &str) -> Result<JsonApiResponse> {
        let query = Query::new()
            .filter_array("parent_task_id", parent_id)
            .include("workflow_status,assignee");
        self.get_all("/tasks", &query, 5).await
    }

    pub async fn get_todos(&self, task_id: &str) -> Result<JsonApiResponse> {
        let query = Query::new().filter_array("task_id", task_id);
        self.get_all("/todos", &query, 5).await
    }
}
