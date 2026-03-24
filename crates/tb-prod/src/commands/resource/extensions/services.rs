use serde_json::{Value, json};

use crate::api::ProductiveClient;

use super::ExtensionResult;

pub async fn dispatch(
    client: &ProductiveClient,
    id: &str,
    action_name: &str,
    data: Option<&Value>,
) -> Option<Result<ExtensionResult, String>> {
    match action_name {
        "move" => Some(move_service(client, id, data).await),
        _ => None,
    }
}

async fn move_service(
    client: &ProductiveClient,
    service_id: &str,
    data: Option<&Value>,
) -> Result<ExtensionResult, String> {
    let target_deal_id = data
        .and_then(|d| d.get("target_id").or(d.get("target_deal_id")))
        .and_then(|v| v.as_str())
        .ok_or("Missing 'target_id' in action data.")?;

    let path = format!("/services/{}/move", service_id);
    let body = json!({
        "data": {
            "type": "services",
            "attributes": {
                "target_id": target_deal_id
            }
        }
    });

    client
        .custom_action(&path, "PATCH", Some(&body))
        .await
        .map_err(|e| e.to_string())?;

    Ok(ExtensionResult::Json(json!({
        "success": true,
        "action": "move",
        "serviceId": service_id,
        "targetDealId": target_deal_id,
    })))
}
