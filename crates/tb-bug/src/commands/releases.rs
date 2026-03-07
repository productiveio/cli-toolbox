use crate::api::BugsnagClient;
use crate::config::Config;
use crate::error::Result;
use crate::output;

pub async fn run(
    client: &BugsnagClient,
    config: &Config,
    project: &str,
    limit: usize,
    json: bool,
) -> Result<()> {
    let project_id = config.resolve_project(project)?;
    let resp = client.list_releases(project_id, limit).await?;

    if json {
        println!("{}", output::render_json(&resp.items));
    } else {
        println!(
            "{:<20} {:<12} {:<13} {:>6} {:>6} {:>10}",
            "VERSION", "STAGE", "RELEASED", "NEW", "SEEN", "SESSIONS"
        );
        for r in &resp.items {
            let stage = r.release_stage.as_ref().map(|s| s.name.as_str()).unwrap_or("?");
            let released = r.release_time.as_deref()
                .map(output::relative_time)
                .unwrap_or_else(|| "?".to_string());
            println!(
                "{:<20} {:<12} {:<13} {:>6} {:>6} {:>10}",
                r.app_version,
                stage,
                released,
                r.errors_introduced_count,
                r.errors_seen_count,
                output::fmt_count(r.total_sessions_count.unwrap_or(0)),
            );
        }
    }

    Ok(())
}
