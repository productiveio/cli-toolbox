use rusqlite::Connection;
use std::path::Path;

use colored::Colorize;

use crate::error::{Error, Result};
use crate::index::parser;
use crate::models::{MessagePreview, SessionDetail};

pub fn run(conn: &Connection, session_id: &str, json: bool) -> Result<()> {
    // Look up session by full ID or prefix match
    let prefix_pattern = format!("{}%", session_id);
    let mut stmt = conn.prepare(
        "SELECT session_id, summary, first_prompt, git_branch, project_path,
                message_count, created_at, modified_at, is_sidechain, file_path
         FROM sessions
         WHERE session_id = ?1 OR session_id LIKE ?2
         LIMIT 1",
    )?;

    let row = stmt
        .query_row(rusqlite::params![session_id, prefix_pattern], |row| {
            Ok(RawSession {
                session_id: row.get(0)?,
                summary: row.get(1)?,
                first_prompt: row.get(2)?,
                git_branch: row.get(3)?,
                project_path: row.get(4)?,
                message_count: row.get(5)?,
                created_at: row.get(6)?,
                modified_at: row.get(7)?,
                is_sidechain: row.get::<_, i64>(8)? != 0,
                file_path: row.get(9)?,
            })
        })
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                Error::Other(format!("Session '{}' not found", session_id))
            }
            other => Error::Db(other),
        })?;

    // Parse JSONL file for message preview if it exists
    let messages = if Path::new(&row.file_path).exists() {
        match parser::parse_session(Path::new(&row.file_path)) {
            Ok(parsed) => Some(parsed.messages),
            Err(_) => None,
        }
    } else {
        None
    };

    if json {
        output_json(&row, messages.as_deref());
    } else {
        output_human(&row, messages.as_deref());
    }

    Ok(())
}

fn output_json(row: &RawSession, messages: Option<&[parser::ParsedMessage]>) {
    let total_messages = messages.map(|m| m.len());

    let previews = messages.map(|msgs| {
        msgs.iter()
            .take(20)
            .map(|m| MessagePreview {
                role: m.role.clone(),
                content: truncate_string(&m.content, 2000),
                timestamp: m.timestamp.clone(),
            })
            .collect()
    });

    let detail = SessionDetail {
        session_id: row.session_id.clone(),
        summary: row.summary.clone(),
        first_prompt: row.first_prompt.clone(),
        git_branch: row.git_branch.clone(),
        project_path: row.project_path.clone(),
        message_count: row.message_count,
        created_at: row.created_at.clone(),
        modified_at: row.modified_at.clone(),
        is_sidechain: row.is_sidechain,
        messages: previews,
        total_messages,
    };

    println!("{}", toolbox_core::output::render_json(&detail));
}

fn output_human(row: &RawSession, messages: Option<&[parser::ParsedMessage]>) {
    // Metadata header
    println!("{} {}", "Session:".bold(), row.session_id);
    if let Some(ref summary) = row.summary {
        println!("{} {}", "Summary:".bold(), summary);
    }
    if let Some(ref prompt) = row.first_prompt {
        println!(
            "{} {}",
            "First prompt:".bold(),
            toolbox_core::output::truncate(prompt, 120)
        );
    }
    if let Some(ref branch) = row.git_branch {
        println!("{} {}", "Branch:".bold(), branch);
    }
    if let Some(ref path) = row.project_path {
        println!("{} {}", "Project:".bold(), path);
    }
    println!("{} {}", "Messages:".bold(), row.message_count);
    if let Some(ref created) = row.created_at {
        println!(
            "{} {} ({})",
            "Created:".bold(),
            created,
            toolbox_core::output::relative_time(created)
        );
    }
    if let Some(ref modified) = row.modified_at {
        println!(
            "{} {} ({})",
            "Modified:".bold(),
            modified,
            toolbox_core::output::relative_time(modified)
        );
    }
    if row.is_sidechain {
        println!("{} yes", "Sidechain:".bold());
    }

    // Conversation preview
    if let Some(msgs) = messages {
        if !msgs.is_empty() {
            println!("\n{}", "--- Conversation Preview ---".dimmed());

            let total = msgs.len();
            if total <= 10 {
                for m in msgs {
                    print_message(m);
                }
            } else {
                // First 5
                for m in &msgs[..5] {
                    print_message(m);
                }
                println!(
                    "  {}",
                    format!("... ({} omitted) ...", total - 10).dimmed()
                );
                // Last 5
                for m in &msgs[total - 5..] {
                    print_message(m);
                }
            }
        }
    }

    println!(
        "\n{} tb-session resume {}",
        "Resume:".bold(),
        row.session_id
    );
}

fn print_message(m: &parser::ParsedMessage) {
    let content = toolbox_core::output::truncate(&m.content, 500);
    let role_label = match m.role.as_str() {
        "user" => "user".blue().bold(),
        "assistant" => "assistant".green().bold(),
        other => other.normal(),
    };
    println!("\n  [{}] {}", role_label, content);
}

/// Truncate a string to at most `max` characters, appending "..." if truncated.
fn truncate_string(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let boundary = s.floor_char_boundary(max.saturating_sub(3));
    format!("{}...", &s[..boundary])
}

struct RawSession {
    session_id: String,
    summary: Option<String>,
    first_prompt: Option<String>,
    git_branch: Option<String>,
    project_path: Option<String>,
    message_count: i64,
    created_at: Option<String>,
    modified_at: Option<String>,
    is_sidechain: bool,
    file_path: String,
}
