use serde::Serialize;

use crate::api::{ProductiveClient, Resource};
use crate::error::Result;
use crate::output;

#[derive(Debug, Serialize)]
struct TaskDetail {
    id: String,
    number: String,
    title: String,
    status: String,
    assignee: String,
    project: String,
    creator: String,
    task_list: String,
    due_date: String,
    created_at: String,
    updated_at: String,
    description: String,
    url: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    subtasks: Vec<SubtaskRow>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    todos: Vec<TodoRow>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    comments: Vec<CommentRow>,
}

#[derive(Debug, Serialize)]
struct SubtaskRow {
    id: String,
    number: String,
    title: String,
    status: String,
}

#[derive(Debug, Serialize)]
struct TodoRow {
    id: String,
    description: String,
    done: bool,
}

#[derive(Debug, Serialize)]
struct CommentRow {
    id: String,
    body: String,
    created_at: String,
}

pub async fn run(
    client: &ProductiveClient,
    task_id: &str,
    json: bool,
) -> Result<()> {
    let resp = client.get_task(task_id).await?;
    let task = &resp.data;

    let status_name = resolve_name(&resp.included, "workflow_statuses", task.relationship_id("workflow_status"));
    let assignee_name = resolve_person(&resp.included, task.relationship_id("assignee"));
    let project_name = resolve_name(&resp.included, "projects", task.relationship_id("project"));
    let creator_name = resolve_person(&resp.included, task.relationship_id("creator"));
    let task_list_name = resolve_name(&resp.included, "task_lists", task.relationship_id("task_list"));

    // Fetch subtasks, todos, and comments in parallel
    let (subtasks_resp, todos_resp, comments_resp) = tokio::join!(
        client.get_subtasks(task_id),
        client.get_todos(task_id),
        client.list_comments(task_id),
    );

    let subtasks: Vec<SubtaskRow> = subtasks_resp
        .map(|r| {
            r.data
                .iter()
                .map(|st| {
                    let st_status = resolve_name(&r.included, "workflow_statuses", st.relationship_id("workflow_status"));
                    SubtaskRow {
                        id: st.id.clone(),
                        number: st.attr_str("number").to_string(),
                        title: st.attr_str("title").to_string(),
                        status: st_status,
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let todos: Vec<TodoRow> = todos_resp
        .map(|r| {
            r.data
                .iter()
                .map(|t| TodoRow {
                    id: t.id.clone(),
                    description: t.attr_str("description").to_string(),
                    done: t.attr_bool("closed"),
                })
                .collect()
        })
        .unwrap_or_default();

    let comments: Vec<CommentRow> = comments_resp
        .map(|r| {
            r.data
                .iter()
                .map(|c| CommentRow {
                    id: c.id.clone(),
                    body: strip_html(c.attr_str("body")),
                    created_at: c.attr_str("created_at").to_string(),
                })
                .collect()
        })
        .unwrap_or_default();

    let description_html = task.attr_str("description");
    let description = if description_html.is_empty() {
        String::new()
    } else {
        strip_html(description_html)
    };

    let url = format!(
        "https://app.productive.io/{}/tasks/task/{}",
        client.org_id(),
        task.id
    );

    let detail = TaskDetail {
        id: task.id.clone(),
        number: task.attr_str("number").to_string(),
        title: task.attr_str("title").to_string(),
        status: status_name,
        assignee: assignee_name,
        project: project_name,
        creator: creator_name,
        task_list: task_list_name,
        due_date: task.attr_str("due_date").to_string(),
        created_at: task.attr_str("created_at").to_string(),
        updated_at: task.attr_str("updated_at").to_string(),
        description,
        url,
        subtasks,
        todos,
        comments,
    };

    if json {
        println!("{}", output::render_json(&detail));
        return Ok(());
    }

    println!("#{} — {}", detail.number, detail.title);
    println!("Status:    {}", detail.status);
    println!("Project:   {}", detail.project);
    println!("Task list: {}", detail.task_list);
    println!("Assignee:  {}", detail.assignee);
    println!("Creator:   {}", detail.creator);
    if !detail.due_date.is_empty() {
        println!("Due:       {}", detail.due_date);
    }
    println!("Created:   {}", output::relative_time(&detail.created_at));
    println!("Updated:   {}", output::relative_time(&detail.updated_at));
    println!("URL:       {}", detail.url);

    if !detail.description.is_empty() {
        println!("\n--- Description ---");
        println!("{}", detail.description);
    }

    if !detail.subtasks.is_empty() {
        println!("\n--- Subtasks ({}) ---", detail.subtasks.len());
        for st in &detail.subtasks {
            println!("  #{} [{}] {}", st.number, st.status, st.title);
        }
    }

    if !detail.todos.is_empty() {
        println!("\n--- Todos ({}) ---", detail.todos.len());
        for t in &detail.todos {
            let check = if t.done { "[x]" } else { "[ ]" };
            println!("  {} {} (ID: {})", check, t.description, t.id);
        }
    }

    if !detail.comments.is_empty() {
        println!("\n--- Comments ({}) ---", detail.comments.len());
        for c in &detail.comments {
            println!("  [{}] {}", output::relative_time(&c.created_at), output::truncate(&c.body, 100));
        }
    }

    Ok(())
}

fn resolve_name(included: &[Resource], rtype: &str, id: Option<&str>) -> String {
    let id = match id {
        Some(id) => id,
        None => return String::new(),
    };
    included
        .iter()
        .find(|r| r.resource_type == rtype && r.id == id)
        .map(|r| r.attr_str("name").to_string())
        .unwrap_or_default()
}

fn resolve_person(included: &[Resource], id: Option<&str>) -> String {
    let id = match id {
        Some(id) => id,
        None => return String::new(),
    };
    included
        .iter()
        .find(|r| r.resource_type == "people" && r.id == id)
        .map(|r| {
            format!(
                "{} {}",
                r.attr_str("first_name"),
                r.attr_str("last_name")
            )
            .trim()
            .to_string()
        })
        .unwrap_or_default()
}

fn strip_html(html: &str) -> String {
    html2text::from_read(html.as_bytes(), 80)
        .unwrap_or_else(|_| html.to_string())
        .trim()
        .to_string()
}
