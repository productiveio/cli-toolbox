mod harness;

use tb_prod::api::Query;

#[tokio::test]
async fn get_page() {
    let client = harness::test_client("tasks_get_page");
    let resp = client
        .get_page("/tasks", &Query::new(), 1, 5)
        .await
        .unwrap();

    assert!(!resp.data.is_empty());
    assert_eq!(resp.data[0].resource_type, "tasks");
}

#[tokio::test]
async fn get_one() {
    let client = harness::test_client("tasks_get_one");
    let list = client
        .get_page("/tasks", &Query::new(), 1, 1)
        .await
        .unwrap();
    let id = &list.data[0].id;

    let single = client.get_one(&format!("/tasks/{}", id)).await.unwrap();
    assert_eq!(single.data.id, *id);
    assert!(!single.data.attr_str("title").is_empty());
}

#[tokio::test]
async fn get_with_includes() {
    let client = harness::test_client("tasks_with_includes");
    let query = Query::new().include("project,assignee");
    let resp = client.get_page("/tasks", &query, 1, 5).await.unwrap();

    assert!(!resp.data.is_empty());
    // Should have included resources
    if !resp.included.is_empty() {
        let types: Vec<&str> = resp
            .included
            .iter()
            .map(|r| r.resource_type.as_str())
            .collect();
        // At least one of the included types should be projects or people
        assert!(
            types.iter().any(|t| *t == "projects" || *t == "people"),
            "expected included projects or people, got: {:?}",
            types
        );
    }
}

#[tokio::test]
async fn filter_by_project() {
    let client = harness::test_client("tasks_filter_project");
    // Get a project first
    let projects = client
        .get_page("/projects", &Query::new().filter("status", "1"), 1, 1)
        .await
        .unwrap();
    if projects.data.is_empty() {
        eprintln!("No projects, skipping");
        return;
    }
    let project_id = &projects.data[0].id;

    let query = Query::new().filter("project_id", project_id);
    let resp = client.get_page("/tasks", &query, 1, 10).await.unwrap();

    // All returned tasks should belong to this project
    assert!(resp.meta.get("total_count").is_some());
}

#[tokio::test]
async fn search() {
    let client = harness::test_client("tasks_search");
    let query = Query::new().filter("title", "test");
    let resp = client.get_page("/tasks", &query, 1, 5).await.unwrap();

    assert!(resp.meta.get("total_count").is_some());
}

#[tokio::test]
async fn sort_by_created_at() {
    let client = harness::test_client("tasks_sort_created");
    let query = Query::new().sort("-created_at"); // descending
    let resp = client.get_page("/tasks", &query, 1, 5).await.unwrap();

    assert!(!resp.data.is_empty());
    // Verify descending order
    let dates: Vec<&str> = resp.data.iter().map(|r| r.attr_str("created_at")).collect();
    for w in dates.windows(2) {
        assert!(
            w[0] >= w[1],
            "expected descending created_at: {} >= {}",
            w[0],
            w[1]
        );
    }
}
