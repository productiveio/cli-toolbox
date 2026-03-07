use serde::Serialize;

use crate::api::SemaphoreClient;
use crate::config::Config;
use crate::error::Result;
use crate::output;

#[derive(Debug, Serialize)]
pub struct PromotionsOutput {
    pub pipeline_id: String,
    pub promotions: Vec<PromotionEntry>,
}

#[derive(Debug, Serialize)]
pub struct PromotionEntry {
    pub name: String,
    pub pipeline_id: String,
    pub start: String,
    pub end: String,
    pub result: String,
}

pub async fn run(
    client: &SemaphoreClient,
    config: &Config,
    pipeline_id: &str,
    name_filter: Option<&str>,
    json: bool,
    utc: bool,
) -> Result<()> {
    let tz = if utc { chrono_tz::UTC } else { config.timezone() };

    // Resolve workflow from the pipeline, then list all promotion pipelines
    let ppl = client.get_pipeline(pipeline_id, false).await?;
    let all_pipelines = client.list_pipelines_for_workflow(&ppl.wf_id).await?;

    let promotions: Vec<PromotionEntry> = all_pipelines
        .iter()
        .filter(|p| p.is_promotion())
        .filter(|p| {
            name_filter
                .map(|f| p.name.to_lowercase().contains(&f.to_lowercase()))
                .unwrap_or(true)
        })
        .map(|p| PromotionEntry {
            name: p.name.clone(),
            pipeline_id: p.ppl_id.clone(),
            start: output::epoch_to_local(p.created_at.seconds, &tz),
            end: output::epoch_to_local(p.done_at.seconds, &tz),
            result: p.result_normalized(),
        })
        .collect();

    let result = PromotionsOutput {
        pipeline_id: pipeline_id.to_string(),
        promotions,
    };

    if json {
        println!("{}", output::render_json(&result));
    } else {
        println!("PIPELINE: {}\n", pipeline_id);

        if result.promotions.is_empty() {
            println!("No promotions found.");
        } else {
            println!("PROMOTIONS:");
            for p in &result.promotions {
                println!(
                    "  {:<30} {} {} -- {} {}",
                    p.name,
                    p.pipeline_id,
                    p.start,
                    p.end,
                    p.result.to_uppercase()
                );
            }
        }
    }

    Ok(())
}
