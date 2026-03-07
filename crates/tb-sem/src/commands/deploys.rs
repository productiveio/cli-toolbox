use serde::Serialize;

use crate::api::SemaphoreClient;
use crate::config::Config;
use crate::error::Result;
use crate::output;

#[derive(Debug, Serialize)]
pub struct DeploysOutput {
    pub pipeline_window: Option<PipelineWindow>,
    pub deploys: Vec<DeployEntry>,
    pub has_overlap: bool,
}

#[derive(Debug, Serialize)]
pub struct PipelineWindow {
    pub start: String,
    pub end: String,
}

#[derive(Debug, Serialize)]
pub struct DeployEntry {
    pub project: String,
    pub name: String,
    pub start: String,
    pub end: String,
    pub overlaps: bool,
}

pub async fn run(
    client: &SemaphoreClient,
    config: &Config,
    project: &str,
    around: Option<&str>,
    json: bool,
    utc: bool,
) -> Result<()> {
    let tz = if utc { chrono_tz::UTC } else { config.timezone() };
    let (project_id, default_branch) = config.resolve_project(project)?;

    // If --around, get the reference pipeline's time window
    let (ppl_start, ppl_end) = if let Some(ppl_id) = around {
        let ppl = client.get_pipeline(ppl_id, false).await?;
        let start = ppl.created_at_dt();
        let end = ppl.done_at_dt();
        (start, end)
    } else {
        (None, None)
    };

    let workflows = client
        .list_workflows(project_id, Some(default_branch), None, None)
        .await?;

    let mut deploys = Vec::new();
    let mut has_overlap = false;

    for wf in workflows.iter().take(10) {
        let wf_time = wf.created_at.to_datetime();

        // If --around, only check workflows within +-2 hours
        if let Some(start) = ppl_start {
            let diff = (wf_time - start).num_hours().abs();
            if diff > 2 {
                continue;
            }
        }

        let pipelines = client
            .list_pipelines_for_workflow(&wf.wf_id)
            .await
            .unwrap_or_default();

        for p in &pipelines {
            if p.is_promotion() {
                let p_start = p.created_at_dt();
                let p_end = p.done_at_dt();

                let overlaps = match (ppl_start, ppl_end) {
                    (Some(start), Some(end)) => p_start < end && p_end > start,
                    _ => false,
                };

                if overlaps {
                    has_overlap = true;
                }

                deploys.push(DeployEntry {
                    project: project.to_string(),
                    name: p.name.clone(),
                    start: output::epoch_to_local(p.created_at.seconds, &tz),
                    end: output::epoch_to_local(p.done_at.seconds, &tz),
                    overlaps,
                });
            }
        }
    }

    let pipeline_window = match (ppl_start, ppl_end) {
        (Some(s), Some(e)) => Some(PipelineWindow {
            start: s.with_timezone(&tz).format("%Y-%m-%d %H:%M").to_string(),
            end: e.with_timezone(&tz).format("%Y-%m-%d %H:%M").to_string(),
        }),
        _ => None,
    };

    let result = DeploysOutput {
        pipeline_window,
        deploys,
        has_overlap,
    };

    if json {
        println!("{}", output::render_json(&result));
    } else {
        if let Some(ref w) = result.pipeline_window {
            println!("E2E RUN WINDOW: {} -- {}\n", w.start, w.end);
        }

        if result.deploys.is_empty() {
            println!("No deploys found for {} (branch: {})", project, default_branch);
        } else {
            println!("{} DEPLOYS:", project.to_uppercase());
            for d in &result.deploys {
                let status = if d.overlaps { "!! OVERLAP" } else { "no overlap" };
                println!("  {} -- {} {} ({})", d.start, d.end, d.name, status);
            }
        }

        if around.is_some() {
            println!(
                "\nVERDICT: {}",
                if result.has_overlap {
                    "Deploy overlap detected!"
                } else {
                    "No deploy overlap detected."
                }
            );
        }
    }

    Ok(())
}
