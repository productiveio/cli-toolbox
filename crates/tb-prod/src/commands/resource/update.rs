use serde_json::Value;

use crate::api::ProductiveClient;
use crate::body;
use crate::json_error;
use crate::schema::ResourceDef;
use crate::validate;

pub async fn run(client: &ProductiveClient, resource: &ResourceDef, id: &str, data: &Value) {
    if !resource.supports_action("update") {
        json_error::exit_with_error(
            "operation_not_supported",
            &format!("{} does not support update.", resource.type_name),
        );
    }

    let schema = crate::schema::schema();

    // Validate
    let errors = validate::validate_update(resource, data, schema);
    if !errors.is_empty() {
        json_error::exit_with_error_details(
            "validation_error",
            &errors[0],
            Some(serde_json::json!({"errors": errors})),
        );
    }

    // Build JSONAPI body with ID
    let payload = match body::build_jsonapi_body(resource, data, Some(id)) {
        Ok(p) => p,
        Err(errors) => {
            json_error::exit_with_error_details(
                "body_construction_error",
                &errors[0],
                Some(serde_json::json!({"errors": errors})),
            );
        }
    };

    let path = format!("{}/{}", resource.api_path(), id);
    match client.update(&path, &payload).await {
        Ok(resp) => {
            let output = serde_json::json!({"data": resp.data});
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
        Err(e) => json_error::exit_with_tb_error(&e),
    }
}
