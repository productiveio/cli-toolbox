use crate::api::BugsnagClient;
use crate::config::Config;
use crate::error::Result;
use crate::output;

pub async fn run(
    client: &BugsnagClient,
    config: &Config,
    project: &str,
    query: &str,
    limit: usize,
    json: bool,
) -> Result<()> {
    let project_id = config.resolve_project(project)?;

    // Bugsnag doesn't have a server-side search API for error classes/messages,
    // so we fetch open errors and filter client-side
    let resp = client
        .list_errors(
            project_id,
            &[("error.status", "open")],
            Some("events"),
            Some("desc"),
            100,
        )
        .await?;

    let query_lower = query.to_lowercase();
    let matches: Vec<_> = resp
        .items
        .iter()
        .filter(|e| {
            e.error_class.to_lowercase().contains(&query_lower)
                || e.message.to_lowercase().contains(&query_lower)
        })
        .take(limit)
        .collect();

    if json {
        println!("{}", output::render_json(&matches));
    } else {
        println!(
            "Search '{}' in '{}': {} matches\n",
            query, project, matches.len()
        );
        if matches.is_empty() {
            println!("  No matching errors found.");
        } else {
            for e in &matches {
                println!(
                    "  {} — {}",
                    e.error_class,
                    output::truncate(&e.message, 60),
                );
                println!(
                    "     {} events  {} users  last seen {}  [{}] [{}]",
                    output::fmt_count(e.events),
                    output::fmt_count(e.users),
                    output::relative_time(&e.last_seen),
                    e.severity,
                    e.id,
                );
            }
        }
    }

    Ok(())
}
