use serde_json::{Value, json};

use crate::api::ProductiveClient;
use crate::prosemirror;

use super::ExtensionResult;

pub async fn dispatch(
    client: &ProductiveClient,
    id: &str,
    action_name: &str,
    data: Option<&Value>,
) -> Option<Result<ExtensionResult, String>> {
    match action_name {
        "update_body" => Some(update_body(client, id, data).await),
        _ => None,
    }
}

async fn update_body(
    client: &ProductiveClient,
    page_id: &str,
    data: Option<&Value>,
) -> Result<ExtensionResult, String> {
    let body_markdown = data
        .and_then(|d| d.get("body"))
        .and_then(|v| v.as_str())
        .ok_or("Missing 'body' (markdown string) in action data.")?;

    // Convert markdown to ProseMirror JSON
    let prosemirror_json = prosemirror::markdown_to_prosemirror_json(body_markdown);

    // PATCH the page body through the standard API
    // Note: ai-agent uses docs-realtime for this, but the CLI uses the simpler
    // API path. This may hit version_number conflicts if the page is being
    // edited concurrently, but it's acceptable for CLI usage.
    let payload = json!({
        "data": {
            "type": "pages",
            "id": page_id,
            "attributes": {
                "body": prosemirror_json
            }
        }
    });

    let path = format!("/pages/{}", page_id);
    client
        .update(&path, &payload)
        .await
        .map_err(|e| e.to_string())?;

    Ok(ExtensionResult::Json(json!({
        "success": true,
        "action": "update_body",
        "pageId": page_id,
        "bodyLength": body_markdown.len(),
    })))
}
