use std::collections::BTreeSet;

use crate::api::SemaphoreClient;
use crate::config::Config;
use crate::error::Result;

pub async fn run(client: &SemaphoreClient, config: &Config, project: &str) -> Result<()> {
    let project_id = config.resolve_project(project)?;

    let workflows = client.list_workflows(project_id, None, None, None).await?;

    let branches: BTreeSet<&str> = workflows.iter().map(|wf| wf.branch_name.as_str()).collect();

    for branch in &branches {
        println!("{}", branch);
    }

    Ok(())
}
