use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::api::ProductiveClient;
use crate::cache::{self, Cache};
use crate::error::Result;
use crate::output;

#[derive(Debug, Deserialize)]
struct BatchInput {
    title: String,
    project: String,
    task_list: String,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    assignee: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    due_date: Option<String>,
}

#[derive(Debug, Serialize)]
struct ResolvedTask {
    title: String,
    project_id: String,
    task_list_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    workflow_status_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    assignee_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    due_date: Option<String>,
}

#[derive(Debug, Serialize)]
struct CreatedTask {
    id: String,
    number: String,
    title: String,
    url: String,
}

pub async fn run(
    client: &ProductiveClient,
    cache: &Cache,
    json_content: &str,
    dry_run: bool,
    json_output: bool,
) -> Result<()> {
    let inputs: Vec<BatchInput> = serde_json::from_str(json_content)
        .map_err(|e| crate::error::TbProdError::Other(format!("Invalid batch JSON: {}", e)))?;

    if inputs.is_empty() {
        return Err(crate::error::TbProdError::Other("Batch file contains no tasks".into()));
    }

    if inputs.len() > 50 {
        return Err(crate::error::TbProdError::Other(
            format!("Batch limited to 50 tasks, got {}", inputs.len()),
        ));
    }

    // Resolve all names to IDs
    let mut resolved = Vec::with_capacity(inputs.len());
    for (i, input) in inputs.iter().enumerate() {
        let project_id = cache.resolve_project(&input.project)
            .map_err(|e| crate::error::TbProdError::Other(format!("Task {}: {}", i + 1, e)))?;

        let workflow_id = cache.workflow_id_for_project(&project_id)?;

        let task_list_id = cache::resolve_task_list(client, &input.task_list, Some(&project_id)).await
            .map_err(|e| crate::error::TbProdError::Other(format!("Task {}: {}", i + 1, e)))?;

        let status_id = input.status.as_deref()
            .map(|s| cache.resolve_workflow_status(s, workflow_id.as_deref()))
            .transpose()
            .map_err(|e| crate::error::TbProdError::Other(format!("Task {}: {}", i + 1, e)))?;

        let assignee_id = input.assignee.as_deref()
            .map(|a| cache.resolve_person(a))
            .transpose()
            .map_err(|e| crate::error::TbProdError::Other(format!("Task {}: {}", i + 1, e)))?;

        resolved.push(ResolvedTask {
            title: input.title.clone(),
            project_id,
            task_list_id,
            workflow_status_id: status_id,
            assignee_id,
            description: input.description.clone(),
            due_date: input.due_date.clone(),
        });
    }

    if dry_run {
        println!("{}", serde_json::to_string_pretty(&resolved)?);
        eprintln!("Dry run — {} tasks validated, none created.", resolved.len());
        return Ok(());
    }

    // Build bulk payload — Productive bulk create uses attributes for IDs
    let data: Vec<serde_json::Value> = resolved.iter().map(|t| {
        let mut attrs = json!({
            "title": t.title,
            "project_id": t.project_id,
            "task_list_id": t.task_list_id,
            "private": false,
        });
        if let Some(ref sid) = t.workflow_status_id {
            attrs["workflow_status_id"] = json!(sid);
        }
        if let Some(ref aid) = t.assignee_id {
            attrs["assignee_id"] = json!(aid);
        }
        if let Some(ref desc) = t.description {
            attrs["description"] = json!(desc);
        }
        if let Some(ref dd) = t.due_date {
            attrs["due_date"] = json!(dd);
        }
        json!({ "type": "tasks", "attributes": attrs })
    }).collect();

    let payload = json!({ "data": data });
    let resp = client.bulk_create_tasks(&payload).await?;

    let created: Vec<CreatedTask> = resp.data.iter().map(|task| {
        CreatedTask {
            id: task.id.clone(),
            number: task.attr_str("number").to_string(),
            title: task.attr_str("title").to_string(),
            url: format!(
                "https://app.productive.io/{}/tasks/task/{}",
                client.org_id(),
                task.id
            ),
        }
    }).collect();

    if json_output {
        println!("{}", output::render_json(&created));
    } else {
        eprintln!("Created {} tasks:", created.len());
        for t in &created {
            println!("  #{} (ID: {}) — {}", t.number, t.id, t.title);
        }
    }

    Ok(())
}
