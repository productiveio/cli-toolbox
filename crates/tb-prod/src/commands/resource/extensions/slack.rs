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
        "send" => Some(send_message(client, data).await),
        _ => None,
    }
}

async fn send_message(client: &ProductiveClient, data: Option<&Value>) -> Result<ExtensionResult, String> {
    let channel_id = data
        .and_then(|d| d.get("channel_id"))
        .and_then(|v| v.as_str())
        .ok_or("Missing 'channel_id' in action data.")?;
    let text = data
        .and_then(|d| d.get("text"))
        .and_then(|v| v.as_str())
        .ok_or("Missing 'text' in action data.")?;

    let body = json!({
        "data": {
            "type": "slack_messages",
            "attributes": {
                "channel_id": channel_id,
                "text": text,
            }
        }
    });

    client.create("/slack_messages", &body).await.map_err(|e| e.to_string())?;

    Ok(ExtensionResult::Json(json!({
        "success": true,
        "action": "send",
        "channelId": channel_id,
    })))
}
