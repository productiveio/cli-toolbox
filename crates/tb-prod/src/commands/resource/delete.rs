use crate::api::ProductiveClient;
use crate::json_error;
use crate::schema::ResourceDef;

pub async fn run(client: &ProductiveClient, resource: &ResourceDef, id: &str, confirm: bool) {
    if !resource.supports_action("delete") {
        json_error::exit_with_error(
            "operation_not_supported",
            &format!("{} does not support delete.", resource.type_name),
        );
    }

    if !confirm {
        // Dry run — show what would be deleted
        let output = serde_json::json!({
            "dryRun": true,
            "action": "delete",
            "type": resource.type_name,
            "id": id,
            "message": format!(
                "Would delete {} {}. Use --confirm to execute.",
                resource.item_name, id
            ),
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
        return;
    }

    let path = format!("{}/{}", resource.api_path(), id);
    match client.delete(&path).await {
        Ok(()) => {
            let output = serde_json::json!({
                "deleted": true,
                "type": resource.type_name,
                "id": id,
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
        Err(e) => json_error::exit_with_tb_error(&e),
    }
}
