use serde::Serialize;

use crate::api::SemaphoreClient;
use crate::config::Config;
use crate::error::Result;
use crate::output;

#[derive(Debug, Serialize)]
pub struct PipelineOutput {
    pub pipeline_id: String,
    pub name: String,
    pub branch: String,
    pub commit_sha: String,
    pub started: String,
    pub finished: Option<String>,
    pub duration: Option<String>,
    pub result: String,
    pub result_reason: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub jobs: Vec<JobEntry>,
}

#[derive(Debug, Serialize)]
pub struct JobEntry {
    pub name: String,
    pub result: String,
    pub job_id: String,
}

pub async fn run(
    client: &SemaphoreClient,
    config: &Config,
    pipeline_id: &str,
    show_jobs: bool,
    json: bool,
    utc: bool,
) -> Result<()> {
    let tz = if utc {
        chrono_tz::UTC
    } else {
        config.timezone()?
    };
    let ppl = client.get_pipeline(pipeline_id, true).await?;

    let started = output::iso_to_local(&ppl.created_at, &tz);
    let finished = ppl.done_at.as_deref().map(|d| output::iso_to_local(d, &tz));
    let duration = match (&ppl.running_at, &ppl.done_at) {
        (Some(s), Some(e)) => Some(output::duration_str(s, e)),
        _ => None,
    };

    let jobs: Vec<JobEntry> = if show_jobs {
        ppl.blocks
            .iter()
            .flat_map(|b| &b.jobs)
            .map(|j| JobEntry {
                name: j.name.clone(),
                result: j.result.clone(),
                job_id: j.job_id.clone(),
            })
            .collect()
    } else {
        Vec::new()
    };

    let result = PipelineOutput {
        pipeline_id: ppl.ppl_id.clone(),
        name: ppl.name.clone(),
        branch: ppl.branch_name.clone(),
        commit_sha: ppl.commit_sha.clone(),
        started,
        finished,
        duration,
        result: ppl.result.to_uppercase(),
        result_reason: ppl.result_reason.clone(),
        jobs,
    };

    if json {
        println!("{}", output::render_json(&result));
    } else {
        println!(
            "Pipeline: {} | {} | {}",
            result.name,
            result.branch,
            &result.commit_sha[..7.min(result.commit_sha.len())]
        );
        println!("Started:  {}", result.started);
        if let (Some(fin), Some(dur)) = (&result.finished, &result.duration) {
            println!("Finished: {} ({})", fin, dur);
        }
        println!("Result:   {} ({})", result.result, result.result_reason);

        if !result.jobs.is_empty() {
            println!("\nJOBS:");
            for j in &result.jobs {
                println!("  {:<24} {:<8}  {}", j.name, j.result, &j.job_id);
            }
        }
    }

    Ok(())
}
