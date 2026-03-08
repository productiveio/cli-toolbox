use crate::api::BugsnagClient;
use crate::config::Config;
use crate::error::Result;
use crate::output;

pub async fn run(
    client: &BugsnagClient,
    config: &Config,
    project: &str,
    error_id: &str,
    limit: usize,
    json: bool,
) -> Result<()> {
    let project_id = config.resolve_project(project)?;
    let events = client.list_events(project_id, error_id, limit).await?;

    if json {
        println!("{}", output::render_json(&events));
    } else {
        println!(
            "{:<13} {:<8} {:<9} {:<12} {:<12} CONTEXT",
            "RECEIVED", "SEV", "UNHANDLED", "VERSION", "STAGE"
        );
        for e in &events {
            let version = e
                .app
                .as_ref()
                .and_then(|a| a.version.as_deref())
                .unwrap_or("");
            let stage = e
                .app
                .as_ref()
                .and_then(|a| a.release_stage.as_deref())
                .unwrap_or("");
            println!(
                "{:<13} {:<8} {:<9} {:<12} {:<12} {}",
                output::relative_time(&e.received_at),
                e.severity,
                if e.unhandled { "yes" } else { "no" },
                version,
                stage,
                e.context,
            );
        }
    }

    Ok(())
}
