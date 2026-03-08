use crate::api::BugsnagClient;
use crate::config::Config;
use crate::error::Result;
use crate::output;

pub async fn run_dashboard(client: &BugsnagClient, config: &Config, project: &str) -> Result<()> {
    let project_id = config.resolve_project(project)?;

    // Fetch stability, errors, and latest release in parallel-ish
    let stability = client.get_stability(project_id).await.ok();
    let latest_release = client
        .get_latest_release(project_id, "production")
        .await
        .ok()
        .flatten();

    let errors = client
        .list_errors(
            project_id,
            &[("error.status", "open"), ("event.since", "7d")],
            Some("events"),
            Some("desc"),
            30,
        )
        .await?;

    println!("# Dashboard: {}\n", project);

    // Stability — show summary + last 7 days
    if let Some(trend) = &stability {
        println!("## Stability ({})", trend.release_stage_name);
        let points = &trend.timeline_points;
        if !points.is_empty() {
            let total_sessions: u64 = points.iter().map(|b| b.total_sessions_count).sum();
            let total_unhandled: u64 = points.iter().map(|b| b.unhandled_sessions_count).sum();
            let overall_rate = if total_sessions > 0 {
                (1.0 - total_unhandled as f64 / total_sessions as f64) * 100.0
            } else {
                100.0
            };
            println!(
                "  30-day average: {:.2}% crash-free ({} sessions, {} unhandled)\n",
                overall_rate,
                output::fmt_count(total_sessions),
                output::fmt_count(total_unhandled),
            );
            let recent = if points.len() > 7 {
                &points[points.len() - 7..]
            } else {
                points
            };
            for b in recent {
                let crash_free = (1.0 - b.unhandled_rate) * 100.0;
                println!(
                    "  {} — {:.2}% crash-free ({} sessions, {} unhandled)",
                    output::relative_time(&b.bucket_start),
                    crash_free,
                    output::fmt_count(b.total_sessions_count),
                    output::fmt_count(b.unhandled_sessions_count),
                );
            }
        }
        println!();
    }

    // Latest release
    if let Some(release) = &latest_release {
        println!("## Latest Release (production)");
        let release_time = release
            .release_time
            .as_deref()
            .map(output::relative_time)
            .unwrap_or_else(|| "?".to_string());
        println!("  {} ({})", release.app_version, release_time);
        println!(
            "  New errors: {}  Seen errors: {}  Sessions: {}",
            release.errors_introduced_count,
            release.errors_seen_count,
            output::fmt_count(release.total_sessions_count.unwrap_or(0)),
        );
        println!();
    }

    // Top errors
    println!("## Open Errors (last 7d)");
    if let Some(total) = errors.total_count {
        println!("  Total: {}\n", total);
    }
    if errors.items.is_empty() {
        println!("  No open errors with activity in the last 7 days.\n");
    } else {
        for (i, e) in errors.items.iter().enumerate() {
            println!(
                "  {}. {} — {}",
                i + 1,
                e.error_class,
                output::truncate(&e.message, 80),
            );
            println!(
                "     {} events  {} users  last seen {}  [{}]",
                output::fmt_count(e.events),
                output::fmt_count(e.users),
                output::relative_time(&e.last_seen),
                e.severity,
            );
        }
    }

    Ok(())
}

pub async fn run_open(
    client: &BugsnagClient,
    config: &Config,
    project: &str,
    limit: usize,
    json: bool,
) -> Result<()> {
    let project_id = config.resolve_project(project)?;

    let mut errors = client
        .list_errors(
            project_id,
            &[("error.status", "open")],
            Some("events"),
            Some("desc"),
            100,
        )
        .await?;

    // Sort by impact: events * users
    errors
        .items
        .sort_by(|a, b| (b.events * b.users).cmp(&(a.events * a.users)));
    errors.items.truncate(limit);

    if json {
        println!("{}", output::render_json(&errors.items));
    } else {
        if let Some(total) = errors.total_count {
            println!(
                "Open errors for '{}' (showing {} of {})\n",
                project,
                errors.items.len(),
                total
            );
        }
        println!(
            "{:<8} {:>7} {:>6} {:>10}  {:<13} ERROR CLASS",
            "SEV", "EVENTS", "USERS", "IMPACT", "LAST SEEN"
        );
        for e in &errors.items {
            let impact = e.events * e.users;
            let class = output::truncate(&e.error_class, 50);
            println!(
                "{:<8} {:>7} {:>6} {:>10}  {:<13} {}",
                e.severity,
                output::fmt_count(e.events),
                output::fmt_count(e.users),
                output::fmt_count(impact),
                output::relative_time(&e.last_seen),
                class,
            );
        }
    }

    Ok(())
}
