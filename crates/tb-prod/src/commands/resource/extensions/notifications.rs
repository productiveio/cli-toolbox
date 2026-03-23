use serde_json::{json, Value};

use crate::api::{ProductiveClient, Query};

use super::ExtensionResult;

pub async fn dispatch(
    client: &ProductiveClient,
    id: &str,
    action_name: &str,
    _data: Option<&Value>,
) -> Option<Result<ExtensionResult, String>> {
    match action_name {
        "load_details" => Some(load_details(client, id).await),
        _ => None,
    }
}

async fn load_details(client: &ProductiveClient, notification_id: &str) -> Result<ExtensionResult, String> {
    let path = format!("/notifications/{}", notification_id);
    let resp = client.get_one(&path).await.map_err(|e| e.to_string())?;

    let target_type = resp.data.attr_str("target_type").to_string();
    let target_id = resp.data.attr_str("target_id").to_string();

    // Resolve target resource type
    let target_resource = match target_type.as_str() {
        "Task" => Some("tasks"),
        "Deal" => Some("deals"),
        "Page" => Some("pages"),
        "Company" => Some("companies"),
        "Person" => Some("people"),
        "Project" => Some("projects"),
        "Booking" => Some("bookings"),
        "Invoice" => Some("invoices"),
        _ => None,
    };

    // Fetch target if resolvable
    let target = if let Some(resource_type) = target_resource {
        if !target_id.is_empty() {
            let target_path = format!("/{}/{}", resource_type, target_id);
            client.get_one(&target_path).await.ok().map(|r| r.data)
        } else {
            None
        }
    } else {
        None
    };

    // Fetch new activities if available
    let new_count = resp.data.attributes.get("new_activities_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let first_unread = resp.data.attr_str("first_unread_activity_id").to_string();

    let activities = if new_count > 0 && !first_unread.is_empty() && !target_id.is_empty() {
        let filter_key = format!("{}_id", target_type.to_lowercase());
        let query = Query::new()
            .filter_array(&filter_key, &target_id)
            .filter_indexed(0, "id", "gt_eq", &first_unread)
            .filter_op("and");
        client.get_page("/activities", &query, 1, 200).await.ok()
    } else {
        None
    };

    let output = json!({
        "notification": resp.data,
        "target": target,
        "targetType": target_type,
        "activities": activities.as_ref().map(|a| &a.data),
        "summary": {
            "newActivitiesCount": new_count,
            "activityCount": activities.as_ref().map(|a| a.data.len()).unwrap_or(0),
            "targetType": target_type,
            "targetResourceType": target_resource,
        }
    });

    Ok(ExtensionResult::Json(output))
}
