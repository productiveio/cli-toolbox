use crate::api::BugsnagClient;
use crate::config::Config;
use crate::error::Result;
use crate::output;

pub async fn run_error(
    client: &BugsnagClient,
    config: &Config,
    project: &str,
    error_id: &str,
) -> Result<()> {
    let project_id = config.resolve_project(project)?;
    let detail: serde_json::Value = client.get_error_detail(project_id, error_id).await?;
    println!("{}", output::render_json(&detail));
    Ok(())
}

pub async fn run_event(
    client: &BugsnagClient,
    config: &Config,
    project: &str,
    event_id: &str,
) -> Result<()> {
    let project_id = config.resolve_project(project)?;
    let detail: serde_json::Value = client.get_event_detail(project_id, event_id).await?;
    println!("{}", output::render_json(&detail));
    Ok(())
}
