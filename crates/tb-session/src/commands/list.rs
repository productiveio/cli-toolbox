use rusqlite::Connection;

use crate::error::Result;
use crate::models::{SessionList, SessionSummary};

#[allow(clippy::too_many_arguments)]
pub fn run(
    conn: &Connection,
    branch: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
    repo_paths: &[String],
    limit: usize,
    page: usize,
    json: bool,
) -> Result<()> {
    // Build WHERE clause
    let mut where_parts = vec!["is_sidechain = 0".to_string()];
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if !repo_paths.is_empty() {
        let placeholders: Vec<String> = (0..repo_paths.len())
            .map(|i| format!("?{}", i + 1))
            .collect();
        where_parts.push(format!("project_path IN ({})", placeholders.join(", ")));
        for path in repo_paths {
            params.push(Box::new(path.clone()));
        }
    }

    if let Some(b) = branch {
        let idx = params.len() + 1;
        where_parts.push(format!("git_branch = ?{idx}"));
        params.push(Box::new(b.to_string()));
    }

    if let Some(f) = from {
        let idx = params.len() + 1;
        where_parts.push(format!("modified_at >= ?{idx}"));
        params.push(Box::new(f.to_string()));
    }

    if let Some(t) = to {
        let idx = params.len() + 1;
        where_parts.push(format!("modified_at <= ?{idx}"));
        params.push(Box::new(t.to_string()));
    }

    let where_clause = where_parts.join(" AND ");

    // COUNT query for pagination
    let count_sql = format!("SELECT COUNT(*) FROM sessions WHERE {where_clause}");

    let param_refs: Vec<&dyn rusqlite::types::ToSql> =
        params.iter().map(|p| p.as_ref()).collect();

    let total: usize = conn
        .query_row(
            &count_sql,
            rusqlite::params_from_iter(param_refs.iter().copied()),
            |row| row.get::<_, i64>(0),
        )
        .map(|n| n as usize)?;

    // Paginated data query
    let offset = (page.saturating_sub(1)) * limit;
    let data_sql = format!(
        "SELECT session_id, summary, git_branch, project_path, message_count, \
         created_at, modified_at FROM sessions WHERE {where_clause} \
         ORDER BY modified_at DESC LIMIT {} OFFSET {}",
        limit, offset
    );

    let mut stmt = conn.prepare(&data_sql)?;
    let results: Vec<SessionSummary> = stmt
        .query_map(
            rusqlite::params_from_iter(param_refs.iter().copied()),
            |row| {
                Ok(SessionSummary {
                    session_id: row.get(0)?,
                    summary: row.get(1)?,
                    git_branch: row.get(2)?,
                    project_path: row.get(3)?,
                    message_count: row.get(4)?,
                    created_at: row.get(5)?,
                    modified_at: row.get(6)?,
                })
            },
        )?
        .collect::<rusqlite::Result<_>>()?;

    let list = SessionList {
        total_results: total,
        page: Some(page),
        results,
    };

    if json {
        println!("{}", toolbox_core::output::render_json(&list));
        return Ok(());
    }

    if list.results.is_empty() {
        println!(
            "{}",
            toolbox_core::output::empty_hint(
                "sessions",
                "Try --all-projects or wider date range."
            )
        );
        return Ok(());
    }

    // Human-readable table
    println!(
        "{:<36} {:<40} {:<20} {:<6} {:<12}",
        "SESSION ID", "SUMMARY", "BRANCH", "MSGS", "MODIFIED"
    );
    for s in &list.results {
        let summary = s
            .summary
            .as_deref()
            .unwrap_or("(no summary)");
        let branch = s
            .git_branch
            .as_deref()
            .unwrap_or("-");
        let modified = s
            .modified_at
            .as_deref()
            .map(toolbox_core::output::relative_time)
            .unwrap_or_else(|| "-".to_string());
        println!(
            "{:<36} {:<40} {:<20} {:<6} {:<12}",
            toolbox_core::output::truncate(&s.session_id, 36),
            toolbox_core::output::truncate(summary, 38),
            toolbox_core::output::truncate(branch, 18),
            s.message_count,
            modified,
        );
    }

    if let Some(hint) =
        toolbox_core::output::pagination_hint(page as u32, limit as u32, total as u32)
    {
        eprintln!("{}", hint);
    }

    Ok(())
}
