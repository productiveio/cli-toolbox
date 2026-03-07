use serde::Serialize;

use crate::api::SemaphoreClient;
use crate::config::Config;
use crate::error::Result;
use crate::output;

#[derive(Debug, Serialize)]
pub struct RunsOutput {
    pub runs: Vec<RunEntry>,
}

#[derive(Debug, Serialize)]
pub struct RunEntry {
    pub time: String,
    pub duration: String,
    pub result: String,
    pub pipeline_id: String,
    pub commit_sha: String,
    pub commit_message: String,
}

#[allow(clippy::too_many_arguments)]
pub async fn run(
    client: &SemaphoreClient,
    config: &Config,
    project: &str,
    branch: Option<&str>,
    failed_only: bool,
    limit: usize,
    json: bool,
    utc: bool,
    after: Option<i64>,
    before: Option<i64>,
) -> Result<()> {
    let (project_id, default_branch) = config.resolve_project(project)?;
    let branch = branch.or(Some(default_branch));
    let tz = if utc { chrono_tz::UTC } else { config.timezone() };

    let workflows = client.list_workflows(project_id, branch, after, before).await?;

    let mut runs = Vec::new();
    for wf in &workflows {
        if runs.len() >= limit {
            break;
        }

        let ppl = client.get_pipeline(&wf.initial_ppl_id, false).await?;

        if failed_only && ppl.result_normalized() != "failed" {
            continue;
        }

        let time = output::epoch_to_local(wf.created_at.seconds, &tz);
        let dur = match (&ppl.running_at, &ppl.done_at) {
            (Some(start), Some(end)) => output::duration_str(start, end),
            _ => "?".to_string(),
        };
        let commit_short = &wf.commit_sha[..7.min(wf.commit_sha.len())];
        let commit_msg = ppl.commit_message.lines().next().unwrap_or("");
        let commit_msg_short = if commit_msg.len() > 50 {
            format!("{}...", &commit_msg[..47])
        } else {
            commit_msg.to_string()
        };

        runs.push(RunEntry {
            time,
            duration: dur,
            result: ppl.result.to_uppercase(),
            pipeline_id: ppl.ppl_id.clone(),
            commit_sha: commit_short.to_string(),
            commit_message: commit_msg_short,
        });
    }

    let result = RunsOutput { runs };

    if json {
        println!("{}", output::render_json(&result));
    } else {
        println!(
            "{:<20} {:<8} {:<8} {:<38} COMMIT",
            "TIME", "DUR", "RESULT", "PIPELINE"
        );
        for r in &result.runs {
            println!(
                "{:<20} {:<8} {:<8} {:<38} {} {}",
                r.time,
                r.duration,
                r.result,
                r.pipeline_id,
                r.commit_sha,
                r.commit_message,
            );
        }
    }

    Ok(())
}
