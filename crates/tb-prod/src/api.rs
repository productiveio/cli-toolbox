use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::error::{Result, TbProdError};

fn map_middleware_err(e: reqwest_middleware::Error) -> TbProdError {
    match e {
        reqwest_middleware::Error::Reqwest(re) => TbProdError::Http(re),
        reqwest_middleware::Error::Middleware(ae) => TbProdError::Other(ae.to_string()),
    }
}

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
        self.attributes
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
    }

    pub fn attr_i64(&self, key: &str) -> Option<i64> {
        self.attributes.get(key).and_then(|v| v.as_i64())
    }

    pub fn attr_bool(&self, key: &str) -> bool {
        self.attributes
            .get(key)
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    pub fn relationship_id(&self, name: &str) -> Option<&str> {
        self.relationships
            .get(name)
            .and_then(|r| r.get("data"))
            .and_then(|d| d.get("id"))
            .and_then(|id| id.as_str())
    }

    /// Extract IDs from a to-many relationship (where `data` is an array).
    pub fn relationship_ids(&self, name: &str) -> Vec<&str> {
        self.relationships
            .get(name)
            .and_then(|r| r.get("data"))
            .and_then(|d| d.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| item.get("id").and_then(|id| id.as_str()))
                    .collect()
            })
            .unwrap_or_default()
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

#[derive(Default)]
pub struct Query {
    filters: Vec<(String, String)>,
    params: Vec<(String, String)>,
}

impl Query {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a simple filter: filter[field]=value
    pub fn filter(mut self, field: &str, value: &str) -> Self {
        self.filters
            .push((format!("filter[{}]", field), value.to_string()));
        self
    }

    /// Add an array filter: filter[field][]=value
    pub fn filter_array(mut self, field: &str, value: &str) -> Self {
        self.filters
            .push((format!("filter[{}][]", field), value.to_string()));
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
        self.filters
            .push(("filter[$op]".to_string(), op.to_string()));
        self
    }

    /// Add a raw filter key-value pair (for nested group serialization).
    pub fn filter_raw(mut self, key: String, value: String) -> Self {
        self.filters.push((key, value));
        self
    }

    pub fn include(mut self, includes: &str) -> Self {
        self.params
            .push(("include".to_string(), includes.to_string()));
        self
    }

    pub fn sort(mut self, sort: &str) -> Self {
        self.params.push(("sort".to_string(), sort.to_string()));
        self
    }

    pub fn page(mut self, number: usize, size: usize) -> Self {
        self.params
            .push(("page[number]".to_string(), number.to_string()));
        self.params
            .push(("page[size]".to_string(), size.to_string()));
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
    client: ClientWithMiddleware,
    token: String,
    org_id: String,
    base_url: String,
}

impl ProductiveClient {
    pub fn new(config: &Config) -> Self {
        Self {
            client: ClientBuilder::new(reqwest::Client::new()).build(),
            token: config.token.clone(),
            org_id: config.org_id.clone(),
            base_url: config.base_url().to_string(),
        }
    }

    /// Create a client with an injected middleware client (for testing with VCR).
    pub fn with_client(
        client: ClientWithMiddleware,
        token: &str,
        org_id: &str,
        base_url: &str,
    ) -> Self {
        Self {
            client,
            token: token.to_string(),
            org_id: org_id.to_string(),
            base_url: base_url.to_string(),
        }
    }

    pub fn org_id(&self) -> &str {
        &self.org_id
    }

    fn request(&self, method: reqwest::Method, url: &str) -> reqwest_middleware::RequestBuilder {
        self.client
            .request(method, url.parse::<reqwest::Url>().expect("valid URL"))
            .header("Content-Type", "application/vnd.api+json")
            .header("X-Auth-Token", &self.token)
            .header("X-Organization-Id", &self.org_id)
    }

    /// GET a single JSONAPI resource.
    pub async fn get_one(&self, path: &str) -> Result<JsonApiSingleResponse> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .request(reqwest::Method::GET, &url)
            .send()
            .await
            .map_err(map_middleware_err)?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(TbProdError::Api {
                status,
                message: body,
            });
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
        let resp = self
            .request(reqwest::Method::GET, &url)
            .send()
            .await
            .map_err(map_middleware_err)?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(TbProdError::Api {
                status,
                message: body,
            });
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
            let resp = self
                .request(reqwest::Method::GET, &url)
                .send()
                .await
                .map_err(map_middleware_err)?;
            let status = resp.status().as_u16();
            if !resp.status().is_success() {
                let body = resp.text().await.unwrap_or_default();
                return Err(TbProdError::Api {
                    status,
                    message: body,
                });
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
    pub async fn create(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<JsonApiSingleResponse> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .request(reqwest::Method::POST, &url)
            .json(body)
            .send()
            .await
            .map_err(map_middleware_err)?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(TbProdError::Api {
                status,
                message: body,
            });
        }
        Ok(resp.json().await?)
    }

    /// PATCH a JSONAPI resource. Returns the updated resource.
    pub async fn update(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<JsonApiSingleResponse> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .request(reqwest::Method::PATCH, &url)
            .json(body)
            .send()
            .await
            .map_err(map_middleware_err)?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(TbProdError::Api {
                status,
                message: body,
            });
        }
        Ok(resp.json().await?)
    }

    // --- Generic resource operations ---

    /// DELETE a JSONAPI resource.
    pub async fn delete(&self, path: &str) -> Result<()> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .request(reqwest::Method::DELETE, &url)
            .send()
            .await
            .map_err(map_middleware_err)?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(TbProdError::Api {
                status,
                message: body,
            });
        }
        Ok(())
    }

    /// Execute a custom action on a resource (arbitrary method + path).
    pub async fn custom_action(
        &self,
        path: &str,
        method: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<Option<JsonApiSingleResponse>> {
        let url = format!("{}{}", self.base_url, path);
        let http_method = match method.to_uppercase().as_str() {
            "POST" => reqwest::Method::POST,
            "PUT" => reqwest::Method::PUT,
            "PATCH" => reqwest::Method::PATCH,
            "DELETE" => reqwest::Method::DELETE,
            _ => reqwest::Method::POST,
        };
        let mut req = self.request(http_method, &url);
        if let Some(b) = body {
            req = req.json(b);
        }
        let resp = req.send().await.map_err(map_middleware_err)?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(TbProdError::Api {
                status,
                message: body,
            });
        }
        // Some actions return 204 No Content
        if status == 204 {
            return Ok(None);
        }
        let text = resp.text().await?;
        if text.is_empty() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_str(&text)?))
    }

    /// POST with JSONAPI bulk extension. Works for any resource type.
    pub async fn bulk_create(
        &self,
        path: &str,
        payload: &serde_json::Value,
    ) -> Result<JsonApiResponse> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .post(&url)
            .header("Content-Type", "application/vnd.api+json; ext=bulk")
            .header("Accept", "application/vnd.api+json; ext=bulk")
            .header("X-Auth-Token", &self.token)
            .header("X-Organization-Id", &self.org_id)
            .json(payload)
            .send()
            .await
            .map_err(map_middleware_err)?;
        let status = resp.status().as_u16();
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(TbProdError::Api {
                status,
                message: body,
            });
        }
        Ok(resp.json().await?)
    }
}
