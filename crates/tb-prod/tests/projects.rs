mod harness;

use tb_prod::api::Query;
use tb_prod::filter::{
    FilterCondition, FilterEntry, FilterGroup, FilterValue, filter_group_to_query,
};

#[tokio::test]
async fn get_page() {
    let client = harness::test_client("projects_get_page");
    let resp = client.get_page("/projects", &Query::new(), 1, 5).await.unwrap();

    assert!(!resp.data.is_empty());
    assert_eq!(resp.data[0].resource_type, "projects");
    assert!(resp.meta.get("total_count").and_then(|v| v.as_u64()).is_some());
}

#[tokio::test]
async fn get_one() {
    let client = harness::test_client("projects_get_one");
    let list = client.get_page("/projects", &Query::new(), 1, 1).await.unwrap();
    let id = &list.data[0].id;

    let single = client.get_one(&format!("/projects/{}", id)).await.unwrap();
    assert_eq!(single.data.id, *id);
    assert_eq!(single.data.resource_type, "projects");
    // Verify attributes are populated
    assert!(!single.data.attr_str("name").is_empty());
}

#[tokio::test]
async fn filter_by_status() {
    let client = harness::test_client("projects_filter_status");
    let query = Query::new().filter("status", "1");
    let resp = client.get_page("/projects", &query, 1, 5).await.unwrap();

    assert!(!resp.data.is_empty());
    for project in &resp.data {
        assert_eq!(project.resource_type, "projects");
    }
}

#[tokio::test]
async fn filter_group() {
    let client = harness::test_client("projects_filter_group");
    let group = FilterGroup {
        op: "and".to_string(),
        conditions: vec![FilterEntry::Condition(FilterCondition {
            field: "status".to_string(),
            operator: "eq".to_string(),
            value: FilterValue::Single("1".to_string()),
        })],
    };
    let query = filter_group_to_query(&group, Query::new());
    let resp = client.get_page("/projects", &query, 1, 5).await.unwrap();

    assert!(!resp.data.is_empty());
}

#[tokio::test]
async fn filter_nested_groups() {
    let client = harness::test_client("projects_filter_nested");
    let group = FilterGroup {
        op: "or".to_string(),
        conditions: vec![
            FilterEntry::Condition(FilterCondition {
                field: "status".to_string(),
                operator: "eq".to_string(),
                value: FilterValue::Single("1".to_string()),
            }),
            FilterEntry::Group(FilterGroup {
                op: "and".to_string(),
                conditions: vec![FilterEntry::Condition(FilterCondition {
                    field: "status".to_string(),
                    operator: "eq".to_string(),
                    value: FilterValue::Single("2".to_string()),
                })],
            }),
        ],
    };
    let query = filter_group_to_query(&group, Query::new());
    let resp = client.get_page("/projects", &query, 1, 5).await.unwrap();

    assert!(resp.meta.get("total_count").is_some());
}

#[tokio::test]
async fn include_company() {
    let client = harness::test_client("projects_include_company");
    let query = Query::new().include("company").filter("status", "1");
    let resp = client.get_page("/projects", &query, 1, 5).await.unwrap();

    assert!(!resp.data.is_empty());
    // At least some projects should have included companies
    // (not guaranteed, but response shape should be valid)
}

#[tokio::test]
async fn search() {
    let client = harness::test_client("projects_search");
    let query = Query::new().filter("query", "test");
    let resp = client.get_page("/projects", &query, 1, 5).await.unwrap();

    assert!(resp.meta.get("total_count").is_some());
}

#[tokio::test]
async fn sort_by_name() {
    let client = harness::test_client("projects_sort_name");
    let query = Query::new().filter("status", "1").sort("name");
    let resp = client.get_page("/projects", &query, 1, 5).await.unwrap();

    assert!(!resp.data.is_empty());
    // Verify sort order
    let names: Vec<&str> = resp.data.iter().map(|r| r.attr_str("name")).collect();
    let mut sorted = names.clone();
    sorted.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    assert_eq!(names, sorted, "projects should be sorted by name");
}

#[tokio::test]
async fn pagination_page_2() {
    let client = harness::test_client("projects_page_2");
    let query = Query::new().filter("status", "1");
    let page1 = client.get_page("/projects", &query, 1, 2).await.unwrap();

    let total = page1.meta.get("total_count").and_then(|v| v.as_u64()).unwrap_or(0);
    if total <= 2 {
        eprintln!("Not enough projects for pagination test, skipping");
        return;
    }

    let page2 = client.get_page("/projects", &query, 2, 2).await.unwrap();
    assert!(!page2.data.is_empty());
    // Page 2 should have different IDs than page 1
    assert_ne!(page1.data[0].id, page2.data[0].id);
}

#[tokio::test]
async fn get_nonexistent_returns_error() {
    let client = harness::test_client("projects_404");
    let result = client.get_one("/projects/999999999").await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("404"), "expected 404, got: {}", err);
}
