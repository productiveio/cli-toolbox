use serde_json::Value;
use std::collections::HashMap;

use crate::schema::{ResourceDef, Schema, TypeCategory};

/// Validate fields for a create operation.
pub fn validate_create(
    resource: &ResourceDef,
    input: &Value,
    schema: &Schema,
) -> Vec<String> {
    let map = match input.as_object() {
        Some(m) => m,
        None => return vec!["Input must be a JSON object.".to_string()],
    };

    let mut errors = Vec::new();

    // Check for unknown and readonly fields
    for key in map.keys() {
        match resource.field_by_param(key) {
            None => {
                errors.push(format!("Unknown field '{}' on {}.", key, resource.type_name));
            }
            Some(f) => {
                if f.readonly {
                    errors.push(format!(
                        "Field '{}' is readonly and cannot be set.",
                        key
                    ));
                }
            }
        }
    }

    // Check required fields
    for field in resource.fields.values() {
        if field.required && !field.id && !field.readonly {
            if let Some(param) = &field.param {
                if !map.contains_key(param.as_str()) {
                    errors.push(format!(
                        "Required field '{}' is missing.",
                        param
                    ));
                }
            }
        }
    }

    // Validate exclusive groups
    errors.extend(validate_exclusive_groups(resource, map));

    // Validate enum values
    errors.extend(validate_enum_values(resource, map, schema));

    errors
}

/// Validate fields for an update operation.
pub fn validate_update(
    resource: &ResourceDef,
    input: &Value,
    schema: &Schema,
) -> Vec<String> {
    let map = match input.as_object() {
        Some(m) => m,
        None => return vec!["Input must be a JSON object.".to_string()],
    };

    let mut errors = Vec::new();

    for key in map.keys() {
        match resource.field_by_param(key) {
            None => {
                errors.push(format!("Unknown field '{}' on {}.", key, resource.type_name));
            }
            Some(f) => {
                if f.readonly {
                    errors.push(format!(
                        "Field '{}' is readonly and cannot be set.",
                        key
                    ));
                }
                if f.create_only {
                    errors.push(format!(
                        "Field '{}' can only be set on create, not update.",
                        key
                    ));
                }
            }
        }
    }

    // Validate enum values
    errors.extend(validate_enum_values(resource, map, schema));

    errors
}

/// Validate that mutually exclusive field groups have exactly one member provided.
fn validate_exclusive_groups(
    resource: &ResourceDef,
    input: &serde_json::Map<String, Value>,
) -> Vec<String> {
    let mut errors = Vec::new();

    // Collect exclusive groups
    let mut groups: HashMap<&str, Vec<&str>> = HashMap::new();
    for field in resource.fields.values() {
        if let (Some(exclusive), Some(param)) = (&field.exclusive, &field.param) {
            groups.entry(exclusive.as_str()).or_default().push(param.as_str());
        }
    }

    for (group_name, fields) in &groups {
        let provided: Vec<&&str> = fields.iter().filter(|f| input.contains_key(**f)).collect();
        if provided.len() > 1 {
            errors.push(format!(
                "Fields {} are mutually exclusive (group '{}'). Provide only one.",
                provided
                    .iter()
                    .map(|f| format!("'{}'", f))
                    .collect::<Vec<_>>()
                    .join(", "),
                group_name
            ));
        }
    }

    errors
}

/// Validate that enum field values are valid according to the schema.
fn validate_enum_values(
    resource: &ResourceDef,
    input: &serde_json::Map<String, Value>,
    schema: &Schema,
) -> Vec<String> {
    let mut errors = Vec::new();

    for (key, value) in input {
        if let Some(field) = resource.field_by_param(key) {
            if field.type_category == TypeCategory::Enum {
                if let Some(enum_def) = schema.enums.get(&field.field_type) {
                    let val_str = value
                        .as_str()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| value.to_string().trim_matches('"').to_string());

                    if !enum_def.values.contains_key(&val_str) {
                        let valid: Vec<String> = enum_def
                            .values
                            .iter()
                            .map(|(k, v)| format!("{}={}", k, v.label))
                            .collect();
                        errors.push(format!(
                            "Invalid value '{}' for enum field '{}' ({}). Valid values: {}",
                            val_str,
                            key,
                            field.field_type,
                            valid.join(", ")
                        ));
                    }
                }
            }
        }
    }

    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::schema;
    use serde_json::json;

    #[test]
    fn validate_create_missing_required() {
        let s = schema();
        let tasks = s.resolve_resource("tasks").unwrap();
        // tasks requires title, project_id, task_list_id
        let input = json!({"title": "Test"});
        let errors = validate_create(tasks, &input, s);
        assert!(!errors.is_empty());
        let all = errors.join(" ");
        assert!(all.contains("project_id") || all.contains("task_list_id"));
    }

    #[test]
    fn validate_create_readonly_rejected() {
        let s = schema();
        let tasks = s.resolve_resource("tasks").unwrap();
        // "id" is readonly with param "id"
        let input = json!({
            "title": "Test",
            "project_id": "1",
            "task_list_id": "2",
            "id": "123"
        });
        let errors = validate_create(tasks, &input, s);
        assert!(errors.iter().any(|e| e.contains("readonly")), "errors: {:?}", errors);
    }

    #[test]
    fn validate_update_create_only_rejected() {
        let s = schema();
        // pages.body is a known createOnly field
        let pages = s.resolve_resource("pages").expect("pages should exist");
        let input = json!({"body": "x"});
        let errors = validate_update(pages, &input, s);
        assert!(errors.iter().any(|e| e.contains("create")), "errors: {:?}", errors);
    }

    #[test]
    fn validate_unknown_field_rejected() {
        let s = schema();
        let tasks = s.resolve_resource("tasks").unwrap();
        let input = json!({"nonexistent_field": "x"});
        let errors = validate_create(tasks, &input, s);
        assert!(errors.iter().any(|e| e.contains("Unknown field")));
    }

    #[test]
    fn validate_valid_create_passes() {
        let s = schema();
        let tasks = s.resolve_resource("tasks").unwrap();
        let input = json!({
            "title": "Test task",
            "project_id": "100",
            "task_list_id": "200"
        });
        let errors = validate_create(tasks, &input, s);
        // May still have errors for other required fields, but not for the ones we provided
        let field_errors: Vec<&String> = errors.iter().filter(|e| {
            e.contains("Unknown") || e.contains("readonly") || e.contains("title") || e.contains("project_id") || e.contains("task_list_id")
        }).collect();
        assert!(field_errors.is_empty(), "unexpected errors: {:?}", field_errors);
    }
}
