use serde_json::json;

use crate::api::ProductiveClient;
use crate::error::Result;

pub async fn run(
    client: &ProductiveClient,
    task_id: &str,
    workflow_status_id: &str,
    json_output: bool,
) -> Result<()> {
    let payload = json!({
        "data": {
            "type": "tasks",
            "id": task_id,
            "relationships": {
                "workflow_status": {
                    "data": { "type": "workflow_statuses", "id": workflow_status_id }
                }
            }
        }
    });

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
