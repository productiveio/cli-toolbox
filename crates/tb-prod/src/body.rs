use serde_json::{Map, Value, json};

use crate::schema::{ResourceDef, TypeCategory};

/// Build a JSONAPI body from flat field input and a resource schema.
///
/// Input: `{"title": "My task", "assignee_id": "100", "project_id": "200"}`
/// Keys are matched against `field.param` in the schema.
///
/// Output: `{"data": {"type": "tasks", "attributes": {...}, "relationships": {...}}}`
pub fn build_jsonapi_body(
    resource: &ResourceDef,
    input: &Value,
    id: Option<&str>,
) -> Result<Value, Vec<String>> {
    let map = match input.as_object() {
        Some(m) => m,
        None => return Err(vec!["Input must be a JSON object.".to_string()]),
    };

    let mut attributes = Map::new();
    let mut relationships = Map::new();
    let mut errors = Vec::new();

    for (key, value) in map {
        // Unknown fields are already rejected by validate.rs — skip gracefully
        let Some(f) = resource.field_by_param(key) else {
            continue;
        };

        if f.type_category == TypeCategory::Resource {
            let rel_key = f.relationship.as_deref().unwrap_or(key);
            if f.array {
                let ids = match value.as_array() {
                    Some(arr) => arr
                        .iter()
                        .map(|v| {
                            json!({"type": f.field_type, "id": v.as_str().unwrap_or(v.to_string().trim_matches('"'))})
                        })
                        .collect::<Vec<_>>(),
                    None => {
                        errors.push(format!(
                            "Field '{}' is an array relationship — value must be a JSON array of IDs.",
                            key
                        ));
                        continue;
                    }
                };
                relationships.insert(rel_key.to_string(), json!({"data": ids}));
            } else if value.is_null() {
                relationships.insert(rel_key.to_string(), json!({"data": null}));
            } else {
                let id_val = value
                    .as_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| value.to_string().trim_matches('"').to_string());
                relationships.insert(
                    rel_key.to_string(),
                    json!({"data": {"type": f.field_type, "id": id_val}}),
                );
            }
        } else {
            let attr_key = f.attribute.as_deref().unwrap_or(key);
            attributes.insert(attr_key.to_string(), value.clone());
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    let mut data = json!({
        "type": resource.type_name,
    });

    if let Some(id) = id {
        data["id"] = json!(id);
    }

    if !attributes.is_empty() {
        data["attributes"] = Value::Object(attributes);
    }

    if !relationships.is_empty() {
        data["relationships"] = Value::Object(relationships);
    }

    Ok(json!({"data": data}))
}

/// Build a JSONAPI bulk body from an array of flat inputs.
pub fn build_jsonapi_bulk_body(
    resource: &ResourceDef,
    items: &[Value],
) -> Result<Value, Vec<String>> {
    let mut data = Vec::new();
    let mut all_errors = Vec::new();

    for (i, item) in items.iter().enumerate() {
        match build_jsonapi_body(resource, item, None) {
            Ok(body) => {
                if let Some(d) = body.get("data") {
                    data.push(d.clone());
                }
            }
            Err(errors) => {
                for e in errors {
                    all_errors.push(format!("[item {}] {}", i, e));
                }
            }
        }
    }

    if !all_errors.is_empty() {
        return Err(all_errors);
    }

    Ok(json!({"data": data}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::schema;

    #[test]
    fn build_body_with_attributes_and_relationships() {
        let s = schema();
        let tasks = s.resolve_resource("tasks").unwrap();
        let input = json!({
            "title": "Test task",
            "project_id": "100",
            "task_list_id": "200",
            "assignee_id": "300"
        });
        let body = build_jsonapi_body(tasks, &input, None).unwrap();
        let data = body.get("data").unwrap();

        assert_eq!(data["type"], "tasks");
        assert_eq!(data["attributes"]["title"], "Test task");
        assert!(data["relationships"]["project"]["data"]["id"].is_string());
        assert_eq!(data["relationships"]["project"]["data"]["type"], "projects");
        assert_eq!(data["relationships"]["assignee"]["data"]["id"], "300");
    }

    #[test]
    fn build_body_with_id_for_update() {
        let s = schema();
        let tasks = s.resolve_resource("tasks").unwrap();
        let input = json!({"title": "Updated"});
        let body = build_jsonapi_body(tasks, &input, Some("42")).unwrap();
        let data = body.get("data").unwrap();
        assert_eq!(data["id"], "42");
    }

    #[test]
    fn build_body_skips_unknown_fields() {
        let s = schema();
        let tasks = s.resolve_resource("tasks").unwrap();
        // Unknown fields are skipped (validation is done by validate.rs)
        let input = json!({"nonexistent_field": "x"});
        let result = build_jsonapi_body(tasks, &input, None);
        assert!(result.is_ok());
    }

    #[test]
    fn build_body_null_relationship() {
        let s = schema();
        let tasks = s.resolve_resource("tasks").unwrap();
        let input = json!({"assignee_id": null});
        let body = build_jsonapi_body(tasks, &input, None).unwrap();
        let data = body.get("data").unwrap();
        assert!(data["relationships"]["assignee"]["data"].is_null());
    }

    #[test]
    fn build_bulk_body() {
        let s = schema();
        let tasks = s.resolve_resource("tasks").unwrap();
        let items = vec![
            json!({"title": "Task 1", "project_id": "1", "task_list_id": "10"}),
            json!({"title": "Task 2", "project_id": "1", "task_list_id": "10"}),
        ];
        let body = build_jsonapi_bulk_body(tasks, &items).unwrap();
        let data = body.get("data").unwrap().as_array().unwrap();
        assert_eq!(data.len(), 2);
    }
}
