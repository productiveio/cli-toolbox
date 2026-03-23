use crate::api::{ProductiveClient, Query};
use crate::json_error;
use crate::schema::ResourceDef;

pub async fn run(client: &ProductiveClient, resource: &ResourceDef, query_text: &str) {
    let search_param = match &resource.search_filter_param {
        Some(p) => p.as_str(),
        None => {
            json_error::exit_with_error(
                "operation_not_supported",
                &format!("{} does not support keyword search.", resource.type_name),
            );
        }
    };

    let query = Query::new().filter(search_param, query_text);
    let path = resource.api_path();
    let page_size = 20;

    match client.get_page(&path, &query, 1, page_size).await {
        Ok(resp) => {
            let total_count = resp.meta.get("total_count").and_then(|v| v.as_u64()).unwrap_or(0);

            let output = serde_json::json!({
                "data": resp.data,
                "meta": {
                    "totalCount": total_count,
                    "query": query_text,
                }
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
        Err(e) => json_error::exit_with_tb_error(&e),
    }
}
