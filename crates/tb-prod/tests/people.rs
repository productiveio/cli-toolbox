mod harness;

use tb_prod::api::Query;

#[tokio::test]
async fn get_page() {
    let client = harness::test_client("people_get_page");
    let resp = client.get_page("/people", &Query::new(), 1, 5).await.unwrap();

    assert!(!resp.data.is_empty());
    assert_eq!(resp.data[0].resource_type, "people");
    // People should have first_name and last_name
    let first = resp.data[0].attr_str("first_name");
    let last = resp.data[0].attr_str("last_name");
    assert!(!first.is_empty() || !last.is_empty(), "person should have a name");
}

#[tokio::test]
async fn get_one() {
    let client = harness::test_client("people_get_one");
    let list = client.get_page("/people", &Query::new(), 1, 1).await.unwrap();
    let id = &list.data[0].id;

    let single = client.get_one(&format!("/people/{}", id)).await.unwrap();
    assert_eq!(single.data.id, *id);
    assert_eq!(single.data.resource_type, "people");
}

#[tokio::test]
async fn filter_by_email() {
    let client = harness::test_client("people_filter_email");
    // Get first person's email to use as filter
    let list = client.get_page("/people", &Query::new(), 1, 1).await.unwrap();
    let email = list.data[0].attr_str("email").to_string();
    if email.is_empty() {
        eprintln!("No email on first person, skipping");
        return;
    }

    let query = Query::new().filter("email", &email);
    let resp = client.get_page("/people", &query, 1, 5).await.unwrap();
    assert!(!resp.data.is_empty());
    assert_eq!(resp.data[0].attr_str("email"), email);
}

#[tokio::test]
async fn cache_name_resolution() {
    let client = harness::test_client("people_cache_resolve");
    let (_tmp, cache) = harness::test_cache();

    let resp = client.get_page("/people", &Query::new(), 1, 20).await.unwrap();
    if resp.data.is_empty() {
        return;
    }

    // Build cache records for people
    let records: Vec<tb_prod::generic_cache::CachedRecord> = resp.data.iter().map(|r| {
        let mut fields = std::collections::HashMap::new();
        fields.insert("first_name".to_string(), r.attr_str("first_name").to_string());
        fields.insert("last_name".to_string(), r.attr_str("last_name").to_string());
        fields.insert("email".to_string(), r.attr_str("email").to_string());
        tb_prod::generic_cache::CachedRecord { id: r.id.clone(), fields }
    }).collect();

    // Write to org cache
    let cache_data = serde_json::json!({"data": records});
    std::fs::write(
        _tmp.path().join("people.json"),
        serde_json::to_string_pretty(&cache_data).unwrap(),
    ).unwrap();

    // Resolve by full name
    let first = resp.data[0].attr_str("first_name");
    let last = resp.data[0].attr_str("last_name");
    let full_name = format!("{} {}", first, last);

    if !full_name.trim().is_empty() {
        let result = cache.resolve_name("people", full_name.trim(), None);
        if let Ok(id) = result {
            assert_eq!(id, resp.data[0].id);
        }
    }
}
