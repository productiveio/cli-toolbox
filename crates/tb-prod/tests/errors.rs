mod harness;

use tb_prod::api::Query;

#[tokio::test]
async fn get_nonexistent_resource_returns_404() {
    let client = harness::test_client("errors_404");
    let result = client.get_one("/projects/999999999").await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("404"), "expected 404, got: {}", err);
}

#[tokio::test]
async fn create_with_invalid_data_returns_422() {
    let client = harness::test_client("errors_422");
    // Create a task without required fields
    let body = serde_json::json!({
        "data": {
            "type": "tasks",
            "attributes": {}
        }
    });
    let result = client.create("/tasks", &body).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("422"), "expected 422, got: {}", err);
}

#[tokio::test]
async fn delete_nonexistent_returns_error() {
    let client = harness::test_client("errors_delete_404");
    let result = client.delete("/comments/999999999").await;

    assert!(result.is_err());
}

#[tokio::test]
async fn get_all_with_pagination() {
    let client = harness::test_client("errors_get_all");
    // get_all fetches multiple pages — test with a small max_pages
    let query = Query::new().filter("status", "1");
    let resp = client.get_all("/projects", &query, 2).await.unwrap();

    assert!(!resp.data.is_empty());
    // Meta should reflect the last page fetched
    assert!(resp.meta.get("total_count").is_some());
}
