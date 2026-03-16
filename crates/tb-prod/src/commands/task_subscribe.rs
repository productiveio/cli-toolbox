use serde_json::json;

use crate::api::ProductiveClient;
use crate::cache::Cache;
use crate::error::Result;

pub async fn subscribe(
    client: &ProductiveClient,
    cache: &Cache,
    task_id: &str,
    people_args: &[String],
    json: bool,
) -> Result<()> {
    let new_ids: Vec<String> = people_args
        .iter()
        .map(|a| cache.resolve_person(a))
        .collect::<Result<_>>()?;

    let resp = client.get_task(task_id).await?;
    let mut merged: Vec<String> = resp
        .data
        .relationship_ids("subscribers")
        .into_iter()
        .map(|s| s.to_string())
        .collect();

    let mut added = Vec::new();
    for id in &new_ids {
        if !merged.contains(id) {
            merged.push(id.clone());
            added.push(id.clone());
        }
    }

    if added.is_empty() {
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "task_id": task_id,
                    "added": [],
                    "subscribers": format_people(cache, &merged),
                }))?
            );
        } else {
            eprintln!("All specified people are already subscribed.");
            print_subscriber_list(cache, &merged);
        }
        return Ok(());
    }

    patch_subscribers(client, task_id, &merged).await?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "task_id": task_id,
                "added": format_people(cache, &added),
                "subscribers": format_people(cache, &merged),
            }))?
        );
    } else {
        eprintln!("Added: {}", format_people(cache, &added).join(", "));
        print_subscriber_list(cache, &merged);
    }

    Ok(())
}

pub async fn unsubscribe(
    client: &ProductiveClient,
    cache: &Cache,
    task_id: &str,
    people_args: &[String],
    json: bool,
) -> Result<()> {
    let remove_ids: Vec<String> = people_args
        .iter()
        .map(|a| cache.resolve_person(a))
        .collect::<Result<_>>()?;

    let resp = client.get_task(task_id).await?;
    let existing_ids: Vec<String> = resp
        .data
        .relationship_ids("subscribers")
        .into_iter()
        .map(|s| s.to_string())
        .collect();

    let mut removed = Vec::new();
    let remaining: Vec<String> = existing_ids
        .iter()
        .filter(|id| {
            if remove_ids.contains(id) {
                removed.push((*id).clone());
                false
            } else {
                true
            }
        })
        .cloned()
        .collect();

    if removed.is_empty() {
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "task_id": task_id,
                    "removed": [],
                    "subscribers": format_people(cache, &existing_ids),
                }))?
            );
        } else {
            eprintln!("None of the specified people were subscribed.");
            print_subscriber_list(cache, &existing_ids);
        }
        return Ok(());
    }

    patch_subscribers(client, task_id, &remaining).await?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "task_id": task_id,
                "removed": format_people(cache, &removed),
                "subscribers": format_people(cache, &remaining),
            }))?
        );
    } else {
        eprintln!("Removed: {}", format_people(cache, &removed).join(", "));
        print_subscriber_list(cache, &remaining);
    }

    Ok(())
}

async fn patch_subscribers(
    client: &ProductiveClient,
    task_id: &str,
    subscriber_ids: &[String],
) -> Result<()> {
    let payload = json!({
        "data": {
            "type": "tasks",
            "id": task_id,
            "relationships": {
                "subscribers": {
                    "data": subscriber_ids.iter()
                        .map(|id| json!({ "type": "people", "id": id }))
                        .collect::<Vec<_>>()
                }
            }
        }
    });
    client.update_task(task_id, &payload).await?;
    Ok(())
}

fn format_people(cache: &Cache, ids: &[String]) -> Vec<String> {
    let people = cache.people().unwrap_or_default();
    ids.iter()
        .map(|id| {
            people
                .iter()
                .find(|p| p.id == *id)
                .map(|p| p.display_name())
                .unwrap_or_else(|| format!("(ID: {})", id))
        })
        .collect()
}

fn print_subscriber_list(cache: &Cache, ids: &[String]) {
    if ids.is_empty() {
        println!("Subscribers: (none)");
    } else {
        let names = format_people(cache, ids);
        println!("Subscribers: {} ({})", names.join(", "), names.len());
    }
}
