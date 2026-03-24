use std::collections::HashMap;

use crate::api::{ProductiveClient, Query, Resource};
use crate::filter::{self, FilterInput};
use crate::generic_cache::GenericCache;
use crate::json_error;
use crate::schema::{DisplayColumn, ResourceDef, Schema, TypeCategory};

pub async fn run(
    client: &ProductiveClient,
    resource: &ResourceDef,
    filter_json: Option<&str>,
    sort: Option<&str>,
    page: Option<usize>,
    include: Option<&str>,
    format: &str,
) {
    if !resource.supports_action("index") {
        json_error::exit_with_error(
            "operation_not_supported",
            &format!("{} does not support querying.", resource.type_name),
        );
    }

    let schema = crate::schema::schema();
    let json_mode = format == "json";

    // Build query
    let mut query = Query::new();

    // Parse user filter (if any)
    let user_input: Option<FilterInput> = filter_json.map(|filter_str| {
        match serde_json::from_str(filter_str) {
            Ok(f) => f,
            Err(e) => {
                json_error::exit_with_error("invalid_json", &format!("Invalid filter JSON: {e}"));
            }
        }
    });

    // Merge with default filters: defaults apply unless user explicitly overrides the same field
    let merged_input = merge_with_defaults(user_input, resource);

    if let Some(input) = merged_input {
        let mut group = filter::normalize_filter(input);

        // Validate
        let errors = filter::validate_filter_group(&group, resource, schema);
        if !errors.is_empty() {
            json_error::exit_with_error_details(
                "invalid_filter",
                &errors[0],
                Some(serde_json::json!({"errors": errors})),
            );
        }

        // Name resolution: resolve names to IDs via cache
        if let Ok(cache) = GenericCache::new(client.org_id())
            && let Err(e) = crate::generic_cache::resolve_filter_names(
                &cache,
                &mut group.conditions,
                resource,
                schema,
            )
        {
            json_error::exit_with_error("name_resolution_error", &e);
        }

        // Serialize to query params
        query = filter::filter_group_to_query(&group, query);
    }

    // Sort
    if let Some(sort_field) = sort {
        query = query.sort(sort_field);
    } else if let Some(default) = &resource.default_sort {
        query = query.sort(default);
    }

    // Include: merge user includes with auto-includes for CSV mode
    if !json_mode {
        let auto = auto_includes_from_columns(resource);
        let user = include.unwrap_or("");
        let merged = merge_includes(&auto, user);
        if !merged.is_empty() {
            query = query.include(&merged);
        }
    } else if let Some(inc) = include {
        query = query.include(inc);
    }

    // Pagination
    let page_num = page.unwrap_or(1);
    let page_size = 20;

    let path = resource.api_path();
    match client.get_page(&path, &query, page_num, page_size).await {
        Ok(resp) => {
            let total_count = resp
                .meta
                .get("total_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let total_pages = resp
                .meta
                .get("total_pages")
                .and_then(|v| v.as_u64())
                .unwrap_or(1);

            if json_mode {
                let output = serde_json::json!({
                    "data": resp.data,
                    "included": resp.included,
                    "meta": {
                        "totalCount": total_count,
                        "totalPages": total_pages,
                        "currentPage": page_num,
                        "hasNextPage": (page_num as u64) < total_pages,
                    }
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                print_csv(
                    &resp.data,
                    &resp.included,
                    resource,
                    schema,
                    client.org_id(),
                    total_count,
                    total_pages,
                    page_num,
                );
            }
        }
        Err(e) => json_error::exit_with_tb_error(&e),
    }
}

// --- Default filter merging ---

fn merge_with_defaults(
    user_input: Option<FilterInput>,
    resource: &ResourceDef,
) -> Option<FilterInput> {
    let defaults = match &resource.default_filters {
        Some(d) if !d.is_empty() => d,
        _ => return user_input,
    };

    match user_input {
        None => {
            let map: serde_json::Map<String, serde_json::Value> = defaults.clone();
            Some(FilterInput::Flat(map))
        }
        Some(FilterInput::Flat(mut user_map)) => {
            for (key, value) in defaults {
                if !user_map.contains_key(key) {
                    user_map.insert(key.clone(), value.clone());
                }
            }
            Some(FilterInput::Flat(user_map))
        }
        Some(group @ FilterInput::Group(_)) => Some(group),
    }
}

// --- Include merging ---

/// Public accessor for search.rs
pub fn auto_includes_from_columns_pub(resource: &ResourceDef) -> Vec<String> {
    auto_includes_from_columns(resource)
}

/// Derive auto-includes from displayColumns config.
fn auto_includes_from_columns(resource: &ResourceDef) -> Vec<String> {
    match &resource.display_columns {
        Some(cols) => cols
            .iter()
            .filter(|c| c.source == "relationship")
            .filter_map(|c| {
                // Check that the relationship is includable
                let field = resource.fields.values().find(|f| {
                    f.relationship.as_deref() == Some(&c.key) && !f.not_includable
                });
                field.map(|_| c.key.clone())
            })
            .collect(),
        None => fallback_auto_includes(resource),
    }
}

/// Fallback auto-includes for resources without displayColumns.
fn fallback_auto_includes(resource: &ResourceDef) -> Vec<String> {
    let priority = ["project", "workflow_status", "assignee", "company", "deal", "person", "service"];
    priority
        .iter()
        .filter(|&&rel| {
            resource.fields.values().any(|f| {
                f.type_category == TypeCategory::Resource
                    && f.relationship.as_deref() == Some(rel)
                    && !f.not_includable
                    && !f.array
            })
        })
        .map(|s| s.to_string())
        .collect()
}

/// Merge auto-includes with user-specified includes (additive).
fn merge_includes(auto: &[String], user: &str) -> String {
    let mut all: Vec<String> = auto.to_vec();
    for inc in user.split(',').map(|s| s.trim()) {
        if !inc.is_empty() && !all.iter().any(|a| a == inc) {
            all.push(inc.to_string());
        }
    }
    all.join(",")
}

// --- Name resolution ---

/// Build a lookup from (type, id) → display name using included resources and cache.
pub fn build_name_lookup(
    included: &[Resource],
    schema: &Schema,
    org_id: &str,
) -> HashMap<(String, String), String> {
    let mut lookup: HashMap<(String, String), String> = HashMap::new();

    // From included (sideloaded) resources
    for inc in included {
        let name = extract_display_name(inc);
        if !name.is_empty() {
            lookup.insert((inc.resource_type.clone(), inc.id.clone()), name);
        }
    }

    // Supplement with org cache
    if let Ok(cache) = GenericCache::new(org_id) {
        for type_name in &["projects", "people", "companies", "service_types"] {
            if let Ok(records) = cache.read_org_cache(type_name) {
                let display_field = schema
                    .resources
                    .get(*type_name)
                    .and_then(|t| t.cache.as_ref())
                    .map(|c| c.display_field.as_str())
                    .unwrap_or("name");

                for r in records {
                    let key = (type_name.to_string(), r.id.clone());
                    if lookup.contains_key(&key) {
                        continue;
                    }
                    let name = if display_field == "name" {
                        if let (Some(first), Some(last)) =
                            (r.fields.get("first_name"), r.fields.get("last_name"))
                        {
                            format!("{} {}", first, last).trim().to_string()
                        } else {
                            r.fields.get("name").cloned().unwrap_or_default()
                        }
                    } else {
                        r.fields.get(display_field).cloned().unwrap_or_default()
                    };
                    if !name.is_empty() {
                        lookup.insert(key, name);
                    }
                }
            }
        }
    }

    lookup
}

/// Extract a display name from a Resource (from included data).
pub fn extract_display_name(resource: &Resource) -> String {
    for key in &["name", "title"] {
        let val = resource.attr_str(key);
        if !val.is_empty() {
            return val.to_string();
        }
    }
    let first = resource.attr_str("first_name");
    let last = resource.attr_str("last_name");
    if !first.is_empty() || !last.is_empty() {
        return format!("{} {}", first, last).trim().to_string();
    }
    String::new()
}

// --- Column building ---

type ColumnExtractor = Box<dyn Fn(&Resource, &HashMap<(String, String), String>) -> String>;

/// Build columns from displayColumns config, or fall back to heuristic.
fn build_columns(resource: &ResourceDef) -> Vec<(String, ColumnExtractor)> {
    match &resource.display_columns {
        Some(cols) => cols.iter().map(|col| build_column_from_config(col)).collect(),
        None => build_columns_fallback(resource),
    }
}

fn build_column_from_config(col: &DisplayColumn) -> (String, ColumnExtractor) {
    let label = col.label.clone();
    match col.source.as_str() {
        "id" => (label, Box::new(|r: &Resource, _| r.id.clone())),
        "attribute" => {
            let key = col.key.clone();
            (
                label,
                Box::new(move |r: &Resource, _| {
                    r.attributes
                        .get(&key)
                        .map(|v| match v {
                            serde_json::Value::String(s) => s.clone(),
                            serde_json::Value::Null => String::new(),
                            serde_json::Value::Bool(b) => b.to_string(),
                            other => other.to_string(),
                        })
                        .unwrap_or_default()
                }),
            )
        }
        "relationship" => {
            let rel_name = col.key.clone();
            let target_type = col.target.clone().unwrap_or_default();
            (
                label,
                Box::new(move |r: &Resource, lookup: &HashMap<(String, String), String>| {
                    if let Some(id) = r.relationship_id(&rel_name) {
                        lookup
                            .get(&(target_type.clone(), id.to_string()))
                            .cloned()
                            .unwrap_or_else(|| format!("#{}", id))
                    } else {
                        String::new()
                    }
                }),
            )
        }
        _ => (label, Box::new(|_, _| String::new())),
    }
}

/// Fallback column builder for resources without displayColumns.
fn build_columns_fallback(resource: &ResourceDef) -> Vec<(String, ColumnExtractor)> {
    let mut cols: Vec<(String, ColumnExtractor)> = Vec::new();

    // ID
    cols.push(("ID".to_string(), Box::new(|r: &Resource, _| r.id.clone())));

    // All non-readonly, non-array, serializable attribute fields (up to 8 total)
    let mut fields: Vec<&crate::schema::FieldDef> = resource
        .fields
        .values()
        .filter(|f| {
            f.type_category == TypeCategory::Primitive
                && f.serialize
                && !f.array
                && f.attribute.is_some()
        })
        .collect();
    // Put title/name first
    fields.sort_by_key(|f| {
        if f.key == "title" || f.key == "name" {
            0
        } else if f.filter.is_some() {
            1
        } else {
            2
        }
    });

    for field in fields.iter().take(7) {
        let attr_key = field.attribute.as_ref().unwrap().clone();
        let label = field.key.clone();
        cols.push((
            label,
            Box::new(move |r: &Resource, _| {
                r.attributes
                    .get(&attr_key)
                    .map(|v| match v {
                        serde_json::Value::String(s) => s.clone(),
                        serde_json::Value::Null => String::new(),
                        serde_json::Value::Bool(b) => b.to_string(),
                        other => other.to_string(),
                    })
                    .unwrap_or_default()
            }),
        ));
    }

    cols
}

// --- CSV output ---

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub fn print_csv(
    data: &[Resource],
    included: &[Resource],
    resource: &ResourceDef,
    schema: &Schema,
    org_id: &str,
    total_count: u64,
    total_pages: u64,
    current_page: usize,
) {
    if data.is_empty() {
        println!("No {} found.", resource.type_name);
        println!("# page {}/{}", current_page, total_pages);
        return;
    }

    let lookup = build_name_lookup(included, schema, org_id);
    let columns = build_columns(resource);

    // Header
    let header: String = columns
        .iter()
        .map(|(h, _)| csv_escape(h))
        .collect::<Vec<_>>()
        .join(",");
    println!("{}", header);

    // Rows
    for record in data {
        let row: String = columns
            .iter()
            .map(|(_, extractor)| csv_escape(&extractor(record, &lookup)))
            .collect::<Vec<_>>()
            .join(",");
        println!("{}", row);
    }

    // Footer
    println!(
        "# {} {} (page {}/{}, {} total)",
        data.len(),
        resource.type_name,
        current_page,
        total_pages,
        total_count
    );
}
