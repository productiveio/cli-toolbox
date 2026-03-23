use crate::api::ProductiveClient;
use crate::json_error;
use crate::schema::ResourceDef;

pub async fn run(client: &ProductiveClient, resource: &ResourceDef, id: &str, include: Option<&str>) {
    if !resource.supports_action("show") {
        json_error::exit_with_error(
            "operation_not_supported",
            &format!("{} does not support show.", resource.type_name),
        );
    }

    let mut path = format!("{}/{}", resource.api_path(), id);
    if let Some(includes) = include {
        path = format!("{}?include={}", path, includes);
    }

    match client.get_one(&path).await {
        Ok(resp) => {
            let output = serde_json::json!({
                "data": resp.data,
                "included": resp.included,
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
        Err(e) => json_error::exit_with_tb_error(&e),
    }
}
