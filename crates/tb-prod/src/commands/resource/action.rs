use serde_json::Value;

use crate::api::ProductiveClient;
use crate::json_error;
use crate::schema::ResourceDef;

use super::extensions;

pub async fn run(
    client: &ProductiveClient,
    resource: &ResourceDef,
    id: &str,
    action_name: &str,
    data: Option<&Value>,
) {
    // Try extension actions first
    if let Some(result) = extensions::dispatch(client, &resource.type_name, id, action_name, data).await {
        match result {
            Ok(extensions::ExtensionResult::Json(v)) => {
                println!("{}", serde_json::to_string_pretty(&v).unwrap());
            }
            Ok(extensions::ExtensionResult::Text(t)) => {
                println!("{}", t);
            }
            Err(e) => {
                json_error::exit_with_error("extension_action_error", &e);
            }
        }
        return;
    }

    // Fall back to schema-level custom actions
    let action = match resource.custom_actions.get(action_name) {
        Some(a) => a,
        None => {
            let mut available: Vec<&str> = resource
                .custom_actions
                .keys()
                .map(|k| k.as_str())
                .collect();
            // Include extension action names in the hint
            let ext_names = extension_action_names(&resource.type_name);
            available.extend(ext_names.iter().map(|s| s.as_str()));

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

/// Return known extension action names for a resource type (for error hints).
fn extension_action_names(resource_type: &str) -> Vec<String> {
    match resource_type {
        "tasks" => vec!["load_activity".into(), "resolve_subscriber_ids".into()],
        "deals" => vec!["load_activity".into()],
        "notifications" => vec!["load_details".into()],
        "bookings" => vec!["find_conflicts".into(), "capacity_availability".into()],
        "services" => vec!["move".into()],
        "service_types" => vec!["merge".into()],
        "people" => vec!["invite".into()],
        "slack_messages" => vec!["send".into()],
        "scenarios" => vec!["copy".into()],
        _ => vec![],
    }
}
