use crate::api::{ProductiveClient, Query};
use crate::cache::Cache;
use crate::error::Result;

pub async fn run(
    client: &ProductiveClient,
    project_name_or_id: &str,
    json_output: bool,
) -> Result<()> {
    let cache = Cache::new(client.org_id())?;
    cache.ensure_fresh(client).await?;

    let project_id = cache.resolve_project(project_name_or_id)?;

    // Fetch project (with workflow) and task lists in parallel
    let project_path = format!("/projects/{}?include=workflow", project_id);
    let task_lists_q = Query::new()
        .filter_array("project_id", &project_id)
        .filter("status", "1"); // active only

    let (project_resp, task_lists_resp) = tokio::join!(
        client.get_one(&project_path),
        client.get_all("/task_lists", &task_lists_q, 5),
    );

    let project_resp = project_resp?;
    let project = &project_resp.data;
    let project_name = project.attr_str("name");

    // Resolve workflow and its statuses from cache
    let workflow_id = project.relationship_id("workflow");
    let workflow_name = workflow_id
        .and_then(|wid| {
            project_resp
                .included
                .iter()
                .find(|r| r.resource_type == "workflows" && r.id == wid)
                .map(|r| r.attr_str("name").to_string())
        })
        .unwrap_or_default();

    let statuses = cache.workflow_statuses()?;
    let project_statuses: Vec<_> = statuses
        .iter()
        .filter(|s| workflow_id.is_some_and(|wid| s.workflow_id == wid))
        .collect();

    // Task lists
    let task_lists_resp = task_lists_resp?;

    if json_output {
        let out = serde_json::json!({
            "id": project_id,
            "name": project_name,
            "workflow": workflow_name,
            "statuses": project_statuses.iter().map(|s| {
                serde_json::json!({ "id": s.id, "name": s.name, "category_id": s.category_id })
            }).collect::<Vec<_>>(),
            "task_lists": task_lists_resp.data.iter().map(|tl| {
                serde_json::json!({ "id": tl.id, "name": tl.attr_str("name") })
            }).collect::<Vec<_>>(),
        });
        println!("{}", crate::output::render_json(&out));
        return Ok(());
    }

    println!("# {} (ID: {})\n", project_name, project_id);

    if !project_statuses.is_empty() {
        println!("## Statuses ({})", workflow_name);
        for s in &project_statuses {
            let cat = match s.category_id.as_str() {
                "1" => "Not Started",
                "2" => "Started",
                "3" => "Closed",
                _ => &s.category_id,
            };
            println!("- {} (ID: {}, {})", s.name, s.id, cat);
        }
        println!();
    }

    if !task_lists_resp.data.is_empty() {
        println!("## Task Lists");
        for tl in &task_lists_resp.data {
            println!("- {} (ID: {})", tl.attr_str("name"), tl.id);
        }
    }

    Ok(())
}
