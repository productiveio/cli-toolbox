use std::collections::HashMap;

use serde::Serialize;

use crate::api::SemaphoreClient;
use crate::config::Config;
use crate::error::Result;
use crate::logs;
use crate::output;

#[derive(Debug, Serialize)]
pub struct FlakyOutput {
    pub project: String,
    pub runs_checked: usize,
    pub flaky_tests: Vec<FlakyTest>,
}

#[derive(Debug, Serialize)]
pub struct FlakyTest {
    pub name: String,
    pub feature_file: Option<String>,
    pub flaky_count: usize,
    pub failure_count: usize,
    pub total_appearances: usize,
    pub flaky_rate: String,
}

pub async fn run(
    client: &SemaphoreClient,
    config: &Config,
    project: &str,
    branch: Option<&str>,
    limit: usize,
    json: bool,
    utc: bool,
) -> Result<()> {
    let tz = if utc {
        chrono_tz::UTC
    } else {
        config.timezone()
    };
    let project_id = config.resolve_project(project)?;

    let workflows = client.list_workflows(project_id, branch, None, None).await?;

    // Track per-scenario: (flaky_count, failure_count, total_count, feature_file)
    let mut stats: HashMap<String, (usize, usize, usize, Option<String>)> = HashMap::new();
    let mut runs_checked = 0;

    for wf in workflows.iter().take(limit) {
        let ppl = client.get_pipeline(&wf.initial_ppl_id, true).await?;

        let Some(job) = ppl.find_test_job() else {
            continue;
        };

        if !json {
            let time = output::epoch_to_local(wf.created_at.seconds, &tz);
            eprint!("\r  Checking run {} ({})...", time, &wf.initial_ppl_id);
        }

        let events = client.get_job_logs(&job.job_id).await?;
        let scenarios = logs::parse_scenarios_best(&events);
        runs_checked += 1;

        for s in &scenarios {
            let entry = stats
                .entry(s.name.clone())
                .or_insert((0, 0, 0, s.feature_file.clone()));
            entry.2 += 1; // total
            match s.result {
                logs::ScenarioOutcome::RetriedPassed => entry.0 += 1,
                logs::ScenarioOutcome::Failed => entry.1 += 1,
                logs::ScenarioOutcome::Passed => {}
            }
        }
    }

    if !json {
        eprintln!(); // clear progress line
    }

    // Filter to tests that were flaky at least once, sort by flaky rate
    let mut flaky_tests: Vec<FlakyTest> = stats
        .into_iter()
        .filter(|(_, (flaky, _, _, _))| *flaky > 0)
        .map(|(name, (flaky, failures, total, feature))| {
            let rate = if total > 0 {
                (flaky as f64 / total as f64) * 100.0
            } else {
                0.0
            };
            FlakyTest {
                name,
                feature_file: feature,
                flaky_count: flaky,
                failure_count: failures,
                total_appearances: total,
                flaky_rate: format!("{:.0}%", rate),
            }
        })
        .collect();

    flaky_tests.sort_by(|a, b| b.flaky_count.cmp(&a.flaky_count));

    let result = FlakyOutput {
        project: project.to_string(),
        runs_checked,
        flaky_tests,
    };

    if json {
        println!("{}", output::render_json(&result));
    } else {
        println!(
            "\n=== FLAKY TESTS ({}, last {} runs) ===\n",
            result.project, result.runs_checked
        );

        if result.flaky_tests.is_empty() {
            println!("  No flaky tests detected.");
        } else {
            println!(
                "  {:<48} {:>5} {:>7} {:>5} {:>5}",
                "TEST NAME", "FLAKY", "FAILED", "TOTAL", "RATE"
            );
            for t in &result.flaky_tests {
                let name = if t.name.len() > 48 {
                    format!("{}...", &t.name[..45])
                } else {
                    t.name.clone()
                };
                println!(
                    "  {:<48} {:>5} {:>7} {:>5} {:>5}",
                    name, t.flaky_count, t.failure_count, t.total_appearances, t.flaky_rate
                );
            }
        }

        println!(
            "\nSUMMARY: {} flaky tests found across {} runs",
            result.flaky_tests.len(),
            result.runs_checked
        );
    }

    Ok(())
}
