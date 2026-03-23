use serde_json::Value;

use crate::api::ProductiveClient;
use crate::body;
use crate::json_error;
use crate::schema::ResourceDef;
use crate::validate;

pub async fn run(client: &ProductiveClient, resource: &ResourceDef, data: &Value) {
    let schema = crate::schema::schema();

    // Bulk or single?
    if let Some(items) = data.as_array() {
        run_bulk(client, resource, items).await;
    } else {
        run_single(client, resource, data, schema).await;
    }
}

async fn run_single(client: &ProductiveClient, resource: &ResourceDef, data: &Value, schema: &crate::schema::Schema) {
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
            let output = serde_json::json!({"data": resp.data});
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
        Err(e) => json_error::exit_with_tb_error(&e),
    }
}

async fn run_bulk(client: &ProductiveClient, resource: &ResourceDef, items: &[Value]) {
    if !resource.supports_bulk("create") {
        json_error::exit_with_error(
            "bulk_not_supported",
            &format!("{} does not support bulk create.", resource.type_name),
        );
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
            let output = serde_json::json!({"data": resp.data});
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
        Err(e) => json_error::exit_with_tb_error(&e),
    }
}
