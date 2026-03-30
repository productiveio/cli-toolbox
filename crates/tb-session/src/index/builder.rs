use super::parser::ParsedSession;
use super::scanner::FileInfo;
use crate::error::Result;
use rusqlite::{Connection, params};

pub fn index_session(
    conn: &Connection,
    file_info: &FileInfo,
    parsed: &ParsedSession,
) -> Result<()> {
    // Determine metadata: prefer index_metadata fields over parsed fields.
    let meta = file_info.index_metadata.as_ref();

    let summary = meta
        .and_then(|m| m.summary.as_deref())
        .or(parsed.summary.as_deref());

    let first_prompt = meta
        .and_then(|m| m.first_prompt.as_deref())
        .or(parsed.first_prompt.as_deref());

    let git_branch = meta
        .and_then(|m| m.git_branch.as_deref())
        .or(parsed.git_branch.as_deref());

    let message_count: i64 = meta
        .and_then(|m| m.message_count)
        .unwrap_or(parsed.message_count) as i64;

    let created_at = meta
        .and_then(|m| m.created.as_deref())
        .or(parsed.created_at.as_deref());

    let modified_at = meta
        .and_then(|m| m.modified.as_deref())
        .or(parsed.modified_at.as_deref());

    let is_sidechain: i64 = meta
        .and_then(|m| m.is_sidechain)
        .unwrap_or(parsed.is_sidechain) as i64;

    // INSERT or REPLACE the session row.
    conn.execute(
        "INSERT OR REPLACE INTO sessions
            (session_id, project_path, project_dir, file_path, file_mtime,
             summary, first_prompt, git_branch, message_count,
             created_at, modified_at, is_sidechain)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            file_info.session_id,
            file_info.project_path,
            file_info.project_dir,
            file_info.file_path.to_string_lossy().as_ref(),
            file_info.file_mtime as i64,
            summary,
            first_prompt,
            git_branch,
            message_count,
            created_at,
            modified_at,
            is_sidechain,
        ],
    )?;

    // DELETE existing FTS rows for this session (handles re-indexing).
    conn.execute(
        "DELETE FROM messages_fts WHERE session_id = ?1",
        params![file_info.session_id],
    )?;

    // INSERT each message into the FTS5 table.
    for msg in &parsed.messages {
        conn.execute(
            "INSERT INTO messages_fts (session_id, role, content, timestamp)
             VALUES (?1, ?2, ?3, ?4)",
            params![file_info.session_id, msg.role, msg.content, msg.timestamp,],
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::parser::ParsedMessage;
    use super::super::scanner::IndexEntry;
    use super::*;
    use crate::index::schema;
    use rusqlite::Connection;
    use std::path::PathBuf;

    fn in_memory() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory DB");
        schema::create_tables(&conn).expect("create_tables");
        conn
    }

    fn make_file_info(session_id: &str) -> FileInfo {
        FileInfo {
            session_id: session_id.to_string(),
            file_path: PathBuf::from(format!("/tmp/{session_id}.jsonl")),
            file_mtime: 1700000000,
            project_path: "/Users/test/myapp".to_string(),
            project_dir: "-Users-test-myapp".to_string(),
            index_metadata: None,
        }
    }

    fn make_parsed(summary: Option<&str>, messages: Vec<ParsedMessage>) -> ParsedSession {
        ParsedSession {
            summary: summary.map(|s| s.to_string()),
            first_prompt: messages
                .iter()
                .find(|m| m.role == "user")
                .map(|m| m.content.clone()),
            git_branch: Some("main".to_string()),
            message_count: messages.len(),
            created_at: Some("2024-01-01T00:00:00Z".to_string()),
            modified_at: Some("2024-01-02T00:00:00Z".to_string()),
            is_sidechain: false,
            messages,
        }
    }

    fn msg(role: &str, content: &str) -> ParsedMessage {
        ParsedMessage {
            role: role.to_string(),
            content: content.to_string(),
            timestamp: Some("2024-01-01T00:00:00Z".to_string()),
        }
    }

    #[test]
    fn test_index_session_inserts_metadata() {
        let conn = in_memory();
        let file_info = make_file_info("session-1");
        let parsed = make_parsed(Some("My summary"), vec![msg("user", "Hello")]);

        index_session(&conn, &file_info, &parsed).unwrap();

        let summary: String = conn
            .query_row(
                "SELECT summary FROM sessions WHERE session_id = 'session-1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(summary, "My summary");
    }

    #[test]
    fn test_index_session_inserts_fts_messages() {
        let conn = in_memory();
        let file_info = make_file_info("session-2");
        let parsed = make_parsed(
            Some("summary"),
            vec![
                msg("user", "First message"),
                msg("assistant", "Second message"),
                msg("user", "Third message"),
            ],
        );

        index_session(&conn, &file_info, &parsed).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages_fts WHERE session_id = 'session-2'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn test_index_session_prefers_index_metadata() {
        let conn = in_memory();
        let mut file_info = make_file_info("session-3");
        file_info.index_metadata = Some(IndexEntry {
            session_id: "session-3".to_string(),
            summary: Some("Index summary".to_string()),
            first_prompt: Some("Index first prompt".to_string()),
            message_count: Some(99),
            git_branch: Some("feature/index-branch".to_string()),
            created: Some("2023-06-01T00:00:00Z".to_string()),
            modified: Some("2023-06-02T00:00:00Z".to_string()),
            is_sidechain: Some(true),
            project_path: Some("/Users/test/myapp".to_string()),
        });

        let parsed = make_parsed(Some("Parsed summary"), vec![msg("user", "Parsed prompt")]);

        index_session(&conn, &file_info, &parsed).unwrap();

        let (summary, first_prompt, message_count, git_branch, created_at, modified_at, is_sidechain): (
            String, String, i64, String, String, String, i64,
        ) = conn
            .query_row(
                "SELECT summary, first_prompt, message_count, git_branch, created_at, modified_at, is_sidechain
                 FROM sessions WHERE session_id = 'session-3'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
            )
            .unwrap();

        assert_eq!(summary, "Index summary");
        assert_eq!(first_prompt, "Index first prompt");
        assert_eq!(message_count, 99);
        assert_eq!(git_branch, "feature/index-branch");
        assert_eq!(created_at, "2023-06-01T00:00:00Z");
        assert_eq!(modified_at, "2023-06-02T00:00:00Z");
        assert_eq!(is_sidechain, 1);
    }

    #[test]
    fn test_fts_search_finds_content() {
        let conn = in_memory();
        let file_info = make_file_info("session-4");
        let parsed = make_parsed(
            Some("summary"),
            vec![
                msg("user", "implement the authentication middleware"),
                msg(
                    "assistant",
                    "Sure, I will implement the authentication middleware",
                ),
            ],
        );

        index_session(&conn, &file_info, &parsed).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages_fts WHERE messages_fts MATCH 'authentication'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
    }
}
