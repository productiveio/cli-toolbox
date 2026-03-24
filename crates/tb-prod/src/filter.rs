use serde::Deserialize;
use serde_json::Value;

use crate::api::Query;
use crate::schema::{FieldDef, ResourceDef, Schema, TypeCategory, operators_for_field};

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
                        other => {
                            FilterValue::Single(other.to_string().trim_matches('"').to_string())
                        }
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
        errors.push(format!(
            "Invalid filter operator '{}'. Must be 'and' or 'or'.",
            group.op
        ));
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
/// Produces the indexed bracket format expected by the Productive API:
///   filter[$op]=and
///   filter[0][field][op][]=value
///   filter[1][$op]=or          (nested group)
///   filter[1][0][field][op][]=value
pub fn filter_group_to_query(group: &FilterGroup, query: Query) -> Query {
    if group.conditions.is_empty() {
        return query;
    }
    serialize_group(group, "filter", query)
}

fn serialize_group(group: &FilterGroup, prefix: &str, mut query: Query) -> Query {
    query = query.filter_raw(format!("{}[$op]", prefix), group.op.clone());

    for (i, entry) in group.conditions.iter().enumerate() {
        let item_prefix = format!("{}[{}]", prefix, i);
        match entry {
            FilterEntry::Condition(cond) => {
                query = serialize_condition(cond, &item_prefix, query);
            }
            FilterEntry::Group(sub) => {
                query = serialize_group(sub, &item_prefix, query);
            }
        }
    }
    query
}

fn serialize_condition(cond: &FilterCondition, prefix: &str, mut query: Query) -> Query {
    let field_path = build_field_path(&cond.field);
    let key = format!("{}{}[{}][]", prefix, field_path, cond.operator);
    match &cond.value {
        FilterValue::Single(v) => {
            query = query.filter_raw(key, v.clone());
        }
        FilterValue::Array(values) => {
            let joined = values.join(",");
            query = query.filter_raw(key, joined);
        }
    }
    query
}

/// Build bracket-notation path for a field key.
/// - Plain keys: "date" → "[date]"
/// - Dot notation: "person.company.name" → "[person.company.name]"
/// - Bracket notation: "formulas[profit]" → "[formulas][profit]"
fn build_field_path(field: &str) -> String {
    match field.find('[') {
        None => format!("[{}]", field),
        Some(base_end) => {
            let mut parts = vec![&field[..base_end]];
            // Extract all bracket contents
            let mut rest = &field[base_end..];
            while let Some(start) = rest.find('[') {
                if let Some(end) = rest[start..].find(']') {
                    parts.push(&rest[start + 1..start + end]);
                    rest = &rest[start + end + 1..];
                } else {
                    break;
                }
            }
            parts.iter().map(|p| format!("[{}]", p)).collect()
        }
    }
}

/// Get the FieldDef for a filter condition's field, checking multiple resolution paths.
pub fn resolve_filter_field<'a>(
    field_name: &str,
    resource: &'a ResourceDef,
) -> Option<&'a FieldDef> {
    // Direct filter key match
    if let Some(f) = resource.field_by_filter(field_name) {
        return Some(f);
    }
    // Try by param
    if let Some(f) = resource.field_by_param(field_name)
        && f.filter.is_some()
    {
        return Some(f);
    }
    // Try by key
    if let Some(f) = resource.fields.get(field_name)
        && f.filter.is_some()
    {
        return Some(f);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::schema;

    #[test]
    fn normalize_flat_filter() {
        let input: FilterInput =
            serde_json::from_str(r#"{"project_id": "123", "status": "1"}"#).unwrap();
        let group = normalize_filter(input);
        assert_eq!(group.op, "and");
        assert_eq!(group.conditions.len(), 2);
    }

    #[test]
    fn normalize_group_filter() {
        let input: FilterInput = serde_json::from_str(
            r#"{"op": "and", "conditions": [{"field": "project_id", "op": "eq", "value": "123"}]}"#,
        )
        .unwrap();
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
    fn filter_nested_group_serialization() {
        // filter[$op]=or
        // filter[0][status][eq][]=active
        // filter[1][$op]=and
        // filter[1][0][department_id][eq][]=100
        // filter[1][1][department_id][eq][]=200
        let group = FilterGroup {
            op: "or".to_string(),
            conditions: vec![
                FilterEntry::Condition(FilterCondition {
                    field: "status".to_string(),
                    operator: "eq".to_string(),
                    value: FilterValue::Single("active".to_string()),
                }),
                FilterEntry::Group(FilterGroup {
                    op: "and".to_string(),
                    conditions: vec![
                        FilterEntry::Condition(FilterCondition {
                            field: "department_id".to_string(),
                            operator: "eq".to_string(),
                            value: FilterValue::Single("100".to_string()),
                        }),
                        FilterEntry::Condition(FilterCondition {
                            field: "department_id".to_string(),
                            operator: "eq".to_string(),
                            value: FilterValue::Single("200".to_string()),
                        }),
                    ],
                }),
            ],
        };
        let query = filter_group_to_query(&group, Query::new());
        let qs = query.to_query_string();
        // Top-level op
        assert!(
            qs.contains("filter[$op]=or"),
            "missing top-level $op: {}",
            qs
        );
        // First condition at index 0
        assert!(
            qs.contains("filter[0][status][eq][]=active"),
            "missing status condition: {}",
            qs
        );
        // Nested group at index 1
        assert!(
            qs.contains("filter[1][$op]=and"),
            "missing nested $op: {}",
            qs
        );
        // Nested conditions at [1][0] and [1][1]
        assert!(
            qs.contains("filter[1][0][department_id][eq][]=100"),
            "missing nested cond 0: {}",
            qs
        );
        assert!(
            qs.contains("filter[1][1][department_id][eq][]=200"),
            "missing nested cond 1: {}",
            qs
        );
    }

    #[test]
    fn filter_array_value_serialization() {
        let group = FilterGroup {
            op: "and".to_string(),
            conditions: vec![FilterEntry::Condition(FilterCondition {
                field: "person_id".to_string(),
                operator: "eq".to_string(),
                value: FilterValue::Array(vec!["1".to_string(), "2".to_string(), "3".to_string()]),
            })],
        };
        let query = filter_group_to_query(&group, Query::new());
        let qs = query.to_query_string();
        assert!(
            qs.contains("1%2C2%2C3") || qs.contains("1,2,3"),
            "array values should be comma-joined: {}",
            qs
        );
    }

    #[test]
    fn filter_bracket_field_path() {
        let group = FilterGroup {
            op: "and".to_string(),
            conditions: vec![FilterEntry::Condition(FilterCondition {
                field: "custom_fields[1234]".to_string(),
                operator: "eq".to_string(),
                value: FilterValue::Single("Smith".to_string()),
            })],
        };
        let query = filter_group_to_query(&group, Query::new());
        let qs = query.to_query_string();
        assert!(
            qs.contains("custom_fields"),
            "missing custom_fields: {}",
            qs
        );
        assert!(qs.contains("1234"), "missing custom field id: {}", qs);
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
