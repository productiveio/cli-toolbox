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
        println!(
            "Would delete {} {}. Use --confirm to execute.",
            resource.item_name, id
        );
        return;
    }

    let path = format!("{}/{}", resource.api_path(), id);
    match client.delete(&path).await {
        Ok(()) => {
            println!("Deleted {} {}", resource.item_name, id);
        }
        Err(e) => json_error::exit_with_tb_error(&e),
    }
}
