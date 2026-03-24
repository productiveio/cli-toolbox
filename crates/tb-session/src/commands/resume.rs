use std::io::IsTerminal;

use rusqlite::Connection;

use crate::error::{Error, Result};

/// Returns true if the input looks like a UUID or UUID prefix (8+ hex chars with optional dashes).
fn looks_like_uuid(s: &str) -> bool {
    let s = s.trim();
    s.len() >= 8 && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// Search sessions by summary or first_prompt, returning the most recently modified match.
fn resolve_by_name(conn: &Connection, query: &str) -> Option<(String, Option<String>)> {
    let pattern = format!("%{}%", query);
    conn.query_row(
        "SELECT session_id, project_path FROM sessions \
         WHERE (summary LIKE ?1 OR first_prompt LIKE ?1) \
           AND is_sidechain = 0 \
         ORDER BY modified_at DESC \
         LIMIT 1",
        rusqlite::params![pattern],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .ok()
}

pub fn run(conn: &Connection, session_id: &str) -> Result<()> {
    let claude_path = which_claude()?;

    // Resolve full session ID and project_path from the index.
    // Try UUID prefix match first, then fall back to name/summary search.
    let prefix_pattern = format!("{}%", session_id);
    let resolved: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT session_id, project_path FROM sessions WHERE session_id = ?1 OR session_id LIKE ?2 ORDER BY modified_at DESC LIMIT 1",
            rusqlite::params![session_id, prefix_pattern],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok()
        .or_else(|| resolve_by_name(conn, session_id));

    if resolved.is_none() && !looks_like_uuid(session_id) {
        return Err(Error::Other(format!(
            "No session found matching '{}'. Try: tb-session search \"{}\" --all-projects",
            session_id, session_id
        )));
    }

    let full_session_id = resolved
        .as_ref()
        .map(|(id, _)| id.as_str())
        .unwrap_or(session_id);

    // Resolve the directory to resume in
    let resume_dir = resolved.as_ref().and_then(|(_, path)| {
        let path = path.as_deref()?;
        let target = std::path::Path::new(path);
        let cwd = std::env::current_dir().ok()?;
        if cwd == target {
            return None;
        }
        if target.is_dir() {
            Some(path)
        } else {
            eprintln!(
                "Warning: original project directory no longer exists: {}",
                path
            );
            None
        }
    });

    // If stdin is not a TTY, we're likely running inside Claude Code or a script.
    // Spawn a new terminal window instead of exec'ing (which would kill the parent).
    if !std::io::stdin().is_terminal() {
        return open_in_terminal(&claude_path, full_session_id, resume_dir);
    }

    // Interactive: cd into the original project and exec claude directly
    if let Some(path) = resume_dir {
        eprintln!("Resuming in {}", path);
        std::env::set_current_dir(path)
            .map_err(|e| Error::Other(format!("Failed to cd into {}: {}", path, e)))?;
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = std::process::Command::new(&claude_path)
            .arg("--resume")
            .arg(full_session_id)
            .exec();
        Err(Error::Other(format!("Failed to exec claude: {}", err)))
    }

    #[cfg(not(unix))]
    {
        let status = std::process::Command::new(&claude_path)
            .arg("--resume")
            .arg(full_session_id)
            .status()
            .map_err(|e| Error::Other(format!("Failed to run claude: {}", e)))?;

        if !status.success() {
            return Err(Error::Other(format!(
                "claude exited with status: {}",
                status
            )));
        }
        Ok(())
    }
}

/// Open a new terminal tab and run `claude --resume` there.
fn open_in_terminal(claude_path: &str, session_id: &str, resume_dir: Option<&str>) -> Result<()> {
    if !cfg!(target_os = "macos") {
        return Err(Error::Other(
            "resume in new terminal tab is only supported on macOS. \
             Run manually: claude --resume <session-id>"
                .into(),
        ));
    }

    let resume_cmd = format!(
        "{} --resume {}",
        shell_escape(claude_path),
        shell_escape(session_id)
    );
    let full_cmd = match resume_dir {
        Some(dir) => format!("cd {} && {}", shell_escape(dir), resume_cmd),
        None => resume_cmd,
    };

    // Pass command via TB_SESSION_CMD env var to avoid AppleScript injection.
    // The osascript reads it with `do shell script "echo $TB_SESSION_CMD"` or
    // writes it directly via `write text` using `(system attribute "TB_SESSION_CMD")`.
    let terminal = std::env::var("TERM_PROGRAM").unwrap_or_default();
    let script = build_osascript(&terminal);

    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .env("TB_SESSION_CMD", &full_cmd)
        .output()
        .map_err(|e| Error::Other(format!("Failed to run osascript: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(Error::Other(format!(
            "Failed to open terminal window: {}",
            stderr.trim()
        )));
    }

    if let Some(dir) = resume_dir {
        eprintln!("Opened new terminal tab in {} to resume session", dir);
    } else {
        eprintln!("Opened new terminal tab to resume session");
    }
    Ok(())
}

fn build_osascript(terminal: &str) -> String {
    // Command is passed via TB_SESSION_CMD env var to avoid AppleScript string injection.
    match terminal {
        "iTerm.app" => r#"tell application "iTerm2"
    tell current window
        create tab with default profile
        tell current session
            write text (system attribute "TB_SESSION_CMD")
        end tell
    end tell
end tell"#
            .to_string(),
        // Terminal.app and anything else
        _ => r#"tell application "Terminal"
    activate
    do script (system attribute "TB_SESSION_CMD")
end tell"#
            .to_string(),
    }
}

fn shell_escape(s: &str) -> String {
    if s.contains(|c: char| c.is_whitespace() || c == '\'' || c == '"' || c == '\\') {
        format!("'{}'", s.replace('\'', "'\\''"))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_looks_like_uuid() {
        assert!(looks_like_uuid("9a06add5-028b-484c-a0bf-f4fc08921042"));
        assert!(looks_like_uuid("9a06add5"));
        assert!(looks_like_uuid("abcdef12"));
        assert!(!looks_like_uuid("auth-refactor"));
        assert!(!looks_like_uuid("fix bug"));
        assert!(!looks_like_uuid("PR #6"));
        assert!(!looks_like_uuid("abc")); // too short
    }
}

fn which_claude() -> Result<String> {
    let output = std::process::Command::new("which")
        .arg("claude")
        .output()
        .map_err(|e| Error::Other(format!("Failed to run 'which claude': {}", e)))?;

    if !output.status.success() {
        return Err(Error::Other(
            "claude CLI not found in PATH. Install it from https://docs.anthropic.com/s/claude-code"
                .to_string(),
        ));
    }

    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        return Err(Error::Other("claude CLI not found in PATH".to_string()));
    }

    Ok(path)
}
