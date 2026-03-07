use serde::Serialize;
use serde_json::json;

use crate::api::ProductiveClient;
use crate::error::Result;
use crate::output;

#[derive(Debug, Serialize)]
struct TodoRow {
    id: String,
    description: String,
    done: bool,
    due_date: String,
}

pub async fn list(client: &ProductiveClient, task_id: &str, json_output: bool) -> Result<()> {
    let resp = client.get_todos(task_id).await?;

    let rows: Vec<TodoRow> = resp
        .data
        .iter()
        .map(|r| TodoRow {
            id: r.id.clone(),
            description: r.attr_str("description").to_string(),
            done: r.attr_bool("closed"),
            due_date: r.attr_str("due_date").to_string(),
        })
        .collect();

    if json_output {
        println!("{}", output::render_json(&rows));
        return Ok(());
    }

    if rows.is_empty() {
        println!("No todos.");
        return Ok(());
    }

    for row in &rows {
        let check = if row.done { "[x]" } else { "[ ]" };
        let due = if row.due_date.is_empty() {
            String::new()
        } else {
            format!(" (due: {})", row.due_date)
        };
        println!("{} {} (ID: {}){}", check, row.description, row.id, due);
    }

    Ok(())
}

pub async fn create(
    client: &ProductiveClient,
    task_id: &str,
    title: &str,
    assignee_id: Option<&str>,
    json_output: bool,
) -> Result<()> {
    let mut relationships = json!({
        "task": { "data": { "type": "tasks", "id": task_id } }
    });

    if let Some(aid) = assignee_id {
        relationships["assignee"] = json!({ "data": { "type": "people", "id": aid } });
    }

    let payload = json!({
        "data": {
            "type": "todos",
            "attributes": {
                "description": title
            },
            "relationships": relationships
        }
    });

    let resp = client.create("/todos", &payload).await?;
    let todo = &resp.data;

    if json_output {
        let out = json!({ "id": todo.id, "description": title, "status": "created" });
        println!("{}", output::render_json(&out));
    } else {
        println!("Todo created (ID: {})", todo.id);
    }

    Ok(())
}

pub async fn update(
    client: &ProductiveClient,
    todo_id: &str,
    done: Option<bool>,
    title: Option<&str>,
    json_output: bool,
) -> Result<()> {
    let mut attributes = json!({});

    if let Some(done) = done {
        attributes["closed"] = json!(done);
    }

    if let Some(t) = title {
        attributes["description"] = json!(t);
    }

    let payload = json!({
        "data": {
            "type": "todos",
            "id": todo_id,
            "attributes": attributes
        }
    });

    let resp = client.update(&format!("/todos/{}", todo_id), &payload).await?;
    let todo = &resp.data;

    if json_output {
        let out = json!({ "id": todo.id, "status": "updated" });
        println!("{}", output::render_json(&out));
    } else {
        println!("Todo updated (ID: {})", todo.id);
    }

    Ok(())
}
