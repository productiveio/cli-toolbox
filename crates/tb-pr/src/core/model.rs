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
    /// Unread GitHub notifications on PRs in the configured org — mentions,
    /// comments, review threads. Not strictly a PR column, but rendered
    /// alongside for the email-replacement workflow.
    Mentions,
}

impl Column {
    pub fn slug(self) -> &'static str {
        match self {
            Column::DraftMine => "draft_mine",
            Column::ReviewMine => "review_mine",
            Column::ReadyToMergeMine => "ready_to_merge_mine",
            Column::WaitingOnMe => "waiting_on_me",
            Column::WaitingOnAuthor => "waiting_on_author",
            Column::Mentions => "mentions",
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
            "mentions" | "inbox" | "notifications" => Some(Column::Mentions),
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

/// Rolled-up CI status for a PR's head commit. `None` on the `Pr` struct
/// means "no CI configured" (or fetch failed) — render nothing, don't lie.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckState {
    Success,
    Failure,
    Pending,
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
    /// Rolled-up CI status on the head commit. Populated only for my own
    /// PRs (draft/review/ready-to-merge) — where CI failures are
    /// actionable — and left `None` on everyone else's to keep fetch cost
    /// bounded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check_state: Option<CheckState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnsData {
    pub draft_mine: Vec<Pr>,
    pub review_mine: Vec<Pr>,
    pub ready_to_merge_mine: Vec<Pr>,
    pub waiting_on_me: Vec<Pr>,
    pub waiting_on_author: Vec<Pr>,
    /// Unread notifications filtered to `subject.type == PullRequest` in the
    /// configured org. Populated only when the notifications fetch succeeds;
    /// a failure there must not tank the rest of the board.
    #[serde(default)]
    pub notifications: Vec<Notification>,
}

/// Why a notification landed in the user's inbox. Mirrors GitHub's `reason`
/// strings; unknown values fall through as `Other(raw)` so we don't lose info
/// when GitHub adds new reasons.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationReason {
    Mention,
    TeamMention,
    Comment,
    ReviewRequested,
    Author,
    StateChange,
    Subscribed,
    Other(String),
}

impl NotificationReason {
    pub fn from_api(raw: &str) -> Self {
        match raw {
            "mention" => Self::Mention,
            "team_mention" => Self::TeamMention,
            "comment" => Self::Comment,
            "review_requested" => Self::ReviewRequested,
            "author" => Self::Author,
            "state_change" => Self::StateChange,
            "subscribed" => Self::Subscribed,
            other => Self::Other(other.to_string()),
        }
    }

    pub fn short_label(&self) -> &str {
        match self {
            Self::Mention => "@mention",
            Self::TeamMention => "@team",
            Self::Comment => "comment",
            Self::ReviewRequested => "review-req",
            Self::Author => "author",
            Self::StateChange => "state",
            Self::Subscribed => "subscribed",
            Self::Other(s) => s,
        }
    }
}

/// A single unread GitHub notification pointing at a PR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    /// Stable thread id used for mark-as-read (PATCH /notifications/threads/:id).
    pub thread_id: String,
    pub reason: NotificationReason,
    pub owner: String,
    pub repo: String,
    pub pr_number: u64,
    pub pr_title: String,
    /// Web URL to the PR (synthesized, not fetched).
    pub pr_url: String,
    pub updated_at: DateTime<Utc>,
    pub age_days: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardState {
    pub user: String,
    pub fetched_at: DateTime<Utc>,
    pub columns: ColumnsData,
}
