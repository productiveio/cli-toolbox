use serde::Serialize;

use crate::api::SemaphoreClient;
use crate::config::Config;
use crate::error::Result;
use crate::logs;
use crate::output;

#[derive(Debug, Serialize)]
pub struct HistoryOutput {
    pub test_name: String,
    pub entries: Vec<HistoryEntry>,
    pub verdict: String,
}

#[derive(Debug, Serialize)]
pub struct HistoryEntry {
    pub time: String,
    pub pipeline_id: String,
    pub result: String,
    pub detail: Option<String>,
}

#[allow(clippy::too_many_arguments)]
pub async fn run(
    client: &SemaphoreClient,
    config: &Config,
    test_name: &str,
    project: &str,
    branch: Option<&str>,
    limit: usize,
    json: bool,
    utc: bool,
) -> Result<()> {
    let tz = if utc {
        chrono_tz::UTC
    } else {
        config.timezone()?
    };
    let project_id = config.resolve_project(project)?;

    let workflows = client
        .list_workflows(project_id, branch, output::branchless_created_after(branch), None)
        .await?;

    let mut entries = Vec::new();

    for wf in workflows.iter().take(limit) {
        let ppl = client.get_pipeline(&wf.initial_ppl_id, true).await?;
        let time = output::epoch_to_local(wf.created_at.seconds, &tz);

        let (result, detail) = if let Some(job) = ppl.find_test_job() {
            let events = client.get_job_logs(&job.job_id).await?;
            let scenarios = logs::parse_scenarios_best(&events);
            let found = scenarios
                .iter()
                .find(|s| s.name.to_lowercase().contains(&test_name.to_lowercase()));

            match found {
                Some(s) => {
                    let result = s.result.to_string();
                    let detail = s.error_detail.clone();
                    (result, detail)
                }
                None => ("NOT FOUND".to_string(), None),
            }
        } else {
            ("NO TEST JOB".to_string(), None)
        };

        entries.push(HistoryEntry {
            time,
            pipeline_id: wf.initial_ppl_id.clone(),
            result,
            detail,
        });
    }

    // Determine verdict
    let failed_count = entries.iter().filter(|e| e.result == "Failed").count();
    let verdict = if failed_count == 0 {
        "Test has been passing consistently.".to_string()
    } else if failed_count == entries.len() {
        "Test has been failing consistently -- persistent regression.".to_string()
    } else if failed_count == 1 && entries.first().is_some_and(|e| e.result == "Failed") {
        "First failure in recent history -- likely new regression or infra issue.".to_string()
    } else {
        format!(
            "Intermittent -- failed {} out of {} runs (flaky test).",
            failed_count,
            entries.len()
        )
    };

    let result = HistoryOutput {
        test_name: test_name.to_string(),
        entries,
        verdict,
    };

    if json {
        println!("{}", output::render_json(&result));
    } else {
        println!("TEST: {}\n", result.test_name);
        println!("{:<20} {:<12} DETAIL", "TIME", "RESULT");
        for e in &result.entries {
            let detail = e.detail.as_deref().unwrap_or("");
            println!("{:<20} {:<12} {}", e.time, e.result, detail);
        }
        println!("\nVERDICT: {}", result.verdict);
    }

    Ok(())
}
