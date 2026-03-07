use chrono::{NaiveDate, Utc};

use crate::api::BugsnagClient;
use crate::config::Config;
use crate::error::Result;
use crate::output;

/// Normalize a `--since` value into a format the Bugsnag API accepts.
///
/// Bugsnag accepts: full ISO8601 (`2026-03-07T00:00:00Z`) and relative
/// durations (`1d`, `7d`, `24h`). This function converts human-friendly
/// shortcuts so the user doesn't have to type full ISO8601.
fn parse_since(value: &str) -> String {
    match value {
        "today" => Utc::now().format("%Y-%m-%dT00:00:00Z").to_string(),
        "yesterday" => (Utc::now() - chrono::Duration::days(1))
            .format("%Y-%m-%dT00:00:00Z")
            .to_string(),
        v if NaiveDate::parse_from_str(v, "%Y-%m-%d").is_ok() => {
            format!("{v}T00:00:00Z")
        }
        other => other.to_string(),
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn run(
    client: &BugsnagClient,
    config: &Config,
    project: &str,
    status: Option<&str>,
    severity: Option<&str>,
    since: Option<&str>,
    stage: Option<&str>,
    class: Option<&str>,
    sort: Option<&str>,
    limit: usize,
    json: bool,
    long: bool,
) -> Result<()> {
    let project_id = config.resolve_project(project)?;

    let mut filters = Vec::new();
    if let Some(s) = status {
        filters.push(("error.status", s));
    }
    if let Some(s) = severity {
        filters.push(("event.severity", s));
    }
    let since_value = since.map(parse_since);
    if let Some(ref s) = since_value {
        filters.push(("event.since", s));
    }
    if let Some(s) = stage {
        filters.push(("app.release_stage", s));
    }
    if let Some(c) = class {
        filters.push(("event.class", c));
    }

    let sort_field = sort.unwrap_or("last_seen");
    let resp = client
        .list_errors(project_id, &filters, Some(sort_field), Some("desc"), limit)
        .await?;

    if json {
        println!("{}", output::render_json(&resp.items));
    } else {
        if let Some(total) = resp.total_count {
            println!(
                "Showing {} of {} errors for '{}'\n",
                resp.items.len(), total, project
            );
        }

        println!(
            "{:<8} {:<8} {:>7} {:>6}  {:<13} ERROR CLASS",
            "STATUS", "SEV", "EVENTS", "USERS", "LAST SEEN"
        );
        for e in &resp.items {
            let last_seen = output::relative_time(&e.last_seen);
            let class_display = if long {
                format!("{}\n{:>50}{}", e.error_class, "", e.message)
            } else {
                let class = output::truncate(&e.error_class, 40);
                let msg = if e.message.is_empty() {
                    String::new()
                } else {
                    format!("  {}", output::truncate(&e.message, 40))
                };
                format!("{}{}", class, msg)
            };

            println!(
                "{:<8} {:<8} {:>7} {:>6}  {:<13} {}",
                e.status,
                e.severity,
                output::fmt_count(e.events),
                output::fmt_count(e.users),
                last_seen,
                class_display,
            );
        }
    }

    Ok(())
}
