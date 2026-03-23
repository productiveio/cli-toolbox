use serde_json::Value;

use crate::api::ProductiveClient;
use crate::json_error;
use crate::schema::ResourceDef;

pub async fn run(
    client: &ProductiveClient,
    resource: &ResourceDef,
    id: &str,
    action_name: &str,
    data: Option<&Value>,
) {
    // Look up the action in schema-level custom actions
    let action = match resource.custom_actions.get(action_name) {
        Some(a) => a,
        None => {
            let available: Vec<&str> = resource
                .custom_actions
                .keys()
                .map(|k| k.as_str())
                .collect();
            let msg = if available.is_empty() {
                format!(
                    "Unknown action '{}' on {}. This resource has no custom actions.",
                    action_name, resource.type_name
                )
            } else {
                format!(
                    "Unknown action '{}' on {}. Available: {}",
                    action_name,
                    resource.type_name,
                    available.join(", ")
                )
            };
            json_error::exit_with_error("action_not_found", &msg);
        }
    };

    // Build the API path
    let path = format!("{}/{}/{}", resource.api_path(), id, action.endpoint);

    // Build request body if params provided
    let body = data.map(|d| {
        serde_json::json!({
            "data": {
                "type": resource.type_name,
                "attributes": d
            }
        })
    });

    match client
        .custom_action(&path, &action.method, body.as_ref())
        .await
    {
        Ok(resp) => {
            if let Some(r) = resp {
                let output = serde_json::json!({"data": r.data});
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                let output = serde_json::json!({
                    "success": true,
                    "action": action_name,
                    "type": resource.type_name,
                    "id": id,
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            }
        }
        Err(e) => json_error::exit_with_tb_error(&e),
    }
}
