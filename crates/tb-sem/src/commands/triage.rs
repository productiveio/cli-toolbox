use std::collections::HashMap;

use serde::Serialize;

use crate::api::SemaphoreClient;
use crate::config::Config;
use crate::error::Result;
use crate::logs;
use crate::output;

#[derive(Debug, Serialize)]
pub struct TriageOutput {
    pub pipeline_id: String,
    pub time_window: String,
    pub branch: String,
    pub result: String,
    pub total_scenarios: u32,
    pub passed: u32,
    pub failed: u32,
    pub failures: Vec<TriageFailure>,
    pub retried_passed: usize,
    pub flaky_scenarios: Vec<FlakyScenario>,
    pub error_distribution: Vec<ErrorDist>,
    pub deploy_overlap: bool,
    pub diagnosis: String,
}

#[derive(Debug, Serialize)]
pub struct TriageFailure {
    pub name: String,
    pub error_class: String,
    pub error_detail: String,
}

#[derive(Debug, Serialize)]
pub struct FlakyScenario {
    pub name: String,
    pub attempts: u32,
}

#[derive(Debug, Serialize)]
pub struct ErrorDist {
    pub classification: String,
    pub count: usize,
}

pub async fn run(
    client: &SemaphoreClient,
    config: &Config,
    pipeline_id: Option<&str>,
    json: bool,
    utc: bool,
) -> Result<()> {
    let tz = if utc { chrono_tz::UTC } else { config.timezone() };

    // Default to e2e-tests project
    let (e2e_id, e2e_branch) = config.resolve_project("e2e-tests")?;

    // Step 1: Find pipeline
    let ppl_id = if let Some(id) = pipeline_id {
        id.to_string()
    } else {
        if !json {
            eprintln!("Finding latest failed e2e run...");
        }
        let workflows = client.list_workflows(e2e_id, Some(e2e_branch), None, None).await?;
        let mut found = None;
        for wf in &workflows {
            let ppl = client.get_pipeline(&wf.initial_ppl_id, false).await?;
            if ppl.result_normalized() == "failed" {
                found = Some(wf.initial_ppl_id.clone());
                break;
            }
        }
        found.ok_or_else(|| crate::error::TbSemError::Other("No failed e2e runs found".into()))?
    };

    // Step 2: Pipeline details
    let ppl = client.get_pipeline(&ppl_id, true).await?;
    let time_window = match (&ppl.created_at_dt(), &ppl.done_at_dt()) {
        (Some(s), Some(e)) => format!(
            "{} -- {}",
            s.with_timezone(&tz).format("%Y-%m-%d %H:%M"),
            e.with_timezone(&tz).format("%Y-%m-%d %H:%M")
        ),
        _ => output::iso_to_local(&ppl.created_at, &tz),
    };

    // Step 3: Parse failures
    let failed_job = ppl
        .blocks
        .iter()
        .flat_map(|b| &b.jobs)
        .find(|j| j.is_failed());

    let mut total_scenarios = 0u32;
    let mut passed = 0u32;
    let mut failed = 0u32;
    let mut failures = Vec::new();
    let mut retried_passed_count = 0usize;
    let mut flaky_scenarios = Vec::new();

    if let Some(job) = failed_job {
        if !json {
            eprintln!("Fetching logs for: {} ...", job.name);
        }
        let events = client.get_job_logs(&job.job_id).await?;

        // Cucumber summary for ground truth counts
        let text = logs::flatten_log(&events);
        let (cucumber_failed, cucumber_passed) = logs::parse_cucumber_summary(&text).unwrap_or((0, 0));
        total_scenarios = cucumber_failed + cucumber_passed;
        passed = cucumber_passed;
        failed = cucumber_failed;

        // Parsed scenario details (uses cucumber summary section as ground truth)
        let all_scenarios = logs::parse_scenarios_best(&events);
        for s in &all_scenarios {
            match s.result {
                logs::ScenarioOutcome::Failed => {
                    failures.push(TriageFailure {
                        name: s.name.clone(),
                        error_class: s.error_class.as_ref().map(|c| c.to_string()).unwrap_or_default(),
                        error_detail: s.error_detail.clone().unwrap_or_default(),
                    });
                }
                logs::ScenarioOutcome::RetriedPassed => {
                    retried_passed_count += 1;
                    flaky_scenarios.push(FlakyScenario {
                        name: s.name.clone(),
                        attempts: s.attempts,
                    });
                }
                logs::ScenarioOutcome::Passed => {}
            }
        }
    }

    // Error distribution
    let mut dist_map: HashMap<String, usize> = HashMap::new();
    for f in &failures {
        let key = if f.error_detail.is_empty() {
            f.error_class.clone()
        } else {
            format!("{} ({})", f.error_class, f.error_detail)
        };
        *dist_map.entry(key).or_default() += 1;
    }
    let error_distribution: Vec<ErrorDist> = dist_map
        .into_iter()
        .map(|(classification, count)| ErrorDist {
            classification,
            count,
        })
        .collect();

    // Step 4: Deploy overlap check
    let mut deploy_overlap = false;
    let mut deploy_lines = Vec::new();

    if let Ok((api_id, api_branch)) = config.resolve_project("api") {
        let api_workflows = client
            .list_workflows(api_id, Some(api_branch), None, None)
            .await
            .unwrap_or_default();

        if let (Some(start), Some(end)) = (ppl.created_at_dt(), ppl.done_at_dt()) {
            for wf in api_workflows.iter().take(10) {
                let wf_time = wf.created_at.to_datetime();
                let diff = (wf_time - start).num_hours().abs();
                if diff > 2 {
                    continue;
                }

                let pipelines = client
                    .list_pipelines_for_workflow(&wf.wf_id)
                    .await
                    .unwrap_or_default();

                for p in &pipelines {
                    if p.is_promotion() {
                        let p_start = p.created_at_dt();
                        let p_end = p.done_at_dt();
                        let overlaps = p_start < end && p_end > start;

                        if overlaps {
                            deploy_overlap = true;
                        }

                        let status = if overlaps { "!! OVERLAP" } else { "no overlap" };
                        deploy_lines.push(format!(
                            "  API deploy: {} -- {} {} ({})",
                            output::epoch_to_local(p.created_at.seconds, &tz),
                            output::epoch_to_local(p.done_at.seconds, &tz),
                            p.name,
                            status
                        ));
                    }
                }
            }
        }
    }

    // Diagnosis
    let diagnosis = if failures.is_empty() {
        "No failures detected.".to_string()
    } else {
        let all_infra = failures.iter().all(|f| f.error_class == "INFRA");
        let overlap_str = if deploy_overlap {
            "Deploy overlap detected."
        } else {
            "No deploy overlap."
        };
        if all_infra {
            format!(
                "All {} failures are INFRA. {} Likely cause: infrastructure issue.",
                failures.len(),
                overlap_str
            )
        } else {
            format!("{} failures across multiple categories. {}", failures.len(), overlap_str)
        }
    };

    let result = TriageOutput {
        pipeline_id: ppl_id.clone(),
        time_window,
        branch: ppl.branch_name.clone(),
        result: ppl.result.to_uppercase(),
        total_scenarios,
        passed,
        failed,
        failures,
        retried_passed: retried_passed_count,
        flaky_scenarios,
        error_distribution,
        deploy_overlap,
        diagnosis,
    };

    if json {
        println!("{}", output::render_json(&result));
    } else {
        println!("\n=== E2E TRIAGE REPORT ===\n");
        println!(
            "Pipeline: {} | {} | {}",
            &result.pipeline_id[..8.min(result.pipeline_id.len())],
            result.time_window,
            result.branch
        );
        println!("Result:   {}", result.result);
        println!(
            "Tests:    {} scenarios -- {} passed, {} failed (cucumber summary)",
            result.total_scenarios, result.passed, result.failed
        );
        if result.retried_passed > 0 {
            println!(
                "          {} scenarios failed at least once but passed on retry",
                result.retried_passed
            );
        }
        println!();

        if !result.failures.is_empty() {
            println!(
                "FAILED ({} scenarios, all attempts failed):{:<7} {:<10} DETAIL",
                result.failures.len(), "", "CLASS"
            );
            for f in &result.failures {
                let name = if f.name.len() > 48 {
                    format!("{}...", &f.name[..45])
                } else {
                    f.name.clone()
                };
                println!("  {:<48} {:<10} {}", name, f.error_class, f.error_detail);
            }

            if !result.error_distribution.is_empty() {
                println!("\nERROR DISTRIBUTION:");
                for e in &result.error_distribution {
                    println!("  {}: {} scenarios", e.classification, e.count);
                }
            }
        }

        if !result.flaky_scenarios.is_empty() {
            println!(
                "\nFLAKY ({} scenarios, retried and passed):",
                result.flaky_scenarios.len()
            );
            for f in &result.flaky_scenarios {
                let name = if f.name.len() > 48 {
                    format!("{}...", &f.name[..45])
                } else {
                    f.name.clone()
                };
                println!("  {:<48} ({} attempts)", name, f.attempts);
            }
        }

        println!("\nDEPLOY OVERLAP CHECK:");
        if deploy_lines.is_empty() {
            println!("  No recent API deploys found in time window.");
        } else {
            for line in &deploy_lines {
                println!("{}", line);
            }
        }

        println!("\nDIAGNOSIS: {}", result.diagnosis);
    }

    Ok(())
}
