use serde_json::{json, Value};

use crate::api::ProductiveClient;

use super::ExtensionResult;

pub async fn dispatch(
    client: &ProductiveClient,
    id: &str,
    action_name: &str,
    data: Option<&Value>,
) -> Option<Result<ExtensionResult, String>> {
    match action_name {
        "invite" => Some(invite(client, id, data).await),
        _ => None,
    }
}

async fn invite(client: &ProductiveClient, person_id: &str, data: Option<&Value>) -> Result<ExtensionResult, String> {
    let company_id = data
        .and_then(|d| d.get("company_id"))
        .and_then(|v| v.as_str())
        .ok_or("Missing 'company_id' in action data.")?;

    let mut relationships = json!({
        "company": { "data": { "type": "companies", "id": company_id } }
    });

    if let Some(role_id) = data.and_then(|d| d.get("custom_role_id")).and_then(|v| v.as_str()) {
        relationships["custom_role"] = json!({ "data": { "type": "roles", "id": role_id } });
    }
    if let Some(sub_id) = data.and_then(|d| d.get("subsidiary_id")).and_then(|v| v.as_str()) {
        relationships["subsidiary"] = json!({ "data": { "type": "subsidiaries", "id": sub_id } });
    }

    let body = json!({
        "data": {
            "type": "people",
            "relationships": relationships,
        }
    });

    let path = format!("/people/{}/invite", person_id);
    client.custom_action(&path, "PATCH", Some(&body)).await.map_err(|e| e.to_string())?;

    Ok(ExtensionResult::Json(json!({
        "success": true,
        "action": "invite",
        "personId": person_id,
    })))
}
