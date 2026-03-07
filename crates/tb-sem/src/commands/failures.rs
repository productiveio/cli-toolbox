use std::collections::HashMap;

use serde::Serialize;

use crate::api::SemaphoreClient;
use crate::error::Result;
use crate::logs::{self, ScenarioOutcome};
use crate::output;

#[derive(Debug, Serialize)]
pub struct FailuresOutput {
    pub pipeline_id: String,
    /// Counts from the cucumber summary line (ground truth)
    pub cucumber_total: u32,
    pub cucumber_passed: u32,
    pub cucumber_failed: u32,
    /// Scenarios that failed on all attempts (never passed)
    pub failures: Vec<FailureEntry>,
    /// Scenarios that failed at least once but eventually passed on retry
    pub retried_passed: Vec<FailureEntry>,
    pub error_distribution: Vec<ErrorDistEntry>,
}

#[derive(Debug, Serialize)]
pub struct FailureEntry {
    pub name: String,
    pub error_class: String,
    pub error_detail: String,
    pub attempts: u32,
}

#[derive(Debug, Serialize)]
pub struct ErrorDistEntry {
    pub classification: String,
    pub count: usize,
}

pub async fn run(
    client: &SemaphoreClient,
    pipeline_id: &str,
    json: bool,
) -> Result<()> {
    let ppl = client.get_pipeline(pipeline_id, true).await?;

    let failed_job = ppl
        .blocks
        .iter()
        .flat_map(|b| &b.jobs)
        .find(|j| j.is_failed());

    let Some(job) = failed_job else {
        println!("No failed jobs in pipeline {}", pipeline_id);
        return Ok(());
    };

    if !json {
        eprintln!("Fetching logs for job: {} ({})", job.name, &job.job_id[..8]);
    }

    let events = client.get_job_logs(&job.job_id).await?;

    // Get cucumber summary (ground truth counts)
    let text = logs::flatten_log(&events);
    let (cucumber_failed, cucumber_passed) = logs::parse_cucumber_summary(&text).unwrap_or((0, 0));

    // Get all parsed scenarios with dedup
    let all_scenarios = logs::parse_scenarios_best(&events);

    let mut failures = Vec::new();
    let mut retried_passed = Vec::new();

    for s in &all_scenarios {
        let entry = FailureEntry {
            name: s.name.clone(),
            error_class: s.error_class.as_ref().map(|c| c.to_string()).unwrap_or_default(),
            error_detail: s.error_detail.clone().unwrap_or_default(),
            attempts: s.attempts,
        };
        match s.result {
            ScenarioOutcome::Failed => failures.push(entry),
            ScenarioOutcome::RetriedPassed => retried_passed.push(entry),
            ScenarioOutcome::Passed => {}
        }
    }

    // Error distribution across all failures (including retried)
    let mut dist_map: HashMap<String, usize> = HashMap::new();
    for f in failures.iter().chain(retried_passed.iter()) {
        if f.error_class.is_empty() {
            continue;
        }
        let key = if f.error_detail.is_empty() {
            f.error_class.clone()
        } else {
            format!("{} ({})", f.error_class, f.error_detail)
        };
        *dist_map.entry(key).or_default() += 1;
    }
    let error_distribution: Vec<ErrorDistEntry> = dist_map
        .into_iter()
        .map(|(classification, count)| ErrorDistEntry {
            classification,
            count,
        })
        .collect();

    let result = FailuresOutput {
        pipeline_id: pipeline_id.to_string(),
        cucumber_total: cucumber_failed + cucumber_passed,
        cucumber_passed,
        cucumber_failed,
        failures,
        retried_passed,
        error_distribution,
    };

    if json {
        println!("{}", output::render_json(&result));
    } else {
        println!(
            "\nRESULT: {} scenarios -- {} passed, {} failed (cucumber summary)\n",
            result.cucumber_total, result.cucumber_passed, result.cucumber_failed
        );

        // Failed scenarios (never passed on any attempt)
        if result.failures.is_empty() {
            println!("FAILED (0): (none)");
        } else {
            println!(
                "FAILED ({}):{:<38} {:<10} DETAIL",
                result.failures.len(), "", "CLASS"
            );
            for f in &result.failures {
                let name = if f.name.len() > 48 {
                    format!("{}...", &f.name[..45])
                } else {
                    f.name.clone()
                };
                let attempts = if f.attempts > 1 {
                    format!("({} attempts)", f.attempts)
                } else {
                    String::new()
                };
                println!(
                    "  {:<48} {:<10} {} {}",
                    name, f.error_class, f.error_detail, attempts
                );
            }
        }

        // Retried-but-passed scenarios
        if result.retried_passed.is_empty() {
            println!("\nFLAKY (0): (none)");
        } else {
            println!(
                "\nFLAKY ({}, retried and passed):",
                result.retried_passed.len()
            );
            for f in &result.retried_passed {
                let name = if f.name.len() > 48 {
                    format!("{}...", &f.name[..45])
                } else {
                    f.name.clone()
                };
                println!(
                    "  {:<48} ({} attempts, passed on retry)",
                    name, f.attempts
                );
            }
        }

        if !result.error_distribution.is_empty() {
            println!("\nERROR DISTRIBUTION:");
            for e in &result.error_distribution {
                println!("  {}: {} scenarios", e.classification, e.count);
            }
        }
    }

    Ok(())
}
