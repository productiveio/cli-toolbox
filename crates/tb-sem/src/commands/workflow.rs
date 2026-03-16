use colored::Colorize;
use serde::Serialize;

use crate::api::{RunWorkflowRequest, SemaphoreClient};
use crate::config::Config;
use crate::error::Result;
use crate::output;

#[derive(Debug, Serialize)]
struct RunOutput {
    workflow_id: String,
    pipeline_id: String,
    branch: String,
    url: String,
}

pub async fn run(
    client: &SemaphoreClient,
    config: &Config,
    project: &str,
    branch: &str,
    commit: Option<&str>,
    pipeline_file: Option<&str>,
    json: bool,
) -> Result<()> {
    let project_id = config.resolve_project(project)?;

    let reference = if branch.starts_with("refs/") {
        branch.to_string()
    } else {
        format!("refs/heads/{}", branch)
    };

    let request = RunWorkflowRequest {
        project_id: project_id.to_string(),
        reference,
        commit_sha: commit.map(String::from),
        pipeline_file: pipeline_file.map(String::from),
    };

    let resp = client.run_workflow(&request).await?;

    let url = format!(
        "https://{}.semaphoreci.com/workflows/{}",
        config.org_id, resp.workflow_id
    );

    let result = RunOutput {
        workflow_id: resp.workflow_id,
        pipeline_id: resp.pipeline_id,
        branch: branch.to_string(),
        url,
    };

    if json {
        println!("{}", output::render_json(&result));
    } else {
        println!(
            "{} Workflow started on branch {}",
            "✓".green().bold(),
            branch.bold()
        );
        println!("  Workflow:  {}", result.workflow_id);
        println!("  Pipeline:  {}", result.pipeline_id);
        println!("  URL:       {}", result.url);
    }

    Ok(())
}
