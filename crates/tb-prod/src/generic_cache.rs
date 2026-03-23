use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::api::{ProductiveClient, Query, Resource};
use crate::error::{Result, TbProdError};
use crate::schema::{self, CacheScope, ResourceDef, Schema};

const TTL_SECS: u64 = 24 * 60 * 60; // 24 hours

/// A cached record: just the fields specified in the cache config.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CachedRecord {
    pub id: String,
    pub fields: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CacheFile {
    data: Vec<CachedRecord>,
}

/// Two-tier cache: org-wide and project-scoped.
pub struct GenericCache {
    org_dir: PathBuf,
}

impl GenericCache {
    pub fn new(org_id: &str) -> Result<Self> {
        let org_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("tb-prod")
            .join(org_id);
        std::fs::create_dir_all(&org_dir)?;
        Ok(Self { org_dir })
    }

    /// Sync all org-wide cacheable types.
    pub async fn sync_org(&self, client: &ProductiveClient) -> Result<()> {
        let schema = schema::schema();
        let mut futures = Vec::new();

        for resource in schema.resources.values() {
            if let Some(cache_config) = &resource.cache {
                if cache_config.enabled && cache_config.scope == CacheScope::Org {
                    futures.push(self.sync_resource(client, resource));
                }
            }
        }

        eprintln!("Syncing {} org-wide cache types...", futures.len());
        // Execute sequentially to avoid hammering the API
        for fut in futures {
            match fut.await {
                Ok(()) => {}
                Err(TbProdError::Api { status: 403, .. }) => {
                    // Access denied — skip this type (user lacks permission)
                }
                Err(e) => return Err(e),
            }
        }
        eprintln!("Cache synced.");
        Ok(())
    }

    /// Sync a single resource type to cache.
    async fn sync_resource(&self, client: &ProductiveClient, resource: &ResourceDef) -> Result<()> {
        let cache_config = resource.cache.as_ref().ok_or_else(|| {
            TbProdError::Other(format!("{} is not cacheable", resource.type_name))
        })?;

        let mut query = Query::new();
        if let Some(sync_filter) = &cache_config.sync_filter {
            for (key, value) in sync_filter {
                query = query.filter(key, value);
            }
        }

        let path = resource.api_path();
        let resp = client.get_all(&path, &query, 10).await?;

        let records: Vec<CachedRecord> = resp
            .data
            .iter()
            .map(|r| extract_cached_record(r, &cache_config.fields, resource))
            .collect();

        let filename = format!("{}.json", resource.type_name);
        eprintln!("  {} — {} records", resource.type_name, records.len());
        self.write_org_cache(&filename, &records)?;
        Ok(())
    }

    /// Sync project-scoped types for a specific project.
    pub async fn sync_project(
        &self,
        client: &ProductiveClient,
        project_id: &str,
        workflow_id: Option<&str>,
    ) -> Result<()> {
        let schema = schema::schema();

        for resource in schema.resources.values() {
            if let Some(cache_config) = &resource.cache {
                if cache_config.scope == CacheScope::Project {
                    let mut query = Query::new();

                    // Build scope filter based on resource type
                    match resource.type_name.as_str() {
                        "task_lists" | "folders" => {
                            query = query.filter("project_id", project_id);
                        }
                        "workflow_statuses" => {
                            if let Some(wid) = workflow_id {
                                query = query.filter_array("workflow_id", wid);
                            } else {
                                continue; // skip if no workflow context
                            }
                        }
                        "workflows" => {
                            if let Some(wid) = workflow_id {
                                query = query.filter("id", wid);
                            } else {
                                continue;
                            }
                        }
                        _ => continue,
                    }

                    let path = resource.api_path();
                    let resp = client.get_all(&path, &query, 5).await?;

                    let records: Vec<CachedRecord> = resp
                        .data
                        .iter()
                        .map(|r| extract_cached_record(r, &cache_config.fields, resource))
                        .collect();

                    self.write_project_cache(project_id, &resource.type_name, &records)?;
                }
            }
        }

        Ok(())
    }

    /// Resolve a name to an ID using the cache.
    /// Returns Ok(id) on unique match, Err on 0 or 2+ matches.
    pub fn resolve_name(
        &self,
        resource_type: &str,
        name_or_id: &str,
        project_id: Option<&str>,
    ) -> Result<String> {
        if name_or_id.chars().all(|c| c.is_ascii_digit()) {
            return Ok(name_or_id.to_string());
        }

        let schema = schema::schema();
        let resource = schema.resources.get(resource_type).ok_or_else(|| {
            TbProdError::Other(format!("Unknown resource type: {}", resource_type))
        })?;
        let cache_config = resource.cache.as_ref().ok_or_else(|| {
            TbProdError::Other(format!("{} is not cacheable", resource_type))
        })?;

        let records = match cache_config.scope {
            CacheScope::Org => self.read_org_cache(resource_type)?,
            CacheScope::Project => {
                let pid = project_id.ok_or_else(|| {
                    TbProdError::Other(format!(
                        "Cannot resolve {} name without project context.",
                        resource_type
                    ))
                })?;
                self.read_project_cache(pid, resource_type)?
            }
        };

        let display_field = &cache_config.display_field;
        fuzzy_resolve(&records, name_or_id, display_field, resource_type)
    }

    /// Read org-wide cache, returns empty vec if not found.
    pub fn read_org_cache(&self, resource_type: &str) -> Result<Vec<CachedRecord>> {
        let filename = format!("{}.json", resource_type);
        let path = self.org_dir.join(&filename);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = std::fs::read_to_string(&path)?;
        let cache: CacheFile = serde_json::from_str(&content)?;
        Ok(cache.data)
    }

    /// Read project-scoped cache, returns empty vec if not found.
    pub fn read_project_cache(&self, project_id: &str, resource_type: &str) -> Result<Vec<CachedRecord>> {
        let path = self.project_cache_path(project_id, resource_type);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = std::fs::read_to_string(&path)?;
        let cache: CacheFile = serde_json::from_str(&content)?;
        Ok(cache.data)
    }

    /// Backfill project cache from API results.
    pub fn backfill_project_cache(
        &self,
        project_id: &str,
        resource_type: &str,
        records: &[CachedRecord],
    ) -> Result<()> {
        self.write_project_cache(project_id, resource_type, records)
    }

    /// Check if an org-wide cache file is stale (>24h old) or missing.
    pub fn is_org_stale(&self, resource_type: &str) -> bool {
        let filename = format!("{}.json", resource_type);
        is_stale(&self.org_dir.join(filename))
    }

    /// Clear all caches (org-wide and project-scoped).
    pub fn clear_all(&self) -> Result<()> {
        // Remove all JSON files in org dir
        if let Ok(entries) = std::fs::read_dir(&self.org_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "json") {
                    std::fs::remove_file(&path)?;
                }
            }
        }
        // Remove projects subdirectory
        let projects_dir = self.org_dir.join("projects");
        if projects_dir.exists() {
            std::fs::remove_dir_all(&projects_dir)?;
        }
        eprintln!("Cache cleared.");
        Ok(())
    }

    // --- Internal ---

    fn write_org_cache(&self, filename: &str, records: &[CachedRecord]) -> Result<()> {
        let path = self.org_dir.join(filename);
        let cache = CacheFile { data: records.to_vec() };
        let json = serde_json::to_string_pretty(&cache)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    fn write_project_cache(
        &self,
        project_id: &str,
        resource_type: &str,
        records: &[CachedRecord],
    ) -> Result<()> {
        let dir = self.org_dir.join("projects").join(project_id);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.json", resource_type));
        let cache = CacheFile { data: records.to_vec() };
        let json = serde_json::to_string_pretty(&cache)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    fn project_cache_path(&self, project_id: &str, resource_type: &str) -> PathBuf {
        self.org_dir
            .join("projects")
            .join(project_id)
            .join(format!("{}.json", resource_type))
    }
}

// --- Helpers ---

fn is_stale(path: &std::path::Path) -> bool {
    match path.metadata().and_then(|m| m.modified()) {
        Ok(modified) => {
            let age = std::time::SystemTime::now()
                .duration_since(modified)
                .unwrap_or_default();
            age.as_secs() > TTL_SECS
        }
        Err(_) => true,
    }
}

/// Extract a CachedRecord from a JSONAPI Resource, keeping only the specified fields.
fn extract_cached_record(
    resource: &Resource,
    fields: &[String],
    resource_def: &ResourceDef,
) -> CachedRecord {
    let mut field_map = HashMap::new();

    for field_name in fields {
        if field_name == "id" {
            field_map.insert("id".to_string(), resource.id.clone());
            continue;
        }

        // Try attribute first
        if let Some(val) = resource.attributes.get(field_name) {
            let str_val = match val {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                serde_json::Value::Null => String::new(),
                other => other.to_string(),
            };
            field_map.insert(field_name.clone(), str_val);
            continue;
        }

        // Try relationship (look up by field definition)
        if let Some(field_def) = resource_def.fields.values().find(|f| {
            f.attribute.as_deref() == Some(field_name)
                || f.key == *field_name
                || f.param.as_deref() == Some(field_name)
        }) {
            if let Some(rel_name) = &field_def.relationship {
                if let Some(id) = resource.relationship_id(rel_name) {
                    field_map.insert(field_name.clone(), id.to_string());
                    continue;
                }
            }
        }

        // Try as direct relationship name
        if let Some(id) = resource.relationship_id(field_name) {
            field_map.insert(field_name.clone(), id.to_string());
            continue;
        }

        // Field not found — store empty
        field_map.insert(field_name.clone(), String::new());
    }

    CachedRecord {
        id: resource.id.clone(),
        fields: field_map,
    }
}

/// Fuzzy substring match on the display field, returning the ID on unique match.
fn fuzzy_resolve(
    records: &[CachedRecord],
    needle: &str,
    display_field: &str,
    resource_type: &str,
) -> Result<String> {
    let lower_needle = needle.to_lowercase();

    // For people, match against "first_name last_name" combined or email
    let matches: Vec<&CachedRecord> = if resource_type == "people" {
        records
            .iter()
            .filter(|r| {
                let first = r.fields.get("first_name").map(|s| s.as_str()).unwrap_or("");
                let last = r.fields.get("last_name").map(|s| s.as_str()).unwrap_or("");
                let email = r.fields.get("email").map(|s| s.as_str()).unwrap_or("");
                let full_name = format!("{} {}", first, last).to_lowercase();
                full_name.contains(&lower_needle) || email.to_lowercase().contains(&lower_needle)
            })
            .collect()
    } else {
        records
            .iter()
            .filter(|r| {
                r.fields
                    .get(display_field)
                    .map(|v| v.to_lowercase().contains(&lower_needle))
                    .unwrap_or(false)
            })
            .collect()
    };

    match matches.len() {
        0 => {
            let available: Vec<String> = records
                .iter()
                .take(20)
                .map(|r| {
                    let name = display_name(r, display_field, resource_type);
                    format!("  {} ({})", name, r.id)
                })
                .collect();
            Err(TbProdError::Other(format!(
                "No {} matching '{}'. Available:\n{}",
                resource_type,
                needle,
                available.join("\n")
            )))
        }
        1 => Ok(matches[0].id.clone()),
        _ => {
            let ambiguous: Vec<String> = matches
                .iter()
                .map(|r| {
                    let name = display_name(r, display_field, resource_type);
                    format!("  {} ({})", name, r.id)
                })
                .collect();
            Err(TbProdError::Other(format!(
                "Ambiguous {} '{}'. Matches:\n{}",
                resource_type,
                needle,
                ambiguous.join("\n")
            )))
        }
    }
}

fn display_name(record: &CachedRecord, display_field: &str, resource_type: &str) -> String {
    if resource_type == "people" {
        let first = record.fields.get("first_name").map(|s| s.as_str()).unwrap_or("");
        let last = record.fields.get("last_name").map(|s| s.as_str()).unwrap_or("");
        format!("{} {}", first, last).trim().to_string()
    } else {
        record
            .fields
            .get(display_field)
            .cloned()
            .unwrap_or_else(|| record.id.clone())
    }
}

/// Two-pass name resolution for filter values.
/// Pass 1: resolve org-wide types.
/// Pass 2: resolve project-scoped types using context from pass 1.
/// Recurses into nested FilterGroup entries.
pub fn resolve_filter_names(
    cache: &GenericCache,
    conditions: &mut [crate::filter::FilterEntry],
    resource: &ResourceDef,
    schema: &Schema,
) -> std::result::Result<(), String> {
    let mut resolved_project_id: Option<String> = None;

    // Pass 1: resolve org-wide names, collect project_id for scoping
    resolve_pass(cache, conditions, resource, schema, CacheScope::Org, &mut resolved_project_id)?;

    // Pass 2: resolve project-scoped names using context from pass 1
    resolve_pass(cache, conditions, resource, schema, CacheScope::Project, &mut resolved_project_id)?;

    Ok(())
}

fn resolve_pass(
    cache: &GenericCache,
    conditions: &mut [crate::filter::FilterEntry],
    resource: &ResourceDef,
    schema: &Schema,
    target_scope: CacheScope,
    resolved_project_id: &mut Option<String>,
) -> std::result::Result<(), String> {
    for entry in conditions.iter_mut() {
        match entry {
            crate::filter::FilterEntry::Condition(cond) => {
                if let Some(field) = crate::filter::resolve_filter_field(&cond.field, resource) {
                    if field.type_category == schema::TypeCategory::Resource {
                        if let Some(target_resource) = schema.resources.get(&field.field_type) {
                            if let Some(cache_config) = &target_resource.cache {
                                if cache_config.enabled && cache_config.scope == target_scope {
                                    let project_ctx = if target_scope == CacheScope::Project {
                                        resolved_project_id.as_deref()
                                    } else {
                                        None
                                    };
                                    resolve_condition_values(cache, cond, &field.field_type, project_ctx)?;

                                    if target_scope == CacheScope::Org && field.field_type == "projects" {
                                        if let crate::filter::FilterValue::Single(ref v) = cond.value {
                                            *resolved_project_id = Some(v.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            crate::filter::FilterEntry::Group(group) => {
                resolve_pass(cache, &mut group.conditions, resource, schema, target_scope, resolved_project_id)?;
            }
        }
    }
    Ok(())
}

fn resolve_condition_values(
    cache: &GenericCache,
    cond: &mut crate::filter::FilterCondition,
    resource_type: &str,
    project_id: Option<&str>,
) -> std::result::Result<(), String> {
    match &mut cond.value {
        crate::filter::FilterValue::Single(v) => {
            if !v.chars().all(|c| c.is_ascii_digit()) {
                *v = cache
                    .resolve_name(resource_type, v, project_id)
                    .map_err(|e| e.to_string())?;
            }
        }
        crate::filter::FilterValue::Array(values) => {
            for v in values.iter_mut() {
                if !v.chars().all(|c| c.is_ascii_digit()) {
                    *v = cache
                        .resolve_name(resource_type, v, project_id)
                        .map_err(|e| e.to_string())?;
                }
            }
        }
    }
    Ok(())
}
