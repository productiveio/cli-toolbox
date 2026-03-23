use rusqlite::types::ToSql;
use rusqlite::Connection;

use crate::error::Result;
use crate::models::{SearchFilters, SearchResult, SessionMatch};

#[allow(clippy::too_many_arguments)]
pub fn run(
    conn: &Connection,
    query: &str,
    branch: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
    project: Option<&str>,
    all_projects: bool,
    limit: usize,
    json: bool,
) -> Result<()> {
    // -- Build dynamic SQL ------------------------------------------------
    let mut where_clauses = vec![
        "messages_fts MATCH ?1".to_string(),
        "s.is_sidechain = 0".to_string(),
    ];
    let mut params: Vec<Box<dyn ToSql>> = vec![Box::new(query.to_string())];
    let mut param_idx: usize = 2;

    // Project scope
    if !all_projects && project.is_none() {
        // Default: scope to current working directory
        let cwd = std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        if !cwd.is_empty() {
            where_clauses.push(format!("s.project_path = ?{param_idx}"));
            params.push(Box::new(cwd));
            param_idx += 1;
        }
    } else if all_projects && project.is_some() {
        // --all-projects + --project: LIKE filter across all projects
        let pattern = format!("%{}%", project.unwrap());
        where_clauses.push(format!("s.project_path LIKE ?{param_idx}"));
        params.push(Box::new(pattern));
        param_idx += 1;
    } else if let Some(proj) = project {
        // --project without --all-projects: LIKE filter
        let pattern = format!("%{proj}%");
        where_clauses.push(format!("s.project_path LIKE ?{param_idx}"));
        params.push(Box::new(pattern));
        param_idx += 1;
    }

    // Branch filter
    if let Some(br) = branch {
        where_clauses.push(format!("s.git_branch = ?{param_idx}"));
        params.push(Box::new(br.to_string()));
        param_idx += 1;
    }

    // Date filters
    if let Some(after) = from {
        where_clauses.push(format!("s.modified_at >= ?{param_idx}"));
        params.push(Box::new(after.to_string()));
        param_idx += 1;
    }
    if let Some(before) = to {
        where_clauses.push(format!("s.modified_at <= ?{param_idx}"));
        params.push(Box::new(before.to_string()));
        param_idx += 1;
    }

    let where_sql = where_clauses.join(" AND ");

    // Use a subquery to find best-matching sessions first, then fetch snippets.
    // snippet() cannot be used with GROUP BY directly in FTS5.
    let sql = format!(
        "SELECT
            s.session_id,
            s.summary,
            s.first_prompt,
            s.git_branch,
            s.project_path,
            s.message_count,
            s.created_at,
            s.modified_at,
            best.best_rank,
            snippet(messages_fts, 2, '«', '»', '…', 20),
            messages_fts.role
         FROM (
             SELECT messages_fts.session_id, MIN(rank) AS best_rank
             FROM messages_fts
             JOIN sessions s ON s.session_id = messages_fts.session_id
             WHERE {where_sql}
             GROUP BY messages_fts.session_id
             ORDER BY best_rank
             LIMIT ?{param_idx}
         ) best
         JOIN sessions s ON s.session_id = best.session_id
         JOIN messages_fts ON messages_fts.rowid = (
             SELECT rowid FROM messages_fts
             WHERE messages_fts.session_id = best.session_id
               AND messages_fts MATCH ?1
             LIMIT 1
         )
         ORDER BY best.best_rank"
    );

    params.push(Box::new(limit as i64));

    // Convert to &dyn ToSql slice
    let param_refs: Vec<&dyn ToSql> = params.iter().map(|p| p.as_ref()).collect();

    // -- Execute ----------------------------------------------------------
    let mut stmt = conn.prepare(&sql)?;

    let raw_rows: Vec<RawRow> = stmt
        .query_map(param_refs.as_slice(), |row| {
            Ok(RawRow {
                session_id: row.get(0)?,
                summary: row.get(1)?,
                first_prompt: row.get(2)?,
                git_branch: row.get(3)?,
                project_path: row.get(4)?,
                message_count: row.get(5)?,
                created_at: row.get(6)?,
                modified_at: row.get(7)?,
                rank: row.get(8)?,
                snippet: row.get(9)?,
                matched_role: row.get(10)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    // -- BM25 normalization: min-max scaling ------------------------------
    let results: Vec<SessionMatch> = if raw_rows.is_empty() {
        Vec::new()
    } else if raw_rows.len() == 1 {
        vec![raw_rows[0].to_match(1.0)]
    } else {
        // rank is negative in FTS5 (more negative = better match)
        // worst = least negative (highest value), best = most negative (lowest value)
        let worst = raw_rows
            .iter()
            .map(|r| r.rank)
            .fold(f64::NEG_INFINITY, f64::max);
        let best = raw_rows
            .iter()
            .map(|r| r.rank)
            .fold(f64::INFINITY, f64::min);
        let range = worst - best;

        raw_rows
            .iter()
            .map(|r| {
                let score = if range.abs() < f64::EPSILON {
                    1.0
                } else {
                    ((worst - r.rank) / range).clamp(0.0, 1.0)
                };
                r.to_match(score)
            })
            .collect()
    };

    let total_results = results.len();

    // -- Build active filters for output ----------------------------------
    let has_filters = branch.is_some()
        || from.is_some()
        || to.is_some()
        || project.is_some()
        || all_projects;

    let filters = if has_filters {
        Some(SearchFilters {
            project: project.map(|s| s.to_string()),
            branch: branch.map(|s| s.to_string()),
            from: from.map(|s| s.to_string()),
            to: to.map(|s| s.to_string()),
            all_projects,
        })
    } else {
        None
    };

    // -- Output -----------------------------------------------------------
    if json {
        let output = SearchResult {
            query: query.to_string(),
            filters,
            total_results,
            results,
        };
        println!("{}", toolbox_core::output::render_json(&output));
        return Ok(());
    }

    // Human-readable output
    if results.is_empty() {
        println!(
            "{}",
            toolbox_core::output::empty_hint(
                "sessions",
                "Try broader terms or --all-projects."
            )
        );
        return Ok(());
    }

    use colored::Colorize;
    use toolbox_core::output::{relative_time, truncate};

    eprintln!(
        "{} results for '{}'",
        total_results.to_string().bold(),
        query.bold()
    );

    // Table header
    println!(
        "\n{:<12} {:<40} {:<25} {:<6} {:<10} {:<5}",
        "SESSION", "SUMMARY", "BRANCH", "MSGS", "MODIFIED", "SCORE"
    );

    for m in &results {
        let session_short = truncate(&m.session_id, 10);
        let summary = m.summary.as_deref().unwrap_or("-");
        let branch_str = m.git_branch.as_deref().unwrap_or("-");
        let modified = m
            .modified_at
            .as_deref()
            .map(relative_time)
            .unwrap_or_else(|| "-".to_string());
        let score_pct = format!("{:.0}%", m.relevance_score * 100.0);

        println!(
            "{:<12} {:<40} {:<25} {:<6} {:<10} {:<5}",
            session_short,
            truncate(summary, 38),
            truncate(branch_str, 23),
            m.message_count,
            modified,
            score_pct,
        );
    }

    // Matched snippets below the table
    println!();
    for m in &results {
        if let Some(ref snippet) = m.matched_snippet {
            let role = m.matched_role.as_deref().unwrap_or("?");
            let session_short = truncate(&m.session_id, 10);
            println!(
                "  {} [{}] {}",
                session_short.dimmed(),
                role.cyan(),
                truncate(snippet, 120),
            );
        }
    }

    Ok(())
}

/// Intermediate row from the SQL query, before BM25 normalization.
struct RawRow {
    session_id: String,
    summary: Option<String>,
    first_prompt: Option<String>,
    git_branch: Option<String>,
    project_path: Option<String>,
    message_count: i64,
    created_at: Option<String>,
    modified_at: Option<String>,
    rank: f64,
    snippet: Option<String>,
    matched_role: Option<String>,
}

impl RawRow {
    /// Convert to a `SessionMatch` with a pre-computed relevance score.
    fn to_match(&self, score: f64) -> SessionMatch {
        SessionMatch {
            session_id: self.session_id.clone(),
            summary: self.summary.clone(),
            first_prompt: self.first_prompt.clone(),
            git_branch: self.git_branch.clone(),
            project_path: self.project_path.clone(),
            message_count: self.message_count,
            created_at: self.created_at.clone(),
            modified_at: self.modified_at.clone(),
            relevance_score: score,
            matched_snippet: self.snippet.clone(),
            matched_role: self.matched_role.clone(),
        }
    }
}
