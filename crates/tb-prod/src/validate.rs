use serde_json::Value;
use std::collections::HashMap;

use crate::schema::{FieldDef, ResourceDef, Schema, TypeCategory};

/// Validate fields for a create operation.
pub fn validate_create(resource: &ResourceDef, input: &Value, schema: &Schema) -> Vec<String> {
    let map = match input.as_object() {
        Some(m) => m,
        None => return vec!["Input must be a JSON object.".to_string()],
    };

    let mut errors = Vec::new();

    // Check for unknown and readonly fields
    for (key, value) in map {
        match resource.field_by_param(key) {
            None => {
                errors.push(format!(
                    "Unknown field '{}' on {}.",
                    key, resource.type_name
                ));
            }
            Some(f) => {
                if f.readonly {
                    errors.push(format!("Field '{}' is readonly and cannot be set.", key));
                }
                if let Some(e) = relationship_shape_error(f, value) {
                    errors.push(e);
                }
            }
        }
    }

    // Check required fields
    for field in resource.fields.values() {
        if field.required
            && !field.id
            && !field.readonly
            && let Some(param) = &field.param
            && !map.contains_key(param.as_str())
        {
            errors.push(format!("Required field '{}' is missing.", param));
        }
    }

    // Validate exclusive groups
    errors.extend(validate_exclusive_groups(resource, map));

    // Validate enum values
    errors.extend(validate_enum_values(resource, map, schema));

    errors
}

/// Validate fields for an update operation.
pub fn validate_update(resource: &ResourceDef, input: &Value, schema: &Schema) -> Vec<String> {
    let map = match input.as_object() {
        Some(m) => m,
        None => return vec!["Input must be a JSON object.".to_string()],
    };

    let mut errors = Vec::new();

    for (key, value) in map {
        match resource.field_by_param(key) {
            None => {
                errors.push(format!(
                    "Unknown field '{}' on {}.",
                    key, resource.type_name
                ));
            }
            Some(f) => {
                if f.readonly {
                    errors.push(format!("Field '{}' is readonly and cannot be set.", key));
                }
                if f.create_only {
                    errors.push(format!(
                        "Field '{}' can only be set on create, not update.",
                        key
                    ));
                }
                if let Some(e) = relationship_shape_error(f, value) {
                    errors.push(e);
                }
            }
        }
    }

    // Validate enum values
    errors.extend(validate_enum_values(resource, map, schema));

    errors
}

/// Relationship fields take a flat ID string (e.g. `"123"`), an array of ID strings for
/// array relationships, or `null` — never the JSON:API `{"id","type"}` object shape that
/// appears in *responses*. Passing the object form makes the body builder stringify the
/// whole object into the `id` slot, producing a bogus reference the API rejects with a
/// misleading 403 access_denied (read as a permissions problem). Catch it locally with a
/// clear message instead.
fn relationship_shape_error(field: &FieldDef, value: &Value) -> Option<String> {
    if field.type_category != TypeCategory::Resource {
        return None;
    }
    let has_object = match value {
        Value::Object(_) => true,
        Value::Array(items) => items.iter().any(Value::is_object),
        _ => false,
    };
    if !has_object {
        return None;
    }
    let param = field.param.as_deref().unwrap_or(&field.key);
    let expected = if field.array {
        "an array of ID strings (e.g. [\"123\", \"456\"])"
    } else {
        "a flat ID string (e.g. \"123\")"
    };
    Some(format!(
        "Field '{}' is a relationship — provide {}, not a {{\"id\",\"type\"}} object.",
        param, expected
    ))
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
            groups
                .entry(exclusive.as_str())
                .or_default()
                .push(param.as_str());
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
        if let Some(field) = resource.field_by_param(key)
            && field.type_category == TypeCategory::Enum
            && let Some(enum_def) = schema.enums.get(&field.field_type)
        {
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
        // tasks requires title, project, task_list
        let input = json!({"title": "Test"});
        let errors = validate_create(tasks, &input, s);
        assert!(!errors.is_empty());
        let all = errors.join(" ");
        assert!(all.contains("project") || all.contains("task_list"));
    }

    #[test]
    fn validate_create_readonly_rejected() {
        let s = schema();
        let tasks = s.resolve_resource("tasks").unwrap();
        // "id" is readonly with param "id"
        let input = json!({
            "title": "Test",
            "project": "1",
            "task_list": "2",
            "id": "123"
        });
        let errors = validate_create(tasks, &input, s);
        assert!(
            errors.iter().any(|e| e.contains("readonly")),
            "errors: {:?}",
            errors
        );
    }

    #[test]
    fn validate_update_create_only_rejected() {
        let s = schema();
        // pages.body is a known createOnly field
        let pages = s.resolve_resource("pages").expect("pages should exist");
        let input = json!({"body": "x"});
        let errors = validate_update(pages, &input, s);
        assert!(
            errors.iter().any(|e| e.contains("create")),
            "errors: {:?}",
            errors
        );
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
    fn validate_create_rejects_relationship_object_form() {
        let s = schema();
        let tasks = s.resolve_resource("tasks").unwrap();
        // The JSON:API {id,type} object shape is a response shape, not valid input —
        // passing it would otherwise produce a bogus id and a misleading API 403.
        let bad = json!({
            "title": "Test",
            "project": {"id": "200", "type": "projects"},
            "task_list": "300"
        });
        let errors = validate_create(tasks, &bad, s);
        assert!(
            errors
                .iter()
                .any(|e| e.contains("relationship") && e.contains("'project'")),
            "expected relationship-shape error, got: {:?}",
            errors
        );
        // The flat ID string form passes the relationship-shape check.
        let ok = json!({"title": "Test", "project": "200", "task_list": "300"});
        assert!(
            !validate_create(tasks, &ok, s)
                .iter()
                .any(|e| e.contains("relationship")),
            "flat ID string should not trigger the relationship-shape error"
        );
    }

    #[test]
    fn validate_valid_create_passes() {
        let s = schema();
        let tasks = s.resolve_resource("tasks").unwrap();
        let input = json!({
            "title": "Test task",
            "project": "100",
            "task_list": "200"
        });
        let errors = validate_create(tasks, &input, s);
        // May still have errors for other required fields, but not for the ones we provided
        let field_errors: Vec<&String> = errors
            .iter()
            .filter(|e| {
                e.contains("Unknown")
                    || e.contains("readonly")
                    || e.contains("title")
                    || e.contains("project")
                    || e.contains("task_list")
            })
            .collect();
        assert!(
            field_errors.is_empty(),
            "unexpected errors: {:?}",
            field_errors
        );
    }
}
