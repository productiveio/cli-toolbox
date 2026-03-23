use crate::api::{ProductiveClient, Query};
use crate::filter::{self, FilterInput};
use crate::generic_cache::GenericCache;
use crate::json_error;
use crate::schema::ResourceDef;

pub async fn run(
    client: &ProductiveClient,
    resource: &ResourceDef,
    filter_json: Option<&str>,
    sort: Option<&str>,
    page: Option<usize>,
    include: Option<&str>,
) {
    if !resource.supports_action("index") {
        json_error::exit_with_error(
            "operation_not_supported",
            &format!("{} does not support querying.", resource.type_name),
        );
    }

    let schema = crate::schema::schema();

    // Build query
    let mut query = Query::new();

    // Parse and validate filter
    if let Some(filter_str) = filter_json {
        let input: FilterInput = match serde_json::from_str(filter_str) {
            Ok(f) => f,
            Err(e) => {
                json_error::exit_with_error("invalid_json", &format!("Invalid filter JSON: {e}"));
            }
        };

        let mut group = filter::normalize_filter(input);

        // Validate
        let errors = filter::validate_filter_group(&group, resource, schema);
        if !errors.is_empty() {
            json_error::exit_with_error_details(
                "invalid_filter",
                &errors[0],
                Some(serde_json::json!({"errors": errors})),
            );
        }

        // Name resolution: resolve names to IDs via cache
        if let Ok(cache) = GenericCache::new(client.org_id())
            && let Err(e) = crate::generic_cache::resolve_filter_names(
                &cache,
                &mut group.conditions,
                resource,
                schema,
            ) {
                json_error::exit_with_error("name_resolution_error", &e);
            }

        // Serialize to query params
        query = filter::filter_group_to_query(&group, query);
    }

    // Sort
    if let Some(sort_field) = sort {
        query = query.sort(sort_field);
    } else if let Some(default) = &resource.default_sort {
        query = query.sort(default);
    }

    // Include
    if let Some(includes) = include {
        query = query.include(includes);
    }

    // Pagination
    let page_num = page.unwrap_or(1);
    let page_size = 20;

    let path = resource.api_path();
    match client.get_page(&path, &query, page_num, page_size).await {
        Ok(resp) => {
            let total_count = resp
                .meta
                .get("total_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let total_pages = resp
                .meta
                .get("total_pages")
                .and_then(|v| v.as_u64())
                .unwrap_or(1);

            let output = serde_json::json!({
                "data": resp.data,
                "included": resp.included,
                "meta": {
                    "totalCount": total_count,
                    "totalPages": total_pages,
                    "currentPage": page_num,
                    "hasNextPage": (page_num as u64) < total_pages,
                }
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        }
        Err(e) => json_error::exit_with_tb_error(&e),
    }
}
