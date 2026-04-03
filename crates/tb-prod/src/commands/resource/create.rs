use serde_json::Value;

use crate::api::ProductiveClient;
use crate::body;
use crate::commands::resource::query;
use crate::json_error;
use crate::schema::ResourceDef;
use crate::validate;

pub async fn run(
    client: &ProductiveClient,
    resource: &ResourceDef,
    data: &Value,
    format: &str,
) {
    let schema = crate::schema::schema();

    // Bulk or single?
    if let Some(items) = data.as_array() {
        run_bulk(client, resource, items, format).await;
    } else {
        run_single(client, resource, data, schema, format).await;
    }
}

async fn run_single(
    client: &ProductiveClient,
    resource: &ResourceDef,
    data: &Value,
    schema: &crate::schema::Schema,
    format: &str,
) {
    if !resource.supports_action("create") {
        json_error::exit_with_error(
            "operation_not_supported",
            &format!("{} does not support create.", resource.type_name),
        );
    }

    // Validate
    let errors = validate::validate_create(resource, data, schema);
    if !errors.is_empty() {
        json_error::exit_with_error_details(
            "validation_error",
            &errors[0],
            Some(serde_json::json!({"errors": errors})),
        );
    }

    // Build JSONAPI body
    let payload = match body::build_jsonapi_body(resource, data, None) {
        Ok(p) => p,
        Err(errors) => {
            json_error::exit_with_error_details(
                "body_construction_error",
                &errors[0],
                Some(serde_json::json!({"errors": errors})),
            );
        }
    };

    let path = resource.api_path();
    match client.create(&path, &payload).await {
        Ok(resp) => {
            if format == "json" {
                let output = serde_json::json!({"data": resp.data});
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                let name = query::extract_display_name(&resp.data);
                if name.is_empty() {
                    println!("Created {} {}", resource.item_name, resp.data.id);
                } else {
                    println!(
                        "Created {} {} — {}",
                        resource.item_name, resp.data.id, name
                    );
                }
            }
        }
        Err(e) => json_error::exit_with_tb_error(&e),
    }
}

async fn run_bulk(
    client: &ProductiveClient,
    resource: &ResourceDef,
    items: &[Value],
    format: &str,
) {
    if !resource.supports_bulk("create") {
        json_error::exit_with_error(
            "bulk_not_supported",
            &format!("{} does not support bulk create.", resource.type_name),
        );
    }

    // Validate each item
    let schema = crate::schema::schema();
    for (i, item) in items.iter().enumerate() {
        let errors = validate::validate_create(resource, item, schema);
        if !errors.is_empty() {
            json_error::exit_with_error_details(
                "validation_error",
                &format!("Item {}: {}", i, &errors[0]),
                Some(serde_json::json!({"item_index": i, "errors": errors})),
            );
        }
    }

    // Build bulk JSONAPI body
    let payload = match body::build_jsonapi_bulk_body(resource, items) {
        Ok(p) => p,
        Err(errors) => {
            json_error::exit_with_error_details(
                "body_construction_error",
                &errors[0],
                Some(serde_json::json!({"errors": errors})),
            );
        }
    };

    let path = resource.api_path();
    match client.bulk_create(&path, &payload).await {
        Ok(resp) => {
            if format == "json" {
                let output = serde_json::json!({"data": resp.data});
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                println!(
                    "Created {} {}",
                    resp.data.len(),
                    resource.collection_name
                );
                for r in &resp.data {
                    let name = query::extract_display_name(r);
                    if name.is_empty() {
                        println!("  {} {}", resource.item_name, r.id);
                    } else {
                        println!("  {} {} — {}", resource.item_name, r.id, name);
                    }
                }
            }
        }
        Err(e) => json_error::exit_with_tb_error(&e),
    }
}
