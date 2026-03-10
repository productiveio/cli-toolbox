use serde_json::json;

use crate::api::ProductiveClient;
use crate::error::Result;

pub struct TaskUpdateParams<'a> {
    pub task_id: &'a str,
    pub workflow_status_id: Option<&'a str>,
    pub title: Option<&'a str>,
    pub assignee_id: Option<&'a str>,
    pub description: Option<&'a str>,
    pub due_date: Option<&'a str>,
    pub starts_on: Option<&'a str>,
    pub task_list_id: Option<&'a str>,
}

pub async fn run(
    client: &ProductiveClient,
    params: &TaskUpdateParams<'_>,
    json_output: bool,
) -> Result<()> {
    let mut relationships = serde_json::Map::new();
    let mut attributes = serde_json::Map::new();

    if let Some(status_id) = params.workflow_status_id {
        relationships.insert(
            "workflow_status".into(),
            json!({ "data": { "type": "workflow_statuses", "id": status_id } }),
        );
    }
    if let Some(aid) = params.assignee_id {
        relationships.insert(
            "assignee".into(),
            json!({ "data": { "type": "people", "id": aid } }),
        );
    }
    if let Some(tl_id) = params.task_list_id {
        relationships.insert(
            "task_list".into(),
            json!({ "data": { "type": "task_lists", "id": tl_id } }),
        );
    }
    if let Some(t) = params.title {
        attributes.insert("title".into(), json!(t));
    }
    if let Some(d) = params.description {
        attributes.insert("description".into(), json!(d));
    }
    if let Some(dd) = params.due_date {
        attributes.insert("due_date".into(), json!(dd));
    }
    if let Some(so) = params.starts_on {
        attributes.insert("starts_on".into(), json!(so));
    }

    let mut data = json!({
        "type": "tasks",
        "id": params.task_id,
    });

    if !relationships.is_empty() {
        data["relationships"] = serde_json::Value::Object(relationships);
    }
    if !attributes.is_empty() {
        data["attributes"] = serde_json::Value::Object(attributes);
    }

    let payload = json!({ "data": data });

    let resp = client.update_task(params.task_id, &payload).await?;
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
