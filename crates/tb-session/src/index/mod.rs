pub mod builder;
pub mod parser;
pub mod scanner;
pub mod schema;

use std::path::{Path, PathBuf};
use rusqlite::Connection;
use crate::error::{Error, Result};
use scanner::FileInfo;

/// Statistics about the current index.
#[derive(Debug)]
pub struct IndexStats {
    pub session_count: u64,
    pub project_count: u64,
    pub db_size_bytes: u64,
}

/// Open (or create) the SQLite database at `~/.cache/tb-session/index.db`.
///
/// Enables WAL journal mode for better concurrent read performance, then
/// creates or resets the schema depending on `no_cache`.
pub fn open_db(no_cache: bool) -> Result<Connection> {
    let db_path = db_path()?;
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let conn = Connection::open(&db_path)?;

    // Enable WAL mode — best-effort, ignore errors on read-only filesystems.
    let _ = conn.execute_batch("PRAGMA journal_mode=WAL;");

    if no_cache {
        schema::reset_tables(&conn)?;
    } else {
        schema::create_tables(&conn)?;
    }

    Ok(conn)
}

/// Return the path to the index database file.
fn db_path() -> Result<PathBuf> {
    let cache_dir = dirs::cache_dir()
        .ok_or_else(|| Error::Other("cannot determine cache directory".to_string()))?;
    Ok(cache_dir.join("tb-session").join("index.db"))
}

/// Return `true` if the on-disk file is newer than what's recorded in the DB.
///
/// A file is considered stale when:
/// - it has no entry in `sessions`, or
/// - its recorded `file_mtime` is less than the current mtime on disk.
pub fn is_stale(conn: &Connection, file_info: &FileInfo) -> Result<bool> {
    let result: rusqlite::Result<i64> = conn.query_row(
        "SELECT file_mtime FROM sessions WHERE session_id = ?1",
        rusqlite::params![file_info.session_id],
        |row| row.get(0),
    );

    match result {
        Ok(recorded_mtime) => Ok(file_info.file_mtime > recorded_mtime as u64),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(true),
        Err(e) => Err(e.into()),
    }
}

/// Scan for JSONL session files, re-index any that are stale.
///
/// `projects_dir` is the root Claude projects directory
/// (typically `~/.claude/projects`). `scope_to_cwd` narrows the scan to a
/// single project path when provided.
pub fn ensure_fresh(
    conn: &Connection,
    projects_dir: &Path,
    scope_to_cwd: Option<&Path>,
) -> Result<()> {
    let files = scanner::scan_projects(projects_dir, scope_to_cwd)?;

    for file_info in &files {
        if is_stale(conn, file_info)? {
            let parsed = parser::parse_session(&file_info.file_path)?;
            builder::index_session(conn, file_info, &parsed)?;
        }
    }

    Ok(())
}

/// Remove index rows for session files that no longer exist on disk.
///
/// Uses parameterized queries — no string interpolation of user data.
pub fn cleanup_deleted(conn: &Connection) -> Result<()> {
    // Collect all file paths currently in the index.
    let mut stmt = conn.prepare("SELECT session_id, file_path FROM sessions")?;
    let rows: Vec<(String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;

    for (session_id, file_path) in rows {
        if !std::path::Path::new(&file_path).exists() {
            // Delete FTS rows first (no foreign-key cascade in FTS5).
            conn.execute(
                "DELETE FROM messages_fts WHERE session_id = ?1",
                rusqlite::params![session_id],
            )?;
            conn.execute(
                "DELETE FROM sessions WHERE session_id = ?1",
                rusqlite::params![session_id],
            )?;
        }
    }

    Ok(())
}

/// Return aggregate statistics about the index.
pub fn get_stats(conn: &Connection) -> Result<IndexStats> {
    let session_count: u64 = conn
        .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get::<_, i64>(0))
        .map(|n| n as u64)?;

    let project_count: u64 = conn
        .query_row(
            "SELECT COUNT(DISTINCT project_path) FROM sessions",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|n| n as u64)?;

    let db_size_bytes: u64 = db_path()
        .and_then(|p| std::fs::metadata(&p).map_err(Into::into))
        .map(|m| m.len())
        .unwrap_or(0);

    Ok(IndexStats {
        session_count,
        project_count,
        db_size_bytes,
    })
}
