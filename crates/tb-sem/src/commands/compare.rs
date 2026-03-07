use std::collections::HashMap;

use serde::Serialize;

use crate::api::SemaphoreClient;
use crate::config::Config;
use crate::error::Result;
use crate::logs;
use crate::output;

#[derive(Debug, Serialize)]
pub struct CompareOutput {
    pub run1: RunSummary,
    pub run2: RunSummary,
    pub new_failures: Vec<String>,
    pub fixed: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct RunSummary {
    pub pipeline_id: String,
    pub time: String,
    pub passed: usize,
    pub failed: usize,
}

pub async fn run(
    client: &SemaphoreClient,
    config: &Config,
    pipeline_id_1: &str,
    pipeline_id_2: &str,
    json: bool,
    utc: bool,
) -> Result<()> {
    let tz = if utc { chrono_tz::UTC } else { config.timezone() };

    // Fetch both pipelines and their test results
    let (scenarios1, ppl1) = fetch_scenarios(client, pipeline_id_1).await?;
    let (scenarios2, ppl2) = fetch_scenarios(client, pipeline_id_2).await?;

    let results1: HashMap<&str, &logs::ScenarioOutcome> = scenarios1
        .iter()
        .map(|s| (s.name.as_str(), &s.result))
        .collect();
    let results2: HashMap<&str, &logs::ScenarioOutcome> = scenarios2
        .iter()
        .map(|s| (s.name.as_str(), &s.result))
        .collect();

    let mut new_failures = Vec::new();
    let mut fixed = Vec::new();

    for (name, result2) in &results2 {
        let was_failing = results1
            .get(name)
            .is_some_and(|r| **r == logs::ScenarioOutcome::Failed);
        let is_failing = **result2 == logs::ScenarioOutcome::Failed;

        if is_failing && !was_failing {
            new_failures.push(name.to_string());
        }
    }

    for (name, result1) in &results1 {
        let was_failing = **result1 == logs::ScenarioOutcome::Failed;
        let is_passing = results2
            .get(name)
            .is_some_and(|r| **r != logs::ScenarioOutcome::Failed);

        if was_failing && is_passing {
            fixed.push(name.to_string());
        }
    }

    let time1 = output::iso_to_local(&ppl1.created_at, &tz);
    let time2 = output::iso_to_local(&ppl2.created_at, &tz);

    let passed1 = scenarios1.iter().filter(|s| s.result != logs::ScenarioOutcome::Failed).count();
    let failed1 = scenarios1.iter().filter(|s| s.result == logs::ScenarioOutcome::Failed).count();
    let passed2 = scenarios2.iter().filter(|s| s.result != logs::ScenarioOutcome::Failed).count();
    let failed2 = scenarios2.iter().filter(|s| s.result == logs::ScenarioOutcome::Failed).count();

    let result = CompareOutput {
        run1: RunSummary {
            pipeline_id: pipeline_id_1.to_string(),
            time: time1,
            passed: passed1,
            failed: failed1,
        },
        run2: RunSummary {
            pipeline_id: pipeline_id_2.to_string(),
            time: time2,
            passed: passed2,
            failed: failed2,
        },
        new_failures,
        fixed,
    };

    if json {
        println!("{}", output::render_json(&result));
    } else {
        println!(
            "RUN 1: {} {} -- {} passed, {} failed",
            &result.run1.pipeline_id,
            result.run1.time,
            result.run1.passed,
            result.run1.failed
        );
        println!(
            "RUN 2: {} {} -- {} passed, {} failed\n",
            &result.run2.pipeline_id,
            result.run2.time,
            result.run2.passed,
            result.run2.failed
        );

        println!("NEW FAILURES ({}):", result.new_failures.len());
        if result.new_failures.is_empty() {
            println!("  (none)");
        } else {
            for name in &result.new_failures {
                println!("  + {} (was: passed)", name);
            }
        }

        println!("\nFIXED ({}):", result.fixed.len());
        if result.fixed.is_empty() {
            println!("  (none)");
        } else {
            for name in &result.fixed {
                println!("  - {} (was: failed)", name);
            }
        }
    }

    Ok(())
}

async fn fetch_scenarios(
    client: &SemaphoreClient,
    pipeline_id: &str,
) -> Result<(Vec<logs::ScenarioResult>, crate::api::Pipeline)> {
    let ppl = client.get_pipeline(pipeline_id, true).await?;

    let scenarios = if let Some(job) = ppl.find_test_job() {
        let events = client.get_job_logs(&job.job_id).await?;
        logs::parse_scenarios_best(&events)
    } else {
        Vec::new()
    };

    Ok((scenarios, ppl))
}
