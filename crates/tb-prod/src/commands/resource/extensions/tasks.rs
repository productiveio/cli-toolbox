use serde_json::{Value, json};

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
        "resolve_subscriber_ids" => Some(resolve_subscriber_ids(client, id).await),
        _ => None,
    }
}

async fn load_activity(
    client: &ProductiveClient,
    task_id: &str,
) -> Result<ExtensionResult, String> {
    // Fetch task
    let task_path = format!("/tasks/{}?include=project", task_id);
    let task_resp = client
        .get_one(&task_path)
        .await
        .map_err(|e| e.to_string())?;

    // Fetch activities
    let query = Query::new()
        .filter_array("task_id", task_id)
        .sort("-created_at");
    let activities_resp = client
        .get_page("/activities", &query, 1, 200)
        .await
        .map_err(|e| e.to_string())?;

    let output = json!({
        "task": task_resp.data,
        "activities": activities_resp.data,
        "included": activities_resp.included,
        "summary": {
            "activityCount": activities_resp.data.len(),
        }
    });

    Ok(ExtensionResult::Json(output))
}

async fn resolve_subscriber_ids(
    client: &ProductiveClient,
    task_id: &str,
) -> Result<ExtensionResult, String> {
    // Fetch people subscribed to this task
    let query = Query::new()
        .filter("subscribable_type", "Task")
        .filter("subscribable_id", task_id)
        .filter("status", "1");
    let resp = client
        .get_all("/people", &query, 5)
        .await
        .map_err(|e| e.to_string())?;

    let ids: Vec<&str> = resp.data.iter().map(|r| r.id.as_str()).collect();

    let output = json!({
        "subscriber_ids": ids,
        "count": ids.len(),
    });

    Ok(ExtensionResult::Json(output))
}
