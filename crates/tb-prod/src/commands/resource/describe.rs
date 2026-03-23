use crate::schema::{self, operators_for_field, ResourceDef, TypeCategory};

use super::extensions;

pub fn run(resource: &ResourceDef, include: Option<&str>) {
    let schema = schema::schema();
    let includes: Vec<&str> = include
        .map(|i| i.split(',').map(|s| s.trim()).collect())
        .unwrap_or_default();

    // Header
    println!(
        "{} — {}",
        resource.type_name, resource.description_short
    );
    if let Some(aliases) = &resource.aliases {
        if !aliases.is_empty() {
            println!("Aliases: {}", aliases.join(", "));
        }
    }
    println!();

    // Operations
    let ops = [
        ("query", resource.supports_action("index")),
        ("search", resource.search_filter_param.is_some()),
        ("create", resource.supports_action("create")),
        ("update", resource.supports_action("update")),
        ("delete", resource.supports_action("delete")),
    ];
    let bulk_ops = [
        ("bulk create", resource.supports_bulk("create")),
        ("bulk update", resource.supports_bulk("update")),
        ("bulk delete", resource.supports_bulk("delete")),
    ];
    let ops_str: Vec<String> = ops
        .iter()
        .chain(bulk_ops.iter())
        .filter(|(_, supported)| *supported)
        .map(|(name, _)| format!("{} ✓", name))
        .collect();
    println!("Operations: {}", ops_str.join(" | "));

    // Custom actions (schema-level + extensions)
    let schema_actions: Vec<&str> = resource
        .custom_actions
        .values()
        .map(|a| a.name.as_str())
        .collect();
    let ext_actions = extensions::action_names(&resource.type_name);
    if schema_actions.is_empty() && ext_actions.is_empty() {
        println!("Actions: (none)");
    } else {
        let mut parts = Vec::new();
        if !schema_actions.is_empty() {
            parts.push(format!("{} (schema)", schema_actions.join(", ")));
        }
        if !ext_actions.is_empty() {
            parts.push(format!("{} (extension)", ext_actions.join(", ")));
        }
        println!("Actions: {}", parts.join(" | "));
    }
    println!();

    // Query hints
    if let Some(hints) = &resource.query_hints {
        println!("Query hints:");
        for line in hints.lines() {
            println!("  {}", line);
        }
        println!();
    }

    // Related types
    let related: Vec<&str> = resource
        .fields
        .values()
        .filter(|f| f.type_category == TypeCategory::Resource)
        .map(|f| f.field_type.as_str())
        .chain(resource.collections.values().map(|c| c.collection_type.as_str()))
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    if !related.is_empty() {
        let mut sorted = related;
        sorted.sort();
        println!("Related types: {}", sorted.join(", "));
        println!();
    }

    // --- Progressive disclosure sections ---

    if includes.contains(&"schema") {
        println!("--- Schema ---\n");

        // Fields table
        println!("Fields:");
        let mut fields: Vec<_> = resource.fields.values().collect();
        fields.sort_by_key(|f| &f.key);

        for f in &fields {
            if !f.serialize {
                continue; // skip hidden fields from display
            }
            let mut flags = Vec::new();
            if f.id {
                flags.push("id");
            }
            if f.required {
                flags.push("required");
            }
            if f.readonly {
                flags.push("readonly");
            }
            if f.create_only {
                flags.push("createOnly");
            }
            if f.filter.is_some() {
                flags.push("filterable");
            }
            if f.sort.is_some() {
                flags.push("sortable");
            }
            if f.array {
                flags.push("array");
            }

            let flags_str = if flags.is_empty() {
                String::new()
            } else {
                format!("[{}]", flags.join(", "))
            };

            let desc = f.description.as_deref().unwrap_or("");
            println!(
                "  {:<24} {:<16} {:<40} {}",
                f.key, f.field_type, flags_str, desc
            );
        }
        println!();

        // Filters table
        println!("Filters:");
        let mut filterable: Vec<_> = resource
            .fields
            .values()
            .filter(|f| f.filter.is_some())
            .collect();
        filterable.sort_by_key(|f| f.filter.as_deref().unwrap_or(""));

        for f in &filterable {
            let filter_key = f.filter.as_deref().unwrap_or("");
            let ops = operators_for_field(f);
            println!(
                "  {:<28} {:<16} {}",
                filter_key,
                f.field_type,
                ops.join(", ")
            );
        }

        // Dot-notation relationship filters
        let rel_fields: Vec<_> = resource
            .fields
            .values()
            .filter(|f| f.type_category == TypeCategory::Resource && f.relationship.is_some())
            .collect();
        if !rel_fields.is_empty() {
            println!();
            println!("Relationship filters (dot-notation):");
            for rf in &rel_fields {
                let rel_name = rf.relationship.as_deref().unwrap_or("");
                if let Some(related_resource) = schema.resources.get(&rf.field_type) {
                    let sub_filters: Vec<&str> = related_resource
                        .fields
                        .values()
                        .filter_map(|f| f.filter.as_deref())
                        .take(5)
                        .collect();
                    if !sub_filters.is_empty() {
                        println!(
                            "  {}.{{{}...}}",
                            rel_name,
                            sub_filters.join(", ")
                        );
                    }
                }
            }
        }

        println!();

        // Sort fields
        let mut sort_fields: Vec<&str> = resource
            .fields
            .values()
            .filter_map(|f| f.sort.as_deref())
            .collect();
        sort_fields.sort();
        if !sort_fields.is_empty() {
            println!("Sort fields: {}", sort_fields.join(", "));
        }
        if let Some(default) = &resource.default_sort {
            println!("Default sort: {}", default);
        }

        // Search config
        if let Some(param) = &resource.search_filter_param {
            println!("Search: keyword via filter param \"{}\"", param);
        }
        println!();
    }

    if includes.contains(&"actions") {
        println!("--- Actions ---\n");
        if resource.custom_actions.is_empty() {
            println!("No custom actions available.");
        } else {
            for action in resource.custom_actions.values() {
                println!(
                    "  {} — {} {} /{}/<id>/{}",
                    action.name, action.method, action.description, resource.type_name, action.endpoint
                );
            }
        }
        println!();
    }

    if includes.contains(&"related") {
        println!("--- Related (collections) ---\n");
        if resource.collections.is_empty() {
            println!("No collections.");
        } else {
            for col in resource.collections.values() {
                let inverse = col
                    .inverse
                    .as_deref()
                    .map(|i| format!(" (inverse: {})", i))
                    .unwrap_or_default();
                println!(
                    "  {} → {} (has-many){}",
                    col.collection_name, col.collection_type, inverse
                );
            }
        }
        println!();
    }
}

/// Print all resource types when an invalid type is provided.
pub fn print_all_types() {
    let schema = schema::schema();
    let grouped = schema.resources_by_domain();

    println!("Available resource types:\n");
    for (domain, resources) in grouped {
        println!("## {}", domain);
        for r in resources {
            println!("  {:<28} {}", r.type_name, r.description_short);
        }
        println!();
    }
}
