mod harness;

use tb_prod::api::Query;

#[tokio::test]
async fn create_and_delete_root_page() {
    let client = harness::test_client("pages_create_delete");

    // Create a root page (doc)
    let body = serde_json::json!({
        "data": {
            "type": "pages",
            "attributes": {
                "title": "Integration Test Doc",
                "body": {"type": "doc", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "Hello from integration test"}]}]}
            }
        }
    });
    let created = client.create("/pages", &body).await.unwrap();
    let page_id = created.data.id.clone();

    assert_eq!(created.data.resource_type, "pages");
    assert_eq!(created.data.attr_str("title"), "Integration Test Doc");

    // Clean up
    client.delete(&format!("/pages/{}", page_id)).await.unwrap();
}

#[tokio::test]
async fn create_child_page() {
    let client = harness::test_client("pages_create_child");

    // Create parent
    let parent_body = serde_json::json!({
        "data": {
            "type": "pages",
            "attributes": {
                "title": "Parent Doc"
            }
        }
    });
    let parent = client.create("/pages", &parent_body).await.unwrap();
    let parent_id = parent.data.id.clone();

    // Create child (needs both rootPage and parentPage)
    let child_body = serde_json::json!({
        "data": {
            "type": "pages",
            "attributes": {
                "title": "Child Page"
            },
            "relationships": {
                "root_page": { "data": { "type": "pages", "id": &parent_id } },
                "parent_page": { "data": { "type": "pages", "id": &parent_id } }
            }
        }
    });
    let child = client.create("/pages", &child_body).await.unwrap();
    let child_id = child.data.id.clone();

    assert_eq!(child.data.attr_str("title"), "Child Page");

    // Clean up (child first, then parent)
    client
        .delete(&format!("/pages/{}", child_id))
        .await
        .unwrap();
    client
        .delete(&format!("/pages/{}", parent_id))
        .await
        .unwrap();
}

#[tokio::test]
async fn update_title() {
    let client = harness::test_client("pages_update_title");

    // Create
    let body = serde_json::json!({
        "data": {
            "type": "pages",
            "attributes": { "title": "Original Title" }
        }
    });
    let created = client.create("/pages", &body).await.unwrap();
    let page_id = created.data.id.clone();

    // Update title
    let update = serde_json::json!({
        "data": {
            "type": "pages",
            "id": &page_id,
            "attributes": { "title": "Updated Title" }
        }
    });
    let updated = client
        .update(&format!("/pages/{}", page_id), &update)
        .await
        .unwrap();
    assert_eq!(updated.data.attr_str("title"), "Updated Title");

    // Clean up
    client.delete(&format!("/pages/{}", page_id)).await.unwrap();
}

#[tokio::test]
async fn update_body_via_extension() {
    let client = harness::test_client("pages_update_body");

    // Create a page
    let body = serde_json::json!({
        "data": {
            "type": "pages",
            "attributes": { "title": "Body Update Test" }
        }
    });
    let created = client.create("/pages", &body).await.unwrap();
    let page_id = created.data.id.clone();

    // Update body using markdown → prosemirror conversion
    let markdown = "# Hello\n\nThis is a **bold** paragraph.\n\n- Item 1\n- Item 2\n";
    let prosemirror_json = tb_prod::prosemirror::markdown_to_prosemirror_json(markdown);
    let prosemirror_body: serde_json::Value = serde_json::from_str(&prosemirror_json).unwrap();

    let update = serde_json::json!({
        "data": {
            "type": "pages",
            "id": &page_id,
            "attributes": {
                "body": prosemirror_body
            }
        }
    });
    let updated = client
        .update(&format!("/pages/{}", page_id), &update)
        .await
        .unwrap();
    assert_eq!(updated.data.id, page_id);

    // Clean up
    client.delete(&format!("/pages/{}", page_id)).await.unwrap();
}

#[tokio::test]
async fn query_with_filter() {
    let client = harness::test_client("pages_query_filter");

    // Create a page so we have something to find
    let body = serde_json::json!({
        "data": {
            "type": "pages",
            "attributes": { "title": "Filterable Test Page" }
        }
    });
    let created = client.create("/pages", &body).await.unwrap();
    let page_id = created.data.id.clone();

    // Query root pages (no parent)
    let query = Query::new();
    let resp = client.get_page("/pages", &query, 1, 20).await.unwrap();
    assert!(!resp.data.is_empty());
    assert!(resp.data.iter().all(|r| r.resource_type == "pages"));

    // Clean up
    client.delete(&format!("/pages/{}", page_id)).await.unwrap();
}

#[tokio::test]
async fn get_one_with_details() {
    let client = harness::test_client("pages_get_one");

    // Create
    let body = serde_json::json!({
        "data": {
            "type": "pages",
            "attributes": { "title": "Detail Fetch Test" }
        }
    });
    let created = client.create("/pages", &body).await.unwrap();
    let page_id = created.data.id.clone();

    // Fetch with details
    let single = client
        .get_one(&format!("/pages/{}", page_id))
        .await
        .unwrap();
    assert_eq!(single.data.id, page_id);
    assert_eq!(single.data.attr_str("title"), "Detail Fetch Test");

    // Clean up
    client.delete(&format!("/pages/{}", page_id)).await.unwrap();
}
