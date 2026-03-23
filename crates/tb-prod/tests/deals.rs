mod harness;

use tb_prod::api::Query;

#[tokio::test]
async fn get_page() {
    let client = harness::test_client("deals_get_page");
    let resp = client.get_page("/deals", &Query::new(), 1, 5).await.unwrap();

    assert!(!resp.data.is_empty());
    assert_eq!(resp.data[0].resource_type, "deals");
}

#[tokio::test]
async fn get_one() {
    let client = harness::test_client("deals_get_one");
    let list = client.get_page("/deals", &Query::new(), 1, 1).await.unwrap();
    if list.data.is_empty() {
        return;
    }
    let id = &list.data[0].id;

    let single = client.get_one(&format!("/deals/{}", id)).await.unwrap();
    assert_eq!(single.data.id, *id);
    assert_eq!(single.data.resource_type, "deals");
}

#[tokio::test]
async fn filter_by_status() {
    let client = harness::test_client("deals_filter_status");
    let query = Query::new().filter("status", "1");
    let resp = client.get_page("/deals", &query, 1, 5).await.unwrap();

    assert!(resp.meta.get("total_count").is_some());
}

#[tokio::test]
async fn query_services_for_deal() {
    let client = harness::test_client("deals_query_services");
    let deals = client.get_page("/deals", &Query::new(), 1, 1).await.unwrap();
    if deals.data.is_empty() {
        return;
    }
    let deal_id = &deals.data[0].id;

    let query = Query::new().filter("deal_id", deal_id);
    let services = client.get_page("/services", &query, 1, 10).await.unwrap();

    assert!(services.meta.get("total_count").is_some());
    for svc in &services.data {
        assert_eq!(svc.resource_type, "services");
    }
}
