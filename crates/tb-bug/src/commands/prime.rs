use crate::api::BugsnagClient;
use crate::config::Config;
use crate::error::Result;
use crate::output;

pub async fn run(client: &BugsnagClient, config: &Config, project: Option<&str>) -> Result<()> {
    match project {
        Some(name) => run_project(client, config, name).await,
        None => run_overview(config).await,
    }
}

async fn run_overview(config: &Config) -> Result<()> {
    println!("# Bugsnag\n");

    println!("## Configured Projects\n");
    if config.projects.is_empty() {
        println!(
            "  No projects configured. Use `tb-bug config add-project <name> <id>` to add one.\n"
        );
    } else {
        for name in config.projects.keys() {
            println!("  - **{}** — `tb-bug prime --project {}`", name, name);
        }
        println!();
    }

    println!("## Quick Commands");
    println!(
        "- `tb-bug prime --project <name>` — detailed project context (errors, stability, releases)"
    );
    println!("- `tb-bug errors --project <name> --since 1d --status open` — today's open errors");
    println!("- `tb-bug report dashboard --project <name>` — full dashboard overview");
    println!("- `tb-bug search --project <name> <query>` — search error classes and messages");

    Ok(())
}

async fn run_project(client: &BugsnagClient, config: &Config, project: &str) -> Result<()> {
    let project_id = config.resolve_project(project)?;

    // Fetch open errors from last 24h, sorted by events desc
    let errors_resp = client
        .list_errors(
            project_id,
            &[("error.status", "open"), ("event.since", "1d")],
            Some("events"),
            Some("desc"),
            30,
        )
        .await?;

    let stability = client.get_stability(project_id).await.ok();
    let latest_release = client
        .get_latest_release(project_id, "production")
        .await
        .ok()
        .flatten();

    println!("# Bugsnag: {}\n", project);

    // Stability
    if let Some(trend) = &stability {
        println!("## Stability ({})", trend.release_stage_name);
        if let Some(latest) = trend.timeline_points.last() {
            let crash_free = (1.0 - latest.unhandled_rate) * 100.0;
            println!("  Crash-free sessions: {:.2}%", crash_free);
            println!(
                "  Sessions: {}  Unhandled: {}",
                output::fmt_count(latest.total_sessions_count),
                output::fmt_count(latest.unhandled_sessions_count),
            );
        }
        println!();
    }

    // Top errors
    println!("## Open Errors (last 24h)");
    if let Some(total) = errors_resp.total_count {
        println!("  Total: {}\n", total);
    }
    if errors_resp.items.is_empty() {
        println!("  No open errors with activity in the last 24h.\n");
    } else {
        for (i, e) in errors_resp.items.iter().take(10).enumerate() {
            let msg = output::truncate(&e.message, 60);
            println!("  {}. {} — {}", i + 1, e.error_class, msg,);
            println!(
                "     {} events  {} users  last seen {}",
                output::fmt_count(e.events),
                output::fmt_count(e.users),
                output::relative_time(&e.last_seen),
            );
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
        println!("  {} ({})", release.app_version, release_time,);
        println!(
            "  New errors: {}  Seen errors: {}  Sessions: {}",
            release.errors_introduced_count,
            release.errors_seen_count,
            output::fmt_count(release.total_sessions_count.unwrap_or(0)),
        );
        println!();
    }

    println!("## Quick Commands");
    println!(
        "- `tb-bug errors --project {} --since 1d --status open` — today's open errors",
        project
    );
    println!(
        "- `tb-bug errors --project {} --sort events` — errors by frequency",
        project
    );
    println!(
        "- `tb-bug errors --project {} --stage production --severity error` — production errors only",
        project
    );

    Ok(())
}
