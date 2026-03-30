use chrono::TimeZone;

use crate::config::Config;
use crate::error::Result;
use crate::index;

pub fn run() -> Result<()> {
    let config = Config::load()?;

    // Render the commands/options section with the configured default_limit
    let default_limit = config.default_limit;

    print!(
        r#"# tb-session — Claude Code session search

## Commands

- `tb-session search <query>` — full-text search across sessions
  - `--branch <name>` — filter by git branch
  - `--after <date>` — created after (YYYY-MM-DD or relative: 7d, 2w, today)
  - `--before <date>` — created before
  - `--project <path>` — filter by project path (substring match)
  - `--all-projects` — search across all projects (default: current dir)
  - `--limit <n>` — max results (default: {default_limit})
  - `--json` — structured output
  - `--no-cache` — force index rebuild first
- `tb-session list` — browse sessions by metadata (no full-text)
  - Same filters as search + `--page <n>`
- `tb-session show <id>` — session detail and conversation preview
- `tb-session resume <id>` — resume session (execs claude --resume)
- `tb-session index [--all-projects]` — rebuild search index
- `tb-session doctor` — verify setup health
- `tb-session cache-clear` — delete index for clean rebuild

## Index Status

"#
    );

    // Try to open the DB and get stats; if absent, show a friendly message
    match index::open_db(false) {
        Ok(conn) => match index::get_stats(&conn) {
            Ok(stats) => {
                println!(
                    "- {} sessions | {} projects",
                    stats.session_count, stats.project_count
                );

                // Last updated: check mtime of the DB file
                let last_updated = config
                    .db_path()
                    .ok()
                    .and_then(|p| std::fs::metadata(&p).ok())
                    .and_then(|m| m.modified().ok())
                    .map(|t| {
                        // Convert SystemTime to ISO 8601 string for relative_time
                        let duration = t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
                        let secs = duration.as_secs() as i64;
                        // Build an ISO 8601 timestamp
                        chrono::Utc
                            .timestamp_opt(secs, 0)
                            .single()
                            .map(|dt| dt.to_rfc3339())
                            .unwrap_or_else(|| "unknown".to_string())
                    })
                    .unwrap_or_else(|| "unknown".to_string());

                let relative = if last_updated == "unknown" {
                    "unknown".to_string()
                } else {
                    toolbox_core::output::relative_time(&last_updated)
                };

                println!("- Last updated: {relative}");
            }
            Err(_) => {
                println!("- Not yet built — run `tb-session index` to build");
            }
        },
        Err(_) => {
            println!("- Not yet built — run `tb-session index` to build");
        }
    }

    // Current project
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "(unknown)".to_string());

    println!();
    println!("## Current Project");
    println!();
    println!("- {cwd}");

    Ok(())
}
