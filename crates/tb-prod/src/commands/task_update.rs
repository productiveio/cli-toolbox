use serde_json::json;

use crate::api::ProductiveClient;
use crate::error::Result;

pub async fn run(
    client: &ProductiveClient,
    task_id: &str,
    workflow_status_id: Option<&str>,
    title: Option<&str>,
    assignee_id: Option<&str>,
    json_output: bool,
) -> Result<()> {
    let mut relationships = serde_json::Map::new();
    let mut attributes = serde_json::Map::new();

    if let Some(status_id) = workflow_status_id {
        relationships.insert("workflow_status".into(), json!({ "data": { "type": "workflow_statuses", "id": status_id } }));
    }
    if let Some(aid) = assignee_id {
        relationships.insert("assignee".into(), json!({ "data": { "type": "people", "id": aid } }));
    }
    if let Some(t) = title {
        attributes.insert("title".into(), json!(t));
    }

    let mut data = json!({
        "type": "tasks",
        "id": task_id,
    });

    if !relationships.is_empty() {
        data["relationships"] = serde_json::Value::Object(relationships);
    }
    if !attributes.is_empty() {
        data["attributes"] = serde_json::Value::Object(attributes);
    }

    let payload = json!({ "data": data });

    let resp = client.update_task(task_id, &payload).await?;
    let task = &resp.data;

    if json_output {
        let out = json!({
            "id": task.id,
            "number": task.attr_str("number"),
            "title": task.attr_str("title"),
            "status": "updated"
        });
        println!("{}", crate::output::render_json(&out));
    } else {
        println!(
            "Updated task #{} (ID: {})",
            task.attr_str("number"),
            task.id
        );
    }

    Ok(())
}
