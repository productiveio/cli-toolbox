use crate::api::BugsnagClient;
use crate::config::Config;
use crate::error::Result;
use crate::output;

pub async fn run(
    client: &BugsnagClient,
    config: &Config,
    project: &str,
    json: bool,
) -> Result<()> {
    let project_id = config.resolve_project(project)?;
    let trend: serde_json::Value = client.get_trends(project_id, 30).await?;

    if json {
        println!("{}", output::render_json(&trend));
    } else if let Some(buckets) = trend.as_array() {
            println!("Trend: {} ({} buckets)\n", project, buckets.len());
            println!(
                "{:<16} {:<16} {:>10}",
                "FROM", "TO", "EVENTS"
            );
            for b in buckets {
                let from = b["from"].as_str().unwrap_or("?");
                let to = b["to"].as_str().unwrap_or("?");
                let events = b["events_count"].as_u64().unwrap_or(0);
                println!(
                    "{:<16} {:<16} {:>10}",
                    output::relative_time(from),
                    output::relative_time(to),
                    output::fmt_count(events),
                );
            }
    } else {
        println!("{}", output::render_json(&trend));
    }

    Ok(())
}
