use serde_json::json;

use crate::api::ProductiveClient;
use crate::error::Result;

pub async fn run(
    client: &ProductiveClient,
    task_id: &str,
    body: &str,
    json_output: bool,
) -> Result<()> {
    let payload = json!({
        "data": {
            "type": "comments",
            "attributes": {
                "body": body
            },
            "relationships": {
                "task": {
                    "data": { "type": "tasks", "id": task_id }
                }
            }
        }
    });

    let resp = client.create_comment(&payload).await?;
    let comment = &resp.data;

    if json_output {
        let out = json!({
            "id": comment.id,
            "task_id": task_id,
            "status": "created"
        });
        println!("{}", crate::output::render_json(&out));
    } else {
        println!("Comment added (ID: {})", comment.id);
    }

    Ok(())
}
