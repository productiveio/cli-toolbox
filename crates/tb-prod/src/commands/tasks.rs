use serde::Serialize;

use crate::api::{ProductiveClient, Query, Resource};
use crate::config::Config;
use crate::error::Result;
use crate::output;

#[derive(Debug, Serialize)]
struct TaskRow {
    id: String,
    number: String,
    title: String,
    status: String,
    assignee: String,
    project: String,
    updated: String,
}

#[allow(clippy::too_many_arguments)]
pub async fn run(
    client: &ProductiveClient,
    config: &Config,
    task_list_id: Option<&str>,
    project_id: Option<&str>,
    assignee_override: Option<&str>,
    category: Option<&str>,
    search: Option<&str>,
    json: bool,
) -> Result<()> {
    let mut query = Query::new()
        .include("workflow_status,assignee,project")
        .sort("-updated_at");

    // Default: active board/task_list/project
    query = query
        .filter("board_status", "1")
        .filter("task_list_status", "1");

    // Task list filter (P0)
    if let Some(tl_id) = task_list_id {
        query = query.filter_array("task_list_id", tl_id);
    }

    // Project filter
    if let Some(pid) = project_id {
        query = query.filter_array("project_id", pid);
    }

    // Category filter (P0 — supports "all" to include closed)
    match category {
        Some("all") => { /* no category filter — include everything */ }
        Some("closed") => {
            query = query.filter_array("workflow_status_category_id", "3");
        }
        Some("started") => {
            query = query.filter_array("workflow_status_category_id", "2");
        }
        Some("not-started") => {
            query = query.filter_array("workflow_status_category_id", "1");
        }
        Some("open") | None => {
            // Default: not-started + started (exclude closed)
            query = query
                .filter_array("workflow_status_category_id", "1")
                .filter_array("workflow_status_category_id", "2");
        }
        Some(other) => {
            return Err(crate::error::TbProdError::Other(format!(
                "Unknown category '{}'. Use: all, open, closed, started, not-started",
                other
            )));
        }
    }

    // Text search (P1)
    if let Some(q) = search {
        query = query.filter("query", q);
    }

    // Assignee filter: explicit override > default (when no task_list/search)
    if let Some(aid) = assignee_override {
        query = query.filter_array("assignee_id", aid);
    } else if task_list_id.is_none()
        && search.is_none()
        && let Some(ref pid) = config.person_id
    {
        query = query.filter_array("assignee_id", pid);
    }

    let resp = client.list_tasks(&query).await?;

    let rows: Vec<TaskRow> = resp
        .data
        .iter()
        .map(|task| {
            let status_id = task.relationship_id("workflow_status").unwrap_or("");
            let assignee_id = task.relationship_id("assignee").unwrap_or("");
            let project_id = task.relationship_id("project").unwrap_or("");

            let status_name = find_included(&resp.included, "workflow_statuses", status_id)
                .map(|r| r.attr_str("name").to_string())
                .unwrap_or_default();
            let assignee_name = find_included(&resp.included, "people", assignee_id)
                .map(|r| format!("{} {}", r.attr_str("first_name"), r.attr_str("last_name")))
                .unwrap_or_default();
            let project_name = find_included(&resp.included, "projects", project_id)
                .map(|r| r.attr_str("name").to_string())
                .unwrap_or_default();

            TaskRow {
                id: task.id.clone(),
                number: task.attr_str("number").to_string(),
                title: task.attr_str("title").to_string(),
                status: status_name,
                assignee: assignee_name.trim().to_string(),
                project: project_name,
                updated: task.attr_str("updated_at").to_string(),
            }
        })
        .collect();

    if json {
        println!("{}", output::render_json(&rows));
        return Ok(());
    }

    let total = resp.meta.get("total_count").and_then(|v| v.as_u64());
    if let Some(total) = total {
        eprintln!("{} tasks", total);
    }

    if rows.is_empty() {
        println!("No tasks found.");
        return Ok(());
    }

    println!(
        "{:<8} {:<6} {:<50} {:<18} {:<25} {:<20} {:<12}",
        "ID", "#", "TITLE", "STATUS", "PROJECT", "ASSIGNEE", "UPDATED"
    );
    for row in &rows {
        println!(
            "{:<8} {:<6} {:<50} {:<18} {:<25} {:<20} {:<12}",
            row.id,
            row.number,
            output::truncate(&row.title, 48),
            output::truncate(&row.status, 16),
            output::truncate(&row.project, 23),
            output::truncate(&row.assignee, 18),
            output::relative_time(&row.updated),
        );
    }

    Ok(())
}

fn find_included<'a>(included: &'a [Resource], rtype: &str, id: &str) -> Option<&'a Resource> {
    if id.is_empty() {
        return None;
    }
    included
        .iter()
        .find(|r| r.resource_type == rtype && r.id == id)
}
