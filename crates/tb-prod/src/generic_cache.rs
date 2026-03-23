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

    /// Create a cache rooted at an arbitrary directory.
    pub fn with_dir(dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&dir)?;
        Ok(Self { org_dir: dir })
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
    /// Note: all-digit strings are always treated as IDs (passed through unchanged).
    /// This means names like "2025" or "404" would not be resolved by fuzzy match.
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
/// Pass 1: resolve org-wide types, collect project IDs for scoping.
/// Pass 2: resolve project-scoped types using all collected project IDs.
/// Recurses into nested FilterGroup entries.
pub fn resolve_filter_names(
    cache: &GenericCache,
    conditions: &mut [crate::filter::FilterEntry],
    resource: &ResourceDef,
    schema: &Schema,
) -> std::result::Result<(), String> {
    let mut resolved_project_ids: Vec<String> = Vec::new();

    // Pass 1: resolve org-wide names, collect project IDs for scoping
    resolve_pass(cache, conditions, resource, schema, CacheScope::Org, &mut resolved_project_ids)?;

    // Pass 2: resolve project-scoped names using project context from pass 1
    resolve_pass(cache, conditions, resource, schema, CacheScope::Project, &mut resolved_project_ids)?;

    Ok(())
}

fn resolve_pass(
    cache: &GenericCache,
    conditions: &mut [crate::filter::FilterEntry],
    resource: &ResourceDef,
    schema: &Schema,
    target_scope: CacheScope,
    resolved_project_ids: &mut Vec<String>,
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
                                        if resolved_project_ids.is_empty() {
                                            None
                                        } else {
                                            // Resolve against all collected projects
                                            resolve_condition_values_multi_project(
                                                cache, cond, &field.field_type, resolved_project_ids,
                                            )?;
                                            continue;
                                        }
                                    } else {
                                        None
                                    };
                                    resolve_condition_values(cache, cond, &field.field_type, project_ctx)?;

                                    // Collect resolved project IDs for pass 2
                                    if target_scope == CacheScope::Org && field.field_type == "projects" {
                                        match &cond.value {
                                            crate::filter::FilterValue::Single(v) => {
                                                resolved_project_ids.push(v.clone());
                                            }
                                            crate::filter::FilterValue::Array(values) => {
                                                resolved_project_ids.extend(values.iter().cloned());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            crate::filter::FilterEntry::Group(group) => {
                resolve_pass(cache, &mut group.conditions, resource, schema, target_scope, resolved_project_ids)?;
            }
        }
    }
    Ok(())
}

/// Resolve a condition's values against multiple project caches.
/// A name must resolve uniquely across all projects — ambiguous matches are an error.
fn resolve_condition_values_multi_project(
    cache: &GenericCache,
    cond: &mut crate::filter::FilterCondition,
    resource_type: &str,
    project_ids: &[String],
) -> std::result::Result<(), String> {
    match &mut cond.value {
        crate::filter::FilterValue::Single(v) => {
            if !v.chars().all(|c| c.is_ascii_digit()) {
                *v = resolve_across_projects(cache, resource_type, v, project_ids)?;
            }
        }
        crate::filter::FilterValue::Array(values) => {
            for v in values.iter_mut() {
                if !v.chars().all(|c| c.is_ascii_digit()) {
                    *v = resolve_across_projects(cache, resource_type, v, project_ids)?;
                }
            }
        }
    }
    Ok(())
}

/// Try resolving a name against each project's cache.
/// Returns the ID if exactly one project resolves it; errors on 0 or 2+ matches.
fn resolve_across_projects(
    cache: &GenericCache,
    resource_type: &str,
    name: &str,
    project_ids: &[String],
) -> std::result::Result<String, String> {
    let mut found: Vec<(String, String)> = Vec::new(); // (project_id, resolved_id)

    for pid in project_ids {
        match cache.resolve_name(resource_type, name, Some(pid)) {
            Ok(id) => found.push((pid.clone(), id)),
            Err(_) => {} // no match in this project — try next
        }
    }

    match found.len() {
        0 => Err(format!(
            "No {} matching '{}' in any of the filtered projects ({}).",
            resource_type, name, project_ids.join(", ")
        )),
        1 => Ok(found[0].1.clone()),
        _ => {
            let details: Vec<String> = found
                .iter()
                .map(|(pid, id)| format!("  project {} → {} ({})", pid, name, id))
                .collect();
            Err(format!(
                "Ambiguous {} '{}' — found in multiple projects:\n{}\nUse the numeric ID instead.",
                resource_type, name, details.join("\n")
            ))
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_record(id: &str, fields: &[(&str, &str)]) -> CachedRecord {
        CachedRecord {
            id: id.to_string(),
            fields: fields.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        }
    }

    fn setup_cache_with_org_records(records: &[CachedRecord], resource_type: &str) -> (tempfile::TempDir, GenericCache) {
        let tmp = tempfile::tempdir().unwrap();
        let cache = GenericCache::with_dir(tmp.path().to_path_buf()).unwrap();
        cache.write_org_cache(&format!("{}.json", resource_type), records).unwrap();
        (tmp, cache)
    }

    // --- org cache read/write ---

    #[test]
    fn write_and_read_org_cache() {
        let records = vec![
            make_record("1", &[("name", "Alpha")]),
            make_record("2", &[("name", "Beta")]),
        ];
        let (_tmp, cache) = setup_cache_with_org_records(&records, "projects");

        let read = cache.read_org_cache("projects").unwrap();
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].id, "1");
        assert_eq!(read[0].fields["name"], "Alpha");
        assert_eq!(read[1].id, "2");
    }

    #[test]
    fn read_org_cache_missing_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = GenericCache::with_dir(tmp.path().to_path_buf()).unwrap();
        let read = cache.read_org_cache("nonexistent").unwrap();
        assert!(read.is_empty());
    }

    // --- project cache read/write ---

    #[test]
    fn write_and_read_project_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = GenericCache::with_dir(tmp.path().to_path_buf()).unwrap();

        let records = vec![make_record("10", &[("name", "Sprint 1")])];
        cache.write_project_cache("99", "task_lists", &records).unwrap();

        let read = cache.read_project_cache("99", "task_lists").unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].fields["name"], "Sprint 1");
    }

    #[test]
    fn read_project_cache_missing_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = GenericCache::with_dir(tmp.path().to_path_buf()).unwrap();
        let read = cache.read_project_cache("999", "task_lists").unwrap();
        assert!(read.is_empty());
    }

    // --- fuzzy_resolve ---

    #[test]
    fn fuzzy_resolve_unique_match() {
        let records = vec![
            make_record("1", &[("name", "Alpha Project")]),
            make_record("2", &[("name", "Beta Project")]),
        ];
        let id = fuzzy_resolve(&records, "alpha", "name", "projects").unwrap();
        assert_eq!(id, "1");
    }

    #[test]
    fn fuzzy_resolve_case_insensitive() {
        let records = vec![make_record("1", &[("name", "My Project")])];
        let id = fuzzy_resolve(&records, "MY PROJECT", "name", "projects").unwrap();
        assert_eq!(id, "1");
    }

    #[test]
    fn fuzzy_resolve_no_match() {
        let records = vec![make_record("1", &[("name", "Alpha")])];
        let err = fuzzy_resolve(&records, "gamma", "name", "projects").unwrap_err();
        assert!(err.to_string().contains("No projects matching 'gamma'"));
    }

    #[test]
    fn fuzzy_resolve_ambiguous() {
        let records = vec![
            make_record("1", &[("name", "Project Alpha")]),
            make_record("2", &[("name", "Project Alpha v2")]),
        ];
        let err = fuzzy_resolve(&records, "project alpha", "name", "projects").unwrap_err();
        assert!(err.to_string().contains("Ambiguous"));
    }

    #[test]
    fn fuzzy_resolve_people_full_name() {
        let records = vec![
            make_record("1", &[("first_name", "John"), ("last_name", "Doe"), ("email", "john@example.com")]),
            make_record("2", &[("first_name", "Jane"), ("last_name", "Smith"), ("email", "jane@example.com")]),
        ];
        let id = fuzzy_resolve(&records, "john doe", "name", "people").unwrap();
        assert_eq!(id, "1");
    }

    #[test]
    fn fuzzy_resolve_people_by_email() {
        let records = vec![
            make_record("1", &[("first_name", "John"), ("last_name", "Doe"), ("email", "john@example.com")]),
        ];
        let id = fuzzy_resolve(&records, "john@example", "name", "people").unwrap();
        assert_eq!(id, "1");
    }

    // --- resolve_name ---

    #[test]
    fn resolve_name_all_digit_passthrough() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = GenericCache::with_dir(tmp.path().to_path_buf()).unwrap();
        // No cache data needed — all-digit strings bypass resolution
        let id = cache.resolve_name("projects", "12345", None).unwrap();
        assert_eq!(id, "12345");
    }

    #[test]
    fn resolve_name_from_org_cache() {
        let records = vec![
            make_record("42", &[("name", "Acme Corp")]),
        ];
        let (_tmp, cache) = setup_cache_with_org_records(&records, "companies");
        let id = cache.resolve_name("companies", "acme", None).unwrap();
        assert_eq!(id, "42");
    }

    #[test]
    fn resolve_name_project_scoped_needs_project_id() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = GenericCache::with_dir(tmp.path().to_path_buf()).unwrap();
        let err = cache.resolve_name("task_lists", "sprint", None).unwrap_err();
        assert!(err.to_string().contains("without project context"));
    }

    #[test]
    fn resolve_name_project_scoped_with_project_id() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = GenericCache::with_dir(tmp.path().to_path_buf()).unwrap();

        let records = vec![make_record("55", &[("name", "Sprint 3")])];
        cache.write_project_cache("99", "task_lists", &records).unwrap();

        let id = cache.resolve_name("task_lists", "sprint 3", Some("99")).unwrap();
        assert_eq!(id, "55");
    }

    // --- resolve_across_projects ---

    #[test]
    fn resolve_across_projects_unique() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = GenericCache::with_dir(tmp.path().to_path_buf()).unwrap();

        cache.write_project_cache("10", "task_lists", &[make_record("100", &[("name", "Backlog")])]).unwrap();
        cache.write_project_cache("20", "task_lists", &[make_record("200", &[("name", "Sprint")])]).unwrap();

        let id = resolve_across_projects(&cache, "task_lists", "backlog", &["10".into(), "20".into()]).unwrap();
        assert_eq!(id, "100");
    }

    #[test]
    fn resolve_across_projects_ambiguous() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = GenericCache::with_dir(tmp.path().to_path_buf()).unwrap();

        // Same name in two projects
        cache.write_project_cache("10", "task_lists", &[make_record("100", &[("name", "Backlog")])]).unwrap();
        cache.write_project_cache("20", "task_lists", &[make_record("200", &[("name", "Backlog")])]).unwrap();

        let err = resolve_across_projects(&cache, "task_lists", "backlog", &["10".into(), "20".into()]).unwrap_err();
        assert!(err.contains("Ambiguous"));
        assert!(err.contains("multiple projects"));
    }

    #[test]
    fn resolve_across_projects_no_match() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = GenericCache::with_dir(tmp.path().to_path_buf()).unwrap();

        cache.write_project_cache("10", "task_lists", &[make_record("100", &[("name", "Sprint")])]).unwrap();

        let err = resolve_across_projects(&cache, "task_lists", "nonexistent", &["10".into()]).unwrap_err();
        assert!(err.contains("No task_lists matching"));
    }

    // --- is_stale ---

    #[test]
    fn is_stale_missing_file() {
        assert!(is_stale(std::path::Path::new("/nonexistent/path.json")));
    }

    #[test]
    fn is_stale_fresh_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("fresh.json");
        std::fs::write(&path, "{}").unwrap();
        assert!(!is_stale(&path));
    }

    // --- clear_all ---

    #[test]
    fn clear_all_removes_caches() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = GenericCache::with_dir(tmp.path().to_path_buf()).unwrap();

        cache.write_org_cache("projects.json", &[make_record("1", &[("name", "P")])]).unwrap();
        cache.write_project_cache("10", "task_lists", &[make_record("2", &[("name", "T")])]).unwrap();

        cache.clear_all().unwrap();

        assert!(cache.read_org_cache("projects").unwrap().is_empty());
        assert!(cache.read_project_cache("10", "task_lists").unwrap().is_empty());
    }
}
