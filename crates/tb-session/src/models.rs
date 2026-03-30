use serde::Serialize;

/// Top-level response for the `search` command.
#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filters: Option<SearchFilters>,
    pub total_results: usize,
    pub results: Vec<SessionMatch>,
}

/// Active filters applied to a search.
#[derive(Debug, Serialize)]
pub struct SearchFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    pub all_projects: bool,
}

/// A single session that matched a search query.
#[derive(Debug, Serialize)]
pub struct SessionMatch {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    pub message_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
    pub relevance_score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_snippet: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_role: Option<String>,
}

/// Detailed view of a single session (for `show` command).
#[derive(Debug, Serialize)]
pub struct SessionDetail {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    pub message_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
    pub is_sidechain: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub messages: Option<Vec<MessagePreview>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_messages: Option<usize>,
}

/// A single message preview within a session.
#[derive(Debug, Serialize)]
pub struct MessagePreview {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

/// Top-level response for the `list` command.
#[derive(Debug, Serialize)]
pub struct SessionList {
    pub total_results: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<usize>,
    pub results: Vec<SessionSummary>,
}

/// Summary of a session in list output.
#[derive(Debug, Serialize)]
pub struct SessionSummary {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    pub message_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
}
