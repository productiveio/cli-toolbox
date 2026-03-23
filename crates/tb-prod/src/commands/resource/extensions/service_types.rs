use serde_json::{json, Value};

use crate::api::ProductiveClient;

use super::ExtensionResult;

pub async fn dispatch(
    client: &ProductiveClient,
    _id: &str,
    action_name: &str,
    data: Option<&Value>,
) -> Option<Result<ExtensionResult, String>> {
    match action_name {
        "merge" => Some(merge(client, data).await),
        _ => None,
    }
}

async fn merge(client: &ProductiveClient, data: Option<&Value>) -> Result<ExtensionResult, String> {
    let winner_id = data
        .and_then(|d| d.get("winner_id"))
        .and_then(|v| v.as_str())
        .ok_or("Missing 'winner_id' in action data.")?;
    let loser_id = data
        .and_then(|d| d.get("loser_id"))
        .and_then(|v| v.as_str())
        .ok_or("Missing 'loser_id' in action data.")?;

    // merge_service_types uses a flat body (not JSON:API wrapped)
    let body = json!({
        "winner_id": winner_id,
        "loser_id": loser_id,
    });

    let path = "/service_types/merge";
    client.custom_action(path, "PATCH", Some(&body)).await.map_err(|e| e.to_string())?;

    Ok(ExtensionResult::Json(json!({
        "success": true,
        "action": "merge",
        "winnerId": winner_id,
        "loserId": loser_id,
    })))
}
