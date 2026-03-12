use toolbox_core::time_range::TimeRange;

use crate::api::BugsnagClient;
use crate::config::Config;
use crate::error::Result;
use crate::output;

#[allow(clippy::too_many_arguments)]
pub async fn run(
    client: &BugsnagClient,
    config: &Config,
    project: &str,
    status: Option<&str>,
    severity: Option<&str>,
    time: &TimeRange,
    stage: Option<&str>,
    class: Option<&str>,
    sort: Option<&str>,
    limit: usize,
    json: bool,
    long: bool,
) -> Result<()> {
    let project_id = config.resolve_project(project)?;

    let range = time.resolve_or_exit();
    let (from_iso, to_iso) = range.to_iso8601();

    let mut filters = Vec::new();
    if let Some(s) = status {
        filters.push(("error.status", s));
    }
    if let Some(s) = severity {
        filters.push(("event.severity", s));
    }
    if let Some(ref s) = from_iso {
        filters.push(("event.since", s));
    }
    if let Some(ref s) = to_iso {
        filters.push(("event.before", s));
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
                resp.items.len(),
                total,
                project
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
