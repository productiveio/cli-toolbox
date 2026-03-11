use crate::api::SemaphoreClient;
use crate::error::Result;
use crate::logs as log_parser;
use crate::output;

#[allow(clippy::too_many_arguments)]
pub async fn run(
    client: &SemaphoreClient,
    id: &str,
    job_name: Option<&str>,
    grep: Option<&str>,
    ignore_case: bool,
    tail: Option<usize>,
    head: Option<usize>,
    summary: bool,
    errors_only: bool,
    raw: bool,
    json: bool,
) -> Result<()> {
    // Resolve job ID: if --job is given, treat `id` as pipeline ID
    let job_id = if let Some(name) = job_name {
        resolve_job_by_name(client, id, name).await?
    } else {
        // Try as pipeline ID first (if it has jobs), fall back to job ID
        match client.get_pipeline(id, true).await {
            Ok(ppl) if !ppl.blocks.is_empty() => {
                let jobs: Vec<_> = ppl.blocks.iter().flat_map(|b| &b.jobs).collect();
                if jobs.len() == 1 {
                    // Single job — use it directly
                    jobs[0].job_id.clone()
                } else {
                    // Multiple jobs — list them and ask user to pick
                    eprintln!(
                        "Pipeline {} has {} jobs. Use --job to select one:\n",
                        id,
                        jobs.len()
                    );
                    for j in &jobs {
                        eprintln!(
                            "  {:<30} {}  {}",
                            j.name,
                            &j.job_id[..8],
                            j.result.to_uppercase()
                        );
                    }
                    return Ok(());
                }
            }
            _ => id.to_string(), // Not a pipeline — treat as job ID
        }
    };

    let events = client.get_job_logs(&job_id).await?;
    let text = log_parser::flatten_log(&events);

    let text = if raw { text } else { output::strip_ansi(&text) };

    // --summary: just the cucumber summary
    if summary {
        if let Some((failed, passed)) = log_parser::parse_cucumber_summary(&text) {
            let total = failed + passed;
            let line = format!(
                "{} scenarios -- {} passed, {} failed",
                total, passed, failed
            );
            if json {
                println!(
                    "{}",
                    serde_json::json!({"total": total, "passed": passed, "failed": failed})
                );
            } else {
                println!("{}", line);
            }
        } else {
            println!("(no cucumber summary found in logs)");
        }
        return Ok(());
    }

    // --errors: extract error-looking lines
    let text = if errors_only {
        text.lines()
            .filter(|l| {
                l.contains("Error")
                    || l.contains("error")
                    || l.contains("FAIL")
                    || l.contains("fail")
                    || l.contains("502")
                    || l.contains("503")
                    || l.contains("Timeout")
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        text
    };

    let lines: Vec<&str> = text.lines().collect();

    let filtered: Vec<&str> = if let Some(pattern) = grep {
        let pattern = if ignore_case {
            format!("(?i){}", pattern)
        } else {
            pattern.to_string()
        };
        let re = regex::Regex::new(&pattern)
            .map_err(|e| crate::error::TbSemError::Other(format!("Invalid regex: {}", e)))?;
        lines.into_iter().filter(|l| re.is_match(l)).collect()
    } else {
        lines
    };

    let output_lines: Vec<&str> = if let Some(n) = tail {
        filtered
            .into_iter()
            .rev()
            .take(n)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    } else if let Some(n) = head {
        filtered.into_iter().take(n).collect()
    } else {
        filtered
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&output_lines).unwrap_or_default()
        );
    } else {
        for line in &output_lines {
            println!("{}", line);
        }
        if grep.is_some() {
            eprintln!("\n({} matches)", output_lines.len());
        }
    }

    Ok(())
}

/// Resolve a job ID from a pipeline ID and job name (case-insensitive substring match).
async fn resolve_job_by_name(
    client: &SemaphoreClient,
    pipeline_id: &str,
    job_name: &str,
) -> Result<String> {
    let ppl = client.get_pipeline(pipeline_id, true).await?;
    let name_lower = job_name.to_lowercase();

    let matches: Vec<_> = ppl
        .blocks
        .iter()
        .flat_map(|b| &b.jobs)
        .filter(|j| j.name.to_lowercase().contains(&name_lower))
        .collect();

    match matches.len() {
        0 => Err(crate::error::TbSemError::Other(format!(
            "No job matching '{}' in pipeline {}",
            job_name, pipeline_id
        ))),
        1 => Ok(matches[0].job_id.clone()),
        _ => {
            let names: Vec<_> = matches.iter().map(|j| j.name.as_str()).collect();
            Err(crate::error::TbSemError::Other(format!(
                "Multiple jobs match '{}': {}",
                job_name,
                names.join(", ")
            )))
        }
    }
}
