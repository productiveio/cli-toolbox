use serde::Deserialize;
use serde_json::Value;

use crate::api::Query;
use crate::schema::{operators_for_field, FieldDef, ResourceDef, Schema, TypeCategory};

// --- FilterGroup types ---

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum FilterInput {
    Group(FilterGroup),
    Flat(serde_json::Map<String, Value>),
}

#[derive(Debug, Clone, Deserialize)]
pub struct FilterGroup {
    pub op: String,
    pub conditions: Vec<FilterEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum FilterEntry {
    Condition(FilterCondition),
    Group(FilterGroup),
}

#[derive(Debug, Clone, Deserialize)]
pub struct FilterCondition {
    pub field: String,
    #[serde(alias = "op")]
    pub operator: String,
    pub value: FilterValue,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum FilterValue {
    Single(String),
    Array(Vec<String>),
}

impl FilterValue {
    pub fn as_strings(&self) -> Vec<&str> {
        match self {
            FilterValue::Single(s) => vec![s.as_str()],
            FilterValue::Array(a) => a.iter().map(|s| s.as_str()).collect(),
        }
    }
}

// --- Normalization ---

/// Normalize a FilterInput into a FilterGroup.
/// Flat objects like {"project_id": "123"} become AND groups of eq conditions.
pub fn normalize_filter(input: FilterInput) -> FilterGroup {
    match input {
        FilterInput::Group(g) => g,
        FilterInput::Flat(map) => {
            let conditions = map
                .into_iter()
                .map(|(field, value)| {
                    let val = match value {
                        Value::String(s) => FilterValue::Single(s),
                        Value::Number(n) => FilterValue::Single(n.to_string()),
                        Value::Array(arr) => FilterValue::Array(
                            arr.into_iter()
                                .map(|v| match v {
                                    Value::String(s) => s,
                                    other => other.to_string().trim_matches('"').to_string(),
                                })
                                .collect(),
                        ),
                        other => FilterValue::Single(other.to_string().trim_matches('"').to_string()),
                    };
                    FilterEntry::Condition(FilterCondition {
                        field,
                        operator: "eq".to_string(),
                        value: val,
                    })
                })
                .collect();
            FilterGroup {
                op: "and".to_string(),
                conditions,
            }
        }
    }
}

// --- Validation ---

/// Validate a FilterGroup against a resource schema.
/// Returns a list of validation errors (empty = valid).
pub fn validate_filter_group(
    group: &FilterGroup,
    resource: &ResourceDef,
    schema: &Schema,
) -> Vec<String> {
    let mut errors = Vec::new();

    if group.op != "and" && group.op != "or" {
        errors.push(format!("Invalid filter operator '{}'. Must be 'and' or 'or'.", group.op));
    }

    for entry in &group.conditions {
        match entry {
            FilterEntry::Condition(cond) => {
                errors.extend(validate_condition(cond, resource, schema));
            }
            FilterEntry::Group(sub) => {
                errors.extend(validate_filter_group(sub, resource, schema));
            }
        }
    }

    errors
}

fn validate_condition(
    cond: &FilterCondition,
    resource: &ResourceDef,
    schema: &Schema,
) -> Vec<String> {
    let mut errors = Vec::new();

    // Handle dot-notation relationship filters (e.g. "project.status")
    if let Some(dot_pos) = cond.field.find('.') {
        let rel_name = &cond.field[..dot_pos];
        let rel_field_name = &cond.field[dot_pos + 1..];

        // Find the relationship field
        let rel_field = resource.fields.values().find(|f| {
            f.relationship.as_deref() == Some(rel_name) && f.type_category == TypeCategory::Resource
        });

        match rel_field {
            None => {
                errors.push(format!(
                    "Unknown relationship '{}' on {}.",
                    rel_name, resource.type_name
                ));
            }
            Some(rf) => {
                // Validate the sub-field against the related resource
                if let Some(related) = schema.resources.get(&rf.field_type) {
                    let sub_field = related.field_by_filter(rel_field_name);
                    match sub_field {
                        None => {
                            errors.push(format!(
                                "Unknown filter field '{}' on related resource {}.",
                                rel_field_name, rf.field_type
                            ));
                        }
                        Some(sf) => {
                            let valid_ops = operators_for_field(sf);
                            if !valid_ops.contains(&cond.operator.as_str()) {
                                errors.push(format!(
                                    "Invalid operator '{}' for {}.{}. Valid: {:?}",
                                    cond.operator, rel_name, rel_field_name, valid_ops
                                ));
                            }
                        }
                    }
                }
            }
        }
        return errors;
    }

    // Direct field filter
    let field = resource.field_by_filter(&cond.field);
    match field {
        None => {
            // Collect available filter fields for suggestion
            let available: Vec<&str> = resource
                .fields
                .values()
                .filter_map(|f| f.filter.as_deref())
                .collect();
            errors.push(format!(
                "Unknown filter field '{}' on {}. Available: {}",
                cond.field,
                resource.type_name,
                available.join(", ")
            ));
        }
        Some(f) => {
            let valid_ops = operators_for_field(f);
            if !valid_ops.contains(&cond.operator.as_str()) {
                errors.push(format!(
                    "Invalid operator '{}' for field '{}' ({}). Valid: {:?}",
                    cond.operator, cond.field, f.field_type, valid_ops
                ));
            }
        }
    }

    errors
}

// --- Serialization to Query builder ---

/// Convert a validated FilterGroup into Query builder calls.
pub fn filter_group_to_query(group: &FilterGroup, query: Query) -> Query {
    let mut q = query.filter_op(&group.op);
    let mut index = 0;
    q = serialize_entries(&group.conditions, q, &mut index);
    q
}

fn serialize_entries(
    entries: &[FilterEntry],
    mut query: Query,
    index: &mut usize,
) -> Query {
    for entry in entries {
        match entry {
            FilterEntry::Condition(cond) => {
                let filter_key = &cond.field;
                match &cond.value {
                    FilterValue::Single(v) => {
                        query = query.filter_indexed(*index, filter_key, &cond.operator, v);
                    }
                    FilterValue::Array(values) => {
                        for v in values {
                            query = query.filter_indexed(*index, filter_key, &cond.operator, v);
                        }
                    }
                }
                *index += 1;
            }
            FilterEntry::Group(sub) => {
                // Nested groups use indexed sub-filters
                // For now, flatten nested groups into the same index space
                // with their own $op at a sub-level
                // This is a simplification — full nested support would need
                // bracket nesting like filter[0][$op]=or&filter[0][0][field][op]=val
                // For now we serialize flat (covers most real-world cases)
                query = serialize_entries(&sub.conditions, query, index);
            }
        }
    }
    query
}

/// Get the FieldDef for a filter condition's field, checking multiple resolution paths.
pub fn resolve_filter_field<'a>(field_name: &str, resource: &'a ResourceDef) -> Option<&'a FieldDef> {
    // Direct filter key match
    if let Some(f) = resource.field_by_filter(field_name) {
        return Some(f);
    }
    // Try by param
    if let Some(f) = resource.field_by_param(field_name) {
        if f.filter.is_some() {
            return Some(f);
        }
    }
    // Try by key
    if let Some(f) = resource.fields.get(field_name) {
        if f.filter.is_some() {
            return Some(f);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::schema;

    #[test]
    fn normalize_flat_filter() {
        let input: FilterInput = serde_json::from_str(r#"{"project_id": "123", "status": "1"}"#).unwrap();
        let group = normalize_filter(input);
        assert_eq!(group.op, "and");
        assert_eq!(group.conditions.len(), 2);
    }

    #[test]
    fn normalize_group_filter() {
        let input: FilterInput = serde_json::from_str(
            r#"{"op": "and", "conditions": [{"field": "project_id", "op": "eq", "value": "123"}]}"#,
        ).unwrap();
        let group = normalize_filter(input);
        assert_eq!(group.op, "and");
        assert_eq!(group.conditions.len(), 1);
    }

    #[test]
    fn validate_valid_filter() {
        let s = schema();
        let tasks = s.resolve_resource("tasks").unwrap();
        let group = FilterGroup {
            op: "and".to_string(),
            conditions: vec![FilterEntry::Condition(FilterCondition {
                field: "title".to_string(),
                operator: "contains".to_string(),
                value: FilterValue::Single("test".to_string()),
            })],
        };
        let errors = validate_filter_group(&group, tasks, s);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
    }

    #[test]
    fn validate_invalid_field() {
        let s = schema();
        let tasks = s.resolve_resource("tasks").unwrap();
        let group = FilterGroup {
            op: "and".to_string(),
            conditions: vec![FilterEntry::Condition(FilterCondition {
                field: "nonexistent".to_string(),
                operator: "eq".to_string(),
                value: FilterValue::Single("x".to_string()),
            })],
        };
        let errors = validate_filter_group(&group, tasks, s);
        assert!(!errors.is_empty());
        assert!(errors[0].contains("Unknown filter field"));
    }

    #[test]
    fn validate_invalid_operator() {
        let s = schema();
        let tasks = s.resolve_resource("tasks").unwrap();
        let group = FilterGroup {
            op: "and".to_string(),
            conditions: vec![FilterEntry::Condition(FilterCondition {
                field: "title".to_string(),
                operator: "gt".to_string(), // gt not valid for strings
                value: FilterValue::Single("x".to_string()),
            })],
        };
        let errors = validate_filter_group(&group, tasks, s);
        assert!(!errors.is_empty());
        assert!(errors[0].contains("Invalid operator"));
    }

    #[test]
    fn filter_to_query_serialization() {
        let group = FilterGroup {
            op: "and".to_string(),
            conditions: vec![
                FilterEntry::Condition(FilterCondition {
                    field: "project_id".to_string(),
                    operator: "eq".to_string(),
                    value: FilterValue::Single("123".to_string()),
                }),
                FilterEntry::Condition(FilterCondition {
                    field: "due_date".to_string(),
                    operator: "lt".to_string(),
                    value: FilterValue::Single("2026-04-01".to_string()),
                }),
            ],
        };
        let query = filter_group_to_query(&group, Query::new());
        let qs = query.to_query_string();
        assert!(qs.contains("filter%5B%24op%5D=and") || qs.contains("filter[$op]=and"));
        assert!(qs.contains("project_id"));
        assert!(qs.contains("due_date"));
    }

    #[test]
    fn validate_dot_notation_filter() {
        let s = schema();
        let tasks = s.resolve_resource("tasks").unwrap();
        let group = FilterGroup {
            op: "and".to_string(),
            conditions: vec![FilterEntry::Condition(FilterCondition {
                field: "project.status".to_string(),
                operator: "eq".to_string(),
                value: FilterValue::Single("1".to_string()),
            })],
        };
        let errors = validate_filter_group(&group, tasks, s);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
    }
}
