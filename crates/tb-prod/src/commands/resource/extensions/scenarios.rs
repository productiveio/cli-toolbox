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
        "copy" => Some(copy(client, id, data).await),
        _ => None,
    }
}

async fn copy(
    client: &ProductiveClient,
    template_id: &str,
    data: Option<&Value>,
) -> Result<ExtensionResult, String> {
    let name = data.and_then(|d| d.get("name")).and_then(|v| v.as_str());

    let mut attributes = json!({ "template_id": template_id });
    if let Some(n) = name {
        attributes["name"] = json!(n);
    }

    let body = json!({
        "data": {
            "type": "scenarios",
            "attributes": attributes,
        }
    });

    // Collection-level POST (no record ID in path)
    client
        .custom_action("/scenarios/copy", "POST", Some(&body))
        .await
        .map_err(|e| e.to_string())?;

    Ok(ExtensionResult::Json(json!({
        "success": true,
        "action": "copy",
        "templateId": template_id,
    })))
}
