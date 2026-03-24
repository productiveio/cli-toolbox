use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use crate::error::Result;

#[derive(Debug)]
pub struct ParsedSession {
    pub summary: Option<String>,
    pub first_prompt: Option<String>,
    pub git_branch: Option<String>,
    pub message_count: usize,
    pub created_at: Option<String>,
    pub modified_at: Option<String>,
    pub is_sidechain: bool,
    pub messages: Vec<ParsedMessage>,
}

#[derive(Debug)]
pub struct ParsedMessage {
    pub role: String,
    pub content: String,
    pub timestamp: Option<String>,
}

fn extract_content(message: &serde_json::Value) -> String {
    match &message["content"] {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(arr) => arr
            .iter()
            .filter(|item| item.get("type").and_then(|t| t.as_str()) == Some("text"))
            .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

pub fn parse_session(file_path: &Path) -> Result<ParsedSession> {
    let file = File::open(file_path)?;
    let reader = BufReader::new(file);

    let mut first_timestamp: Option<String> = None;
    let mut last_timestamp: Option<String> = None;
    let mut git_branch: Option<String> = None;
    let mut is_sidechain = false;
    let mut messages: Vec<ParsedMessage> = Vec::new();
    let mut first_prompt: Option<String> = None;
    let mut summary: Option<String> = None;

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let entry: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Track timestamps
        if let Some(ts) = entry.get("timestamp").and_then(|t| t.as_str()) {
            if first_timestamp.is_none() {
                first_timestamp = Some(ts.to_string());
            }
            last_timestamp = Some(ts.to_string());
        }

        // Extract gitBranch from first entry that has it
        if git_branch.is_none()
            && let Some(branch) = entry.get("gitBranch").and_then(|b| b.as_str()) {
                git_branch = Some(branch.to_string());
            }

        // Check isSidechain
        if let Some(sc) = entry.get("isSidechain").and_then(|v| v.as_bool())
            && sc {
                is_sidechain = true;
            }

        // Extract user/assistant messages
        if let Some(message) = entry.get("message") {
            let role = match message.get("role").and_then(|r| r.as_str()) {
                Some(r) if r == "user" || r == "assistant" => r.to_string(),
                _ => continue,
            };

            let content = extract_content(message);

            let timestamp = entry
                .get("timestamp")
                .and_then(|t| t.as_str())
                .map(|s| s.to_string());

            if role == "user" && first_prompt.is_none() && !content.is_empty() {
                first_prompt = Some(content.clone());
            }

            if role == "assistant" && summary.is_none() && !content.is_empty() {
                let boundary = content.floor_char_boundary(197);
                summary = Some(if boundary < content.len() {
                    format!("{}...", &content[..boundary])
                } else {
                    content.clone()
                });
            }

            messages.push(ParsedMessage {
                role,
                content,
                timestamp,
            });
        }
    }

    Ok(ParsedSession {
        summary,
        first_prompt,
        git_branch,
        message_count: messages.len(),
        created_at: first_timestamp,
        modified_at: last_timestamp,
        is_sidechain,
        messages,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_jsonl(lines: &[&str]) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        for line in lines {
            writeln!(file, "{line}").unwrap();
        }
        file
    }

    #[test]
    fn test_parse_user_message() {
        let file = write_jsonl(&[
            r#"{"timestamp":"2024-01-01T00:00:00Z","message":{"role":"user","content":"Hello, world!"}}"#,
        ]);

        let parsed = parse_session(file.path()).unwrap();

        assert_eq!(parsed.messages.len(), 1);
        assert_eq!(parsed.messages[0].role, "user");
        assert_eq!(parsed.messages[0].content, "Hello, world!");
        assert_eq!(parsed.first_prompt.as_deref(), Some("Hello, world!"));
    }

    #[test]
    fn test_parse_assistant_message_with_content_array() {
        let file = write_jsonl(&[
            r#"{"timestamp":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":[{"type":"text","text":"Here is the answer."},{"type":"tool_use","id":"123","name":"bash"}]}}"#,
        ]);

        let parsed = parse_session(file.path()).unwrap();

        assert_eq!(parsed.messages.len(), 1);
        assert_eq!(parsed.messages[0].role, "assistant");
        assert_eq!(parsed.messages[0].content, "Here is the answer.");
    }

    #[test]
    fn test_parse_extracts_metadata() {
        let file = write_jsonl(&[
            r#"{"timestamp":"2024-01-01T00:00:00Z","gitBranch":"feature/my-branch","isSidechain":true,"message":{"role":"user","content":"First message"}}"#,
            r#"{"timestamp":"2024-01-02T00:00:00Z","message":{"role":"assistant","content":"Response"}}"#,
        ]);

        let parsed = parse_session(file.path()).unwrap();

        assert_eq!(parsed.git_branch.as_deref(), Some("feature/my-branch"));
        assert_eq!(parsed.created_at.as_deref(), Some("2024-01-01T00:00:00Z"));
        assert_eq!(parsed.modified_at.as_deref(), Some("2024-01-02T00:00:00Z"));
        assert_eq!(parsed.message_count, 2);
        assert!(parsed.is_sidechain);
    }

    #[test]
    fn test_parse_skips_non_message_lines() {
        let file = write_jsonl(&[
            r#"{"type":"progress","text":"Thinking..."}"#,
            r#"{"type":"file-history-snapshot","files":[]}"#,
            r#"{"timestamp":"2024-01-01T00:00:00Z","message":{"role":"user","content":"Real message"}}"#,
        ]);

        let parsed = parse_session(file.path()).unwrap();

        assert_eq!(parsed.messages.len(), 1);
        assert_eq!(parsed.messages[0].content, "Real message");
        // message_count reflects actual user/assistant messages, not all JSON lines
        assert_eq!(parsed.message_count, 1);
    }

    #[test]
    fn test_parse_handles_malformed_lines() {
        let file = write_jsonl(&[
            r#"this is not valid json {"#,
            r#"{"timestamp":"2024-01-01T00:00:00Z","message":{"role":"user","content":"Valid"}}"#,
        ]);

        let parsed = parse_session(file.path()).unwrap();

        assert_eq!(parsed.messages.len(), 1);
        assert_eq!(parsed.messages[0].content, "Valid");
        // Only 1 valid JSON line (the malformed one is skipped)
        assert_eq!(parsed.message_count, 1);
    }

    #[test]
    fn test_parse_zero_messages() {
        // All lines are non-message entries — message_count should be 0
        let file = write_jsonl(&[
            r#"{"type":"progress","text":"Thinking..."}"#,
            r#"{"type":"file-history-snapshot","files":[]}"#,
            r#"{"cwd":"/Users/test/myapp"}"#,
        ]);

        let parsed = parse_session(file.path()).unwrap();
        assert_eq!(parsed.messages.len(), 0);
        assert_eq!(parsed.message_count, 0);
        assert!(parsed.first_prompt.is_none());
        assert!(parsed.summary.is_none());
    }

    #[test]
    fn test_parse_skips_non_user_assistant_roles() {
        let file = write_jsonl(&[
            r#"{"timestamp":"2024-01-01T00:00:00Z","message":{"role":"system","content":"System prompt"}}"#,
            r#"{"timestamp":"2024-01-01T00:00:01Z","message":{"role":"tool","content":"Tool output"}}"#,
            r#"{"timestamp":"2024-01-01T00:00:02Z","message":{"role":"user","content":"Hello"}}"#,
        ]);

        let parsed = parse_session(file.path()).unwrap();
        assert_eq!(parsed.messages.len(), 1);
        assert_eq!(parsed.message_count, 1);
        assert_eq!(parsed.messages[0].role, "user");
    }

    #[test]
    fn test_parse_first_prompt_skips_empty_content() {
        let file = write_jsonl(&[
            r#"{"timestamp":"2024-01-01T00:00:00Z","message":{"role":"user","content":""}}"#,
            r#"{"timestamp":"2024-01-01T00:00:01Z","message":{"role":"user","content":"Real prompt"}}"#,
        ]);

        let parsed = parse_session(file.path()).unwrap();
        assert_eq!(parsed.first_prompt.as_deref(), Some("Real prompt"));
    }

    #[test]
    fn test_parse_summary_skips_empty_assistant() {
        let file = write_jsonl(&[
            r#"{"timestamp":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":""}}"#,
            r#"{"timestamp":"2024-01-01T00:00:01Z","message":{"role":"assistant","content":"Real summary"}}"#,
        ]);

        let parsed = parse_session(file.path()).unwrap();
        assert_eq!(parsed.summary.as_deref(), Some("Real summary"));
    }

    #[test]
    fn test_parse_summary_not_overwritten_by_second_assistant() {
        let file = write_jsonl(&[
            r#"{"timestamp":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"First response"}}"#,
            r#"{"timestamp":"2024-01-01T00:00:01Z","message":{"role":"assistant","content":"Second response"}}"#,
        ]);

        let parsed = parse_session(file.path()).unwrap();
        assert_eq!(parsed.summary.as_deref(), Some("First response"));
    }

    #[test]
    fn test_parse_extract_content_non_standard_types() {
        // Content that is null/number/object should produce empty string
        let file = write_jsonl(&[
            r#"{"timestamp":"2024-01-01T00:00:00Z","message":{"role":"user","content":null}}"#,
        ]);

        let parsed = parse_session(file.path()).unwrap();
        assert_eq!(parsed.messages.len(), 1);
        assert_eq!(parsed.messages[0].content, "");
    }

    #[test]
    fn test_parse_summary_from_first_assistant() {
        let long_text = "A".repeat(300);
        let line = format!(
            r#"{{"timestamp":"2024-01-01T00:00:00Z","message":{{"role":"assistant","content":"{long_text}"}}}}"#
        );
        let file = write_jsonl(&[&line]);

        let parsed = parse_session(file.path()).unwrap();

        assert!(parsed.summary.is_some());
        let summary = parsed.summary.unwrap();
        assert!(summary.len() <= 200, "summary length {} > 200", summary.len());
        assert!(summary.ends_with("..."));
    }
}
