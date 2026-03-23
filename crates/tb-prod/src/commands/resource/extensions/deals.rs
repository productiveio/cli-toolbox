use serde_json::{json, Value};

use crate::api::{ProductiveClient, Query};

use super::ExtensionResult;

pub async fn dispatch(
    client: &ProductiveClient,
    id: &str,
    action_name: &str,
    _data: Option<&Value>,
) -> Option<Result<ExtensionResult, String>> {
    match action_name {
        "load_activity" => Some(load_activity(client, id).await),
        _ => None,
    }
}

async fn load_activity(client: &ProductiveClient, deal_id: &str) -> Result<ExtensionResult, String> {
    let deal_path = format!("/deals/{}", deal_id);
    let deal_resp = client.get_one(&deal_path).await.map_err(|e| e.to_string())?;

    let query = Query::new()
        .filter_array("deal_id", deal_id)
        .sort("-created_at");
    let activities_resp = client.get_page("/activities", &query, 1, 200).await.map_err(|e| e.to_string())?;

    let output = json!({
        "deal": deal_resp.data,
        "activities": activities_resp.data,
        "included": activities_resp.included,
        "summary": {
            "activityCount": activities_resp.data.len(),
        }
    });

    Ok(ExtensionResult::Json(output))
}
