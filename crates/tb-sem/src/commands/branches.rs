use std::collections::BTreeSet;

use crate::api::SemaphoreClient;
use crate::config::Config;
use crate::error::Result;

pub async fn run(
    client: &SemaphoreClient,
    config: &Config,
    project: &str,
    days: u32,
) -> Result<()> {
    let project_id = config.resolve_project(project)?;
    let created_after = crate::output::days_ago(days);

    let workflows = client
        .list_workflows(project_id, None, Some(created_after), None)
        .await?;

    let branches: BTreeSet<&str> = workflows.iter().map(|wf| wf.branch_name.as_str()).collect();

    for branch in &branches {
        println!("{}", branch);
    }

    Ok(())
}
