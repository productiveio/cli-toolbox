use serde::Serialize;

use crate::api::SemaphoreClient;
use crate::error::Result;
use crate::logs;
use crate::output;

#[derive(Debug, Serialize)]
pub struct TestsOutput {
    pub pipeline_id: String,
    pub total: u32,
    pub passed: u32,
    pub failed: u32,
    pub scenarios: Vec<ScenarioEntry>,
}

#[derive(Debug, Serialize)]
pub struct ScenarioEntry {
    pub name: String,
    pub result: String,
    pub error_class: Option<String>,
    pub error_detail: Option<String>,
}

pub async fn run(
    client: &SemaphoreClient,
    pipeline_id: &str,
    failed_only: bool,
    retried_only: bool,
    summary_only: bool,
    json: bool,
) -> Result<()> {
    let ppl = client.get_pipeline(pipeline_id, true).await?;

    let Some(job) = ppl.find_test_job() else {
        println!("No test job found in pipeline {}", pipeline_id);
        return Ok(());
    };

    if !json {
        eprintln!("Fetching logs for: {} ...", job.name);
    }

    let events = client.get_job_logs(&job.job_id).await?;
    let all_scenarios = logs::parse_scenarios_best(&events);

    let scenarios: Vec<ScenarioEntry> = all_scenarios
        .iter()
        .filter(|s| {
            if failed_only {
                return s.result == logs::ScenarioOutcome::Failed;
            }
            if retried_only {
                return s.result == logs::ScenarioOutcome::RetriedPassed;
            }
            true
        })
        .map(|s| ScenarioEntry {
            name: s.name.clone(),
            result: s.result.to_string(),
            error_class: s.error_class.as_ref().map(|c| c.to_string()),
            error_detail: s.error_detail.clone(),
        })
        .collect();

    let total = all_scenarios.len() as u32;
    let passed = all_scenarios
        .iter()
        .filter(|s| s.result == logs::ScenarioOutcome::Passed)
        .count() as u32;
    let failed = all_scenarios
        .iter()
        .filter(|s| s.result == logs::ScenarioOutcome::Failed)
        .count() as u32;

    let result = TestsOutput {
        pipeline_id: pipeline_id.to_string(),
        total,
        passed,
        failed,
        scenarios,
    };

    if json {
        println!("{}", output::render_json(&result));
    } else if summary_only {
        println!(
            "PIPELINE: {} | {} passed, {} failed | Total: {}",
            pipeline_id, result.passed, result.failed, result.total
        );
    } else {
        println!(
            "PIPELINE: {} | {} passed, {} failed\n",
            pipeline_id, result.passed, result.failed
        );

        if result.scenarios.is_empty() {
            println!("(no scenarios matched filter)");
        } else {
            for s in &result.scenarios {
                let detail = match (&s.error_class, &s.error_detail) {
                    (Some(c), Some(d)) => format!("  {} {}", c, d),
                    (Some(c), None) => format!("  {}", c),
                    _ => String::new(),
                };
                println!("  {:<8} {}{}", s.result, s.name, detail);
            }
        }
    }

    Ok(())
}
