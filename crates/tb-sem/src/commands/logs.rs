use crate::api::SemaphoreClient;
use crate::error::Result;
use crate::logs as log_parser;
use crate::output;

#[allow(clippy::too_many_arguments)]
pub async fn run(
    client: &SemaphoreClient,
    job_id: &str,
    grep: Option<&str>,
    tail: Option<usize>,
    head: Option<usize>,
    summary: bool,
    errors_only: bool,
    raw: bool,
    json: bool,
) -> Result<()> {
    let events = client.get_job_logs(job_id).await?;
    let text = log_parser::flatten_log(&events);

    let text = if raw { text } else { output::strip_ansi(&text) };

    // --summary: just the cucumber summary
    if summary {
        if let Some((failed, passed)) = log_parser::parse_cucumber_summary(&text) {
            let total = failed + passed;
            let line = format!("{} scenarios -- {} passed, {} failed", total, passed, failed);
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
                l.contains("Error") || l.contains("error")
                    || l.contains("FAIL") || l.contains("fail")
                    || l.contains("502") || l.contains("503")
                    || l.contains("Timeout")
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        text
    };

    let lines: Vec<&str> = text.lines().collect();

    let filtered: Vec<&str> = if let Some(pattern) = grep {
        let re = regex::Regex::new(pattern)
            .map_err(|e| crate::error::SemiError::Other(format!("Invalid regex: {}", e)))?;
        lines.into_iter().filter(|l| re.is_match(l)).collect()
    } else {
        lines
    };

    let output_lines: Vec<&str> = if let Some(n) = tail {
        filtered.into_iter().rev().take(n).collect::<Vec<_>>().into_iter().rev().collect()
    } else if let Some(n) = head {
        filtered.into_iter().take(n).collect()
    } else {
        filtered
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&output_lines).unwrap_or_default());
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
