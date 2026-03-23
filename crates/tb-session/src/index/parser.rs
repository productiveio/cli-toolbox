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

pub fn parse_session(_file_path: &Path) -> Result<ParsedSession> {
    Ok(ParsedSession {
        summary: None,
        first_prompt: None,
        git_branch: None,
        message_count: 0,
        created_at: None,
        modified_at: None,
        is_sidechain: false,
        messages: vec![],
    })
}
