use crate::error::Result;
use rusqlite::Connection;

/// Create all tables and FTS5 virtual table if they don't exist.
pub fn create_tables(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS sessions (
            session_id     TEXT PRIMARY KEY,
            project_path   TEXT NOT NULL,
            project_dir    TEXT NOT NULL,
            file_path      TEXT NOT NULL,
            file_mtime     INTEGER NOT NULL,
            summary        TEXT,
            first_prompt   TEXT,
            git_branch     TEXT,
            message_count  INTEGER DEFAULT 0,
            created_at     TEXT,
            modified_at    TEXT,
            is_sidechain   INTEGER DEFAULT 0
        );

        -- Note: NOT contentless (spec says content='') because contentless FTS5
        -- does not support snippet(), COUNT(*), or simple DELETE.
        CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
            session_id,
            role,
            content,
            timestamp
        );

        CREATE INDEX IF NOT EXISTS idx_sessions_project_path ON sessions(project_path);
        CREATE INDEX IF NOT EXISTS idx_sessions_git_branch ON sessions(git_branch);
        CREATE INDEX IF NOT EXISTS idx_sessions_modified_at ON sessions(modified_at);
        ",
    )?;
    Ok(())
}

/// Drop all tables and recreate (for --no-cache full rebuild).
pub fn reset_tables(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        DROP TABLE IF EXISTS messages_fts;
        DROP TABLE IF EXISTS sessions;
        ",
    )?;
    create_tables(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn in_memory() -> Connection {
        Connection::open_in_memory().expect("in-memory DB")
    }

    #[test]
    fn test_create_tables() {
        let conn = in_memory();
        create_tables(&conn).expect("create_tables should succeed");

        // Verify sessions table exists by inserting a row
        conn.execute(
            "INSERT INTO sessions (session_id, project_path, project_dir, file_path, file_mtime)
             VALUES ('sid1', '/proj', 'mydir', '/proj/file.jsonl', 1234567890)",
            [],
        )
        .expect("sessions table should exist and accept inserts");

        // Verify messages_fts table exists by inserting a row
        conn.execute(
            "INSERT INTO messages_fts (session_id, role, content, timestamp)
             VALUES ('sid1', 'user', 'hello world', '2024-01-01T00:00:00Z')",
            [],
        )
        .expect("messages_fts table should exist and accept inserts");

        // Verify data is retrievable
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
            .expect("COUNT query should succeed");
        assert_eq!(count, 1);
    }

    #[test]
    fn test_create_tables_idempotent() {
        let conn = in_memory();
        create_tables(&conn).expect("first call should succeed");
        create_tables(&conn).expect("second call should not error");

        // Tables should still be usable
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
            .expect("COUNT query should succeed");
        assert_eq!(count, 0);
    }

    #[test]
    fn test_reset_tables() {
        let conn = in_memory();
        create_tables(&conn).expect("create_tables should succeed");

        // Insert data into both tables
        conn.execute(
            "INSERT INTO sessions (session_id, project_path, project_dir, file_path, file_mtime)
             VALUES ('sid1', '/proj', 'mydir', '/proj/file.jsonl', 1234567890)",
            [],
        )
        .expect("insert into sessions");
        conn.execute(
            "INSERT INTO messages_fts (session_id, role, content, timestamp)
             VALUES ('sid1', 'user', 'some content', '2024-01-01T00:00:00Z')",
            [],
        )
        .expect("insert into messages_fts");

        // Confirm data exists
        let session_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
            .expect("COUNT sessions");
        assert_eq!(session_count, 1);

        // Reset and verify tables are empty
        reset_tables(&conn).expect("reset_tables should succeed");

        let session_count_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
            .expect("COUNT sessions after reset");
        assert_eq!(session_count_after, 0);

        let fts_count_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM messages_fts", [], |r| r.get(0))
            .expect("COUNT messages_fts after reset");
        assert_eq!(fts_count_after, 0);
    }
}
