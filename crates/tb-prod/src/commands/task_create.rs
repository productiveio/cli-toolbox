use serde::Serialize;
use serde_json::json;

use crate::api::ProductiveClient;
use crate::error::Result;
use crate::output;

#[derive(Debug, Serialize)]
struct CreatedTask {
    id: String,
    number: String,
    title: String,
    url: String,
}

pub async fn run(
    client: &ProductiveClient,
    title: &str,
    project_id: &str,
    task_list_id: &str,
    workflow_status_id: Option<&str>,
    assignee_id: Option<&str>,
    description: Option<&str>,
    due_date: Option<&str>,
    json: bool,
) -> Result<()> {
    let mut relationships = json!({
        "project": { "data": { "type": "projects", "id": project_id } },
        "task_list": { "data": { "type": "task_lists", "id": task_list_id } }
    });

    if let Some(status_id) = workflow_status_id {
        relationships["workflow_status"] = json!({ "data": { "type": "workflow_statuses", "id": status_id } });
    }
    if let Some(aid) = assignee_id {
        relationships["assignee"] = json!({ "data": { "type": "people", "id": aid } });
    }

    let mut attributes = json!({
        "title": title,
        "private": false
    });

    if let Some(desc) = description {
        attributes["description"] = json!(desc);
    }
    if let Some(dd) = due_date {
        attributes["due_date"] = json!(dd);
    }

    let payload = json!({
        "data": {
            "type": "tasks",
            "attributes": attributes,
            "relationships": relationships
        }
    });

    let resp = client.create_task(&payload).await?;
    let task = &resp.data;

    let created = CreatedTask {
        id: task.id.clone(),
        number: task.attr_str("number").to_string(),
        title: task.attr_str("title").to_string(),
        url: format!(
            "https://app.productive.io/109-productive/tasks/task/{}",
            task.id
        ),
    };

    if json {
        println!("{}", output::render_json(&created));
    } else {
        println!("Created task #{} (ID: {})", created.number, created.id);
        println!("Title: {}", created.title);
        println!("URL:   {}", created.url);
    }

    Ok(())
}
