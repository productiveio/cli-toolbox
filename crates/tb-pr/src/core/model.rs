use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Which column a PR lives in on the kanban board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Column {
    DraftMine,
    ReviewMine,
    ReadyToMergeMine,
    WaitingOnMe,
    WaitingOnAuthor,
}

impl Column {
    pub fn slug(self) -> &'static str {
        match self {
            Column::DraftMine => "draft_mine",
            Column::ReviewMine => "review_mine",
            Column::ReadyToMergeMine => "ready_to_merge_mine",
            Column::WaitingOnMe => "waiting_on_me",
            Column::WaitingOnAuthor => "waiting_on_author",
        }
    }

    /// Resolve a user-supplied column name (dashes, underscores, case-insensitive) to the enum.
    pub fn parse(s: &str) -> Option<Self> {
        let norm = s.to_ascii_lowercase().replace('-', "_");
        match norm.as_str() {
            "draft_mine" | "draft" => Some(Column::DraftMine),
            "review_mine" | "review" | "in_review" => Some(Column::ReviewMine),
            "ready_to_merge_mine" | "ready" | "ready_merge" | "ready_to_merge" => {
                Some(Column::ReadyToMergeMine)
            }
            "waiting_on_me" => Some(Column::WaitingOnMe),
            "waiting_on_author" => Some(Column::WaitingOnAuthor),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum SizeBucket {
    Xs,
    S,
    M,
    L,
    Xl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RottingBucket {
    Fresh,
    Warming,
    Stale,
    Rotting,
    Critical,
}

/// The PR state relevant to a card. For M2 this is derived from `draft`
/// only; M3 refines `Ready` vs `Approved` using the reviews API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrState {
    Draft,
    Ready,
    Approved,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pr {
    pub number: u64,
    pub repo: String,
    pub title: String,
    pub url: String,
    pub author: String,
    pub state: PrState,
    pub created_at: DateTime<Utc>,
    pub age_days: f64,
    pub size: Option<SizeBucket>,
    pub rotting: RottingBucket,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub productive_task_id: Option<String>,
    pub comments_count: u64,
    pub base_branch: Option<String>,
    /// Only set for PRs in the `waiting_on_author` column — true if the
    /// author has pushed commits since the viewer's last review.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_new_commits_since_my_review: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnsData {
    pub draft_mine: Vec<Pr>,
    pub review_mine: Vec<Pr>,
    pub ready_to_merge_mine: Vec<Pr>,
    pub waiting_on_me: Vec<Pr>,
    pub waiting_on_author: Vec<Pr>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardState {
    pub user: String,
    pub fetched_at: DateTime<Utc>,
    pub columns: ColumnsData,
}
