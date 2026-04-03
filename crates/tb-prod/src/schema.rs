use std::collections::HashMap;
use std::sync::LazyLock;

use serde::Deserialize;

static SCHEMA_JSON: &str = include_str!("../schema.json");

static SCHEMA: LazyLock<Schema> =
    LazyLock::new(|| serde_json::from_str(SCHEMA_JSON).expect("embedded schema.json is invalid"));

pub fn schema() -> &'static Schema {
    &SCHEMA
}

// --- Top-level schema ---

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Schema {
    pub version: String,
    pub resources: HashMap<String, ResourceDef>,
    pub enums: HashMap<String, EnumDef>,
}

impl Schema {
    /// Look up a resource by type name or alias.
    pub fn resolve_resource(&self, input: &str) -> Option<&ResourceDef> {
        // Direct match first
        if let Some(r) = self.resources.get(input) {
            return Some(r);
        }
        // Alias match
        let lower = input.to_lowercase();
        self.resources.values().find(|r| {
            r.aliases
                .as_ref()
                .map(|a| a.iter().any(|alias| alias.to_lowercase() == lower))
                .unwrap_or(false)
        })
    }

    /// Get all resource types sorted alphabetically.
    pub fn resource_types_sorted(&self) -> Vec<&ResourceDef> {
        let mut types: Vec<_> = self.resources.values().collect();
        types.sort_by_key(|r| &r.type_name);
        types
    }

    /// Get resource types grouped by domain, preserving group order.
    pub fn resources_by_domain(&self) -> Vec<(&str, Vec<&ResourceDef>)> {
        let mut domain_order: Vec<&str> = Vec::new();
        let mut by_domain: HashMap<&str, Vec<&ResourceDef>> = HashMap::new();

        for r in self.resources.values() {
            let domain = r.domain.as_str();
            by_domain.entry(domain).or_default().push(r);
            if !domain_order.contains(&domain) {
                domain_order.push(domain);
            }
        }

        // Sort resources within each domain alphabetically
        for resources in by_domain.values_mut() {
            resources.sort_by_key(|r| &r.type_name);
        }

        domain_order
            .into_iter()
            .filter_map(|d| by_domain.remove(d).map(|r| (d, r)))
            .collect()
    }
}

// --- Resource definition ---

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceDef {
    #[serde(rename = "type")]
    pub type_name: String,
    pub domain: String,
    pub item_name: String,
    pub collection_name: String,
    pub description: String,
    pub description_short: String,
    #[serde(default)]
    pub aliases: Option<Vec<String>>,
    pub endpoint: Option<String>,
    pub query_hints: Option<String>,
    #[serde(default)]
    pub default_filters: Option<serde_json::Map<String, serde_json::Value>>,
    pub default_sort: Option<String>,
    pub search_filter_param: Option<String>,
    #[serde(default)]
    pub search_quick_result_type: Option<Vec<String>>,
    pub bulk_actions: Option<BulkActions>,
    pub actions: Option<ResourceActions>,
    #[serde(default)]
    pub custom_actions: HashMap<String, CustomAction>,
    #[serde(default)]
    pub fields: HashMap<String, FieldDef>,
    #[serde(default)]
    pub collections: HashMap<String, CollectionDef>,
    pub cache: Option<CacheConfig>,
    #[serde(default)]
    pub display_columns: Option<Vec<DisplayColumn>>,
}

impl ResourceDef {
    /// The API endpoint path for this resource (e.g. "/tasks").
    pub fn api_path(&self) -> String {
        format!("/{}", self.endpoint.as_deref().unwrap_or(&self.type_name))
    }

    /// Look up a field by its `param` key (used for create/update input).
    pub fn field_by_param(&self, param: &str) -> Option<&FieldDef> {
        self.fields
            .values()
            .find(|f| f.param.as_deref() == Some(param))
    }

    /// Look up a field by its `filter` key (used for filter input).
    pub fn field_by_filter(&self, filter_key: &str) -> Option<&FieldDef> {
        self.fields
            .values()
            .find(|f| f.filter.as_deref() == Some(filter_key))
    }

    /// Check if a REST operation is available.
    pub fn supports_action(&self, action: &str) -> bool {
        match &self.actions {
            None => true, // all allowed by default
            Some(a) => match action {
                "index" => a.index.unwrap_or(true),
                "show" => a.show.unwrap_or(true),
                "create" => a.create.unwrap_or(true),
                "update" => a.update.unwrap_or(true),
                "delete" => a.delete.unwrap_or(true),
                _ => false,
            },
        }
    }

    /// Check if bulk operation is supported.
    pub fn supports_bulk(&self, op: &str) -> bool {
        match &self.bulk_actions {
            None => false,
            Some(b) => match op {
                "create" => b.create.unwrap_or(false),
                "update" => b.update.unwrap_or(false),
                "delete" => b.delete.unwrap_or(false),
                _ => false,
            },
        }
    }
}

// --- Field definition ---

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldDef {
    pub key: String,
    #[serde(rename = "type")]
    pub field_type: String,
    pub type_category: TypeCategory,
    pub unit: Option<String>,
    pub format: Option<String>,
    pub param: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub readonly: bool,
    #[serde(default)]
    pub create_only: bool,
    pub filter: Option<String>,
    pub sort: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub id: bool,
    pub attribute: Option<String>,
    pub relationship: Option<String>,
    #[serde(default)]
    pub not_includable: bool,
    #[serde(default)]
    pub array: bool,
    #[serde(default = "default_true")]
    pub serialize: bool,
    pub filter_config: Option<FilterConfig>,
    pub exclusive: Option<String>,
    pub enabled_when: Option<EnableCondition>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TypeCategory {
    Primitive,
    Enum,
    Resource,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterConfig {
    pub required: Option<bool>,
    pub array: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnableCondition {
    pub flags: Option<Vec<String>>,
    pub permissions: Option<Vec<String>>,
    pub features: Option<Vec<String>>,
}

// --- Collection definition ---

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionDef {
    pub key: String,
    pub collection_name: String,
    #[serde(rename = "type")]
    pub collection_type: String,
    pub inverse: Option<String>,
}

// --- Custom action ---

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomAction {
    pub name: String,
    pub description: String,
    pub endpoint: String,
    pub method: String,
    pub enabled_when: Option<EnableCondition>,
}

// --- Display columns ---

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayColumn {
    /// Column header label
    pub label: String,
    /// "attribute" or "relationship"
    pub source: String,
    /// For attributes: the JSON attribute key (e.g. "title", "due_date")
    /// For relationships: the relationship name (e.g. "project", "assignee")
    pub key: String,
    /// For relationships: the target resource type (e.g. "projects", "people")
    pub target: Option<String>,
}

// --- Cache config ---

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheConfig {
    pub enabled: bool,
    pub scope: CacheScope,
    pub display_field: String,
    pub fields: Vec<String>,
    pub sync_filter: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CacheScope {
    Org,
    Project,
}

// --- Bulk / REST action availability ---

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkActions {
    pub create: Option<bool>,
    pub update: Option<bool>,
    pub delete: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceActions {
    pub index: Option<bool>,
    pub show: Option<bool>,
    pub create: Option<bool>,
    pub update: Option<bool>,
    pub delete: Option<bool>,
}

// --- Enum definition ---

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnumDef {
    #[serde(rename = "type")]
    pub enum_type: String,
    pub description: String,
    pub values: HashMap<String, EnumValue>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnumValue {
    pub label: String,
    pub description: Option<String>,
}

// --- Tests ---

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_loads_and_has_resources() {
        let s = schema();
        assert!(
            s.resources.len() > 80,
            "expected 80+ resources, got {}",
            s.resources.len()
        );
        assert!(
            s.enums.len() > 60,
            "expected 60+ enums, got {}",
            s.enums.len()
        );
    }

    #[test]
    fn resolve_resource_by_type() {
        let s = schema();
        let tasks = s.resolve_resource("tasks").expect("tasks should exist");
        assert_eq!(tasks.type_name, "tasks");
        assert_eq!(tasks.domain, "Project Management");
    }

    #[test]
    fn resolve_resource_by_alias() {
        let s = schema();
        // "event" is a known alias for "events" (absence categories)
        let resolved = s
            .resolve_resource("event")
            .expect("alias 'event' should resolve");
        assert_eq!(resolved.type_name, "events");
    }

    #[test]
    fn resource_api_path() {
        let s = schema();
        let tasks = s.resolve_resource("tasks").unwrap();
        assert_eq!(tasks.api_path(), "/tasks");

        // search-quick-results has an endpoint override
        let sqr = s
            .resolve_resource("search-quick-results")
            .expect("search-quick-results should exist");
        assert_eq!(sqr.api_path(), "/search/quick");
    }

    #[test]
    fn field_by_param_lookup() {
        let s = schema();
        let tasks = s.resolve_resource("tasks").unwrap();
        let field = tasks
            .field_by_param("assignee_id")
            .expect("assignee_id param should exist");
        assert_eq!(field.type_category, TypeCategory::Resource);
        assert_eq!(field.field_type, "people");
    }

    #[test]
    fn operators_for_field_types() {
        let s = schema();
        let tasks = s.resolve_resource("tasks").unwrap();

        // String field
        let title = tasks.fields.get("title").unwrap();
        let ops = operators_for_field(title);
        assert!(ops.contains(&"contains"));

        // Resource field
        let assignee = tasks.fields.get("assignee").unwrap();
        let ops = operators_for_field(assignee);
        assert!(ops.contains(&"any_of"));

        // Date field
        let due = tasks.fields.get("dueDate").unwrap();
        let ops = operators_for_field(due);
        assert!(ops.contains(&"gt"));
    }

    #[test]
    fn cache_config_scopes() {
        let s = schema();
        let projects = s.resolve_resource("projects").unwrap();
        let cache = projects
            .cache
            .as_ref()
            .expect("projects should be cacheable");
        assert_eq!(cache.scope, CacheScope::Org);
        assert!(cache.sync_filter.is_some());

        let wf_statuses = s.resolve_resource("workflow_statuses").unwrap();
        let cache = wf_statuses
            .cache
            .as_ref()
            .expect("workflow_statuses should be cacheable");
        assert_eq!(cache.scope, CacheScope::Project);
    }

    #[test]
    fn resources_by_domain_groups() {
        let s = schema();
        let grouped = s.resources_by_domain();
        assert!(grouped.len() >= 9, "expected 9+ domain groups");
        let domains: Vec<&str> = grouped.iter().map(|(d, _)| *d).collect();
        assert!(domains.contains(&"Project Management"));
        assert!(domains.contains(&"Financial"));
    }
}

// --- Operator derivation ---

/// Get valid filter operators for a field based on its type category and primitive type.
pub fn operators_for_field(field: &FieldDef) -> &'static [&'static str] {
    match field.type_category {
        TypeCategory::Primitive => match field.field_type.as_str() {
            "date" | "number" => &["eq", "not_eq", "gt", "lt", "gt_eq", "lt_eq"],
            "boolean" => &["eq"],
            "string" => &["eq", "not_eq", "contains", "not_contain"],
            _ => &["eq", "not_eq"],
        },
        TypeCategory::Resource => &["eq", "not_eq", "any_of", "none_of"],
        TypeCategory::Enum => &["eq", "not_eq"],
    }
}
