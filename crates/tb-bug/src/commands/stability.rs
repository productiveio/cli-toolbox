use crate::api::BugsnagClient;
use crate::config::Config;
use crate::error::Result;
use crate::output;

pub async fn run(client: &BugsnagClient, config: &Config, project: &str, json: bool) -> Result<()> {
    let project_id = config.resolve_project(project)?;
    let trend = client.get_stability(project_id).await?;

    if json {
        println!("{}", output::render_json(&trend));
    } else {
        println!("Stability: {} ({})\n", project, trend.release_stage_name);
        println!(
            "{:<13} {:>12} {:>10} {:>10}",
            "PERIOD", "CRASH-FREE", "SESSIONS", "UNHANDLED"
        );
        for b in &trend.timeline_points {
            let crash_free = (1.0 - b.unhandled_rate) * 100.0;
            println!(
                "{:<13} {:>11.2}% {:>10} {:>10}",
                output::relative_time(&b.bucket_start),
                crash_free,
                output::fmt_count(b.total_sessions_count),
                output::fmt_count(b.unhandled_sessions_count),
            );
        }
    }

    Ok(())
}
