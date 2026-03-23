pub mod bookings;
pub mod deals;
pub mod notifications;
pub mod pages;
pub mod people;
pub mod scenarios;
pub mod service_types;
pub mod services;
pub mod slack;
pub mod tasks;

use serde_json::Value;

use crate::api::ProductiveClient;

/// Result of an extension action execution.
pub enum ExtensionResult {
    /// JSON output to print
    Json(Value),
}

/// Return known extension action names for a resource type.
pub fn action_names(resource_type: &str) -> Vec<&'static str> {
    match resource_type {
        "tasks" => vec!["load_activity", "resolve_subscriber_ids"],
        "deals" => vec!["load_activity"],
        "notifications" => vec!["load_details"],
        "bookings" => vec!["find_conflicts", "capacity_availability"],
        "pages" => vec!["update_body"],
        "services" => vec!["move"],
        "service_types" => vec!["merge"],
        "people" => vec!["invite"],
        "slack_messages" => vec!["send"],
        "scenarios" => vec!["copy"],
        _ => vec![],
    }
}

/// Try to dispatch an extension action. Returns None if no extension handles it.
pub async fn dispatch(
    client: &ProductiveClient,
    resource_type: &str,
    id: &str,
    action_name: &str,
    data: Option<&Value>,
) -> Option<Result<ExtensionResult, String>> {
    match resource_type {
        "tasks" => tasks::dispatch(client, id, action_name, data).await,
        "deals" => deals::dispatch(client, id, action_name, data).await,
        "notifications" => notifications::dispatch(client, id, action_name, data).await,
        "pages" => pages::dispatch(client, id, action_name, data).await,
        "bookings" => bookings::dispatch(client, id, action_name, data).await,
        "services" => services::dispatch(client, id, action_name, data).await,
        "service_types" => service_types::dispatch(client, id, action_name, data).await,
        "people" => people::dispatch(client, id, action_name, data).await,
        "slack_messages" => slack::dispatch(client, id, action_name, data).await,
        "scenarios" => scenarios::dispatch(client, id, action_name, data).await,
        _ => None,
    }
}
