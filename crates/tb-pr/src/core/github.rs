use std::collections::{HashMap, HashSet};
use std::process::Command;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::core::classifier;
use crate::core::model::{BoardState, Column, ColumnsData, Pr, PrState};
use crate::core::productive::extract_task_id;
use crate::core::reviews::{Review, ReviewSummary};
use crate::error::{Error, Result};

const API_BASE: &str = "https://api.github.com";
const USER_AGENT: &str = concat!("tb-pr/", env!("CARGO_PKG_VERSION"));
const SEARCH_PER_PAGE: u32 = 100;

#[derive(Clone)]
pub struct GhClient {
    client: reqwest::Client,
    token: String,
}

impl GhClient {
    pub fn new() -> Result<Self> {
        let token = gh_auth_token()?;
        let client = reqwest::Client::builder().user_agent(USER_AGENT).build()?;
        Ok(Self { client, token })
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        let resp = self
            .client
            .get(url)
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            // Detect rate limiting: GitHub returns 403 with X-RateLimit-Remaining=0
            // (and 429 with a Retry-After header). Surface a friendlier message so
            // the TUI error banner and doctor output are actionable.
            let remaining = resp
                .headers()
                .get("x-ratelimit-remaining")
                .and_then(|h| h.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());
            let reset = resp
                .headers()
                .get("x-ratelimit-reset")
                .and_then(|h| h.to_str().ok())
                .and_then(|v| v.parse::<i64>().ok());
            let raw = resp.text().await.unwrap_or_default();
            let message = if status.as_u16() == 429 || remaining == Some(0) {
                let when = reset
                    .and_then(|ts| chrono::DateTime::<Utc>::from_timestamp(ts, 0))
                    .map(|t| format!(" — resets at {}", t.format("%H:%M:%S UTC")))
                    .unwrap_or_default();
                format!("rate limited{when}. Run `tb-pr refresh` later or widen TTL.")
            } else {
                raw
            };
            return Err(Error::Api {
                status: status.as_u16(),
                message,
            });
        }
        Ok(resp.json::<T>().await?)
    }

    /// Lightweight GET against `/orgs/{org}` — used by `doctor` to verify
    /// that the token has org visibility.
    pub async fn probe_org(&self, org: &str) -> Result<()> {
        // We only care about success; parse into Value to avoid a throwaway struct.
        let _: serde_json::Value = self.get(&format!("{API_BASE}/orgs/{org}")).await?;
        Ok(())
    }

    pub async fn user_login(&self) -> Result<String> {
        #[derive(Deserialize)]
        struct User {
            login: String,
        }
        let u: User = self.get(&format!("{API_BASE}/user")).await?;
        Ok(u.login)
    }

    pub async fn search_issues(&self, query: &str) -> Result<Vec<SearchItem>> {
        let mut url = reqwest::Url::parse(&format!("{API_BASE}/search/issues")).unwrap();
        url.query_pairs_mut()
            .append_pair("q", query)
            .append_pair("per_page", &SEARCH_PER_PAGE.to_string());
        let resp: SearchResponse = self.get(url.as_str()).await?;
        Ok(resp.items)
    }

    pub async fn pull_detail(&self, owner: &str, repo: &str, number: u64) -> Result<PullDetail> {
        self.get(&format!("{API_BASE}/repos/{owner}/{repo}/pulls/{number}"))
            .await
    }

    pub async fn pull_reviews(&self, owner: &str, repo: &str, number: u64) -> Result<Vec<Review>> {
        self.get(&format!(
            "{API_BASE}/repos/{owner}/{repo}/pulls/{number}/reviews?per_page=100"
        ))
        .await
    }

    pub async fn commit_date(&self, owner: &str, repo: &str, sha: &str) -> Result<DateTime<Utc>> {
        let commit: CommitResponse = self
            .get(&format!("{API_BASE}/repos/{owner}/{repo}/commits/{sha}"))
            .await?;
        Ok(commit.commit.committer.date)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SearchItem {
    pub number: u64,
    pub title: String,
    pub html_url: String,
    pub user: SearchUser,
    #[serde(default)]
    pub body: Option<String>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub comments: u64,
    pub repository_url: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SearchUser {
    pub login: String,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    items: Vec<SearchItem>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PullDetail {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub user: Option<SearchUser>,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub comments: u64,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub additions: u64,
    #[serde(default)]
    pub deletions: u64,
    pub base: BaseRef,
    pub head: HeadRef,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BaseRef {
    #[serde(rename = "ref")]
    pub branch: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HeadRef {
    pub sha: String,
}

#[derive(Debug, Deserialize)]
struct CommitResponse {
    commit: CommitInner,
}

#[derive(Debug, Deserialize)]
struct CommitInner {
    committer: CommitActor,
}

#[derive(Debug, Deserialize)]
struct CommitActor {
    date: DateTime<Utc>,
}

fn gh_auth_token() -> Result<String> {
    let output = Command::new("gh")
        .args(["auth", "token"])
        .output()
        .map_err(|e| Error::Other(format!("failed to run `gh auth token`: {e}")))?;
    if !output.status.success() {
        return Err(Error::Other(
            "`gh auth token` failed — run `gh auth login`".to_string(),
        ));
    }
    let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if token.is_empty() {
        return Err(Error::Other(
            "`gh auth token` returned an empty token".to_string(),
        ));
    }
    Ok(token)
}

/// Parse a repository_url like `https://api.github.com/repos/productiveio/ai-agent`
/// into `(owner, repo)`.
pub fn parse_repo_url(url: &str) -> Option<(String, String)> {
    let suffix = url.strip_prefix("https://api.github.com/repos/")?;
    let mut parts = suffix.splitn(2, '/');
    let owner = parts.next()?.to_string();
    let repo = parts.next()?.to_string();
    if repo.is_empty() {
        return None;
    }
    Some((owner, repo))
}

/// Fetch the full kanban state: 4 search queries in parallel, then per-PR
/// details + reviews + head commit dates in parallel. Applies M3 filters:
///
/// - `review_mine` excludes PRs already fully approved (those become
///   `ready_to_merge_mine`).
/// - `waiting_on_author` keeps only PRs where the viewer's last review is
///   COMMENTED or CHANGES_REQUESTED and flags PRs where the author has
///   pushed new commits since that review.
pub async fn fetch_board_state(
    client: &GhClient,
    org: &str,
    productive_org_slug: &str,
    username_override: Option<&str>,
) -> Result<BoardState> {
    let user = match username_override {
        Some(u) if !u.is_empty() => u.to_string(),
        _ => client.user_login().await?,
    };

    let q_draft = format!("is:pr is:open draft:true author:@me org:{org}");
    let q_author_mine = format!("is:pr is:open draft:false author:@me org:{org}");
    // user-review-requested (not review-requested) — the former is
    // direct-only; the latter also matches PRs where a team I'm on was
    // requested as codeowner, which I don't want on the board.
    let q_wait_me = format!("is:pr is:open user-review-requested:@me org:{org}");
    let q_wait_author = format!("is:pr is:open reviewed-by:@me -author:@me org:{org}");

    let (r_draft, r_review_mine_raw, r_wait_me, r_wait_author_raw) = tokio::try_join!(
        client.search_issues(&q_draft),
        client.search_issues(&q_author_mine),
        client.search_issues(&q_wait_me),
        client.search_issues(&q_wait_author),
    )?;

    let details = fetch_all_details(
        client,
        [&r_draft, &r_review_mine_raw, &r_wait_me, &r_wait_author_raw],
    )
    .await?;

    // Reviews: we only need them for author-mine (to split ready-to-merge)
    // and waiting-on-author (to filter + flag new commits).
    let review_keys = collect_keys(&[&r_review_mine_raw, &r_wait_author_raw])?;
    let reviews = fetch_all_reviews(client, review_keys).await?;

    // Head commit dates: only needed for waiting-on-author.
    let commit_keys = collect_commit_keys(&r_wait_author_raw, &details)?;
    let commit_dates = fetch_all_commit_dates(client, commit_keys).await?;

    let now = Utc::now();

    // Draft + waiting-on-me are straight pass-throughs.
    let draft_mine: Vec<Pr> = r_draft
        .iter()
        .map(|item| build_pr(item, Column::DraftMine, &details, productive_org_slug, now))
        .collect();
    let waiting_on_me: Vec<Pr> = r_wait_me
        .iter()
        .map(|item| {
            build_pr(
                item,
                Column::WaitingOnMe,
                &details,
                productive_org_slug,
                now,
            )
        })
        .collect();

    // Split author-mine into ready_to_merge vs review_mine using reviews.
    let mut review_mine = Vec::new();
    let mut ready_to_merge_mine = Vec::new();
    for item in &r_review_mine_raw {
        let (_, repo, number) = repo_key(item)?;
        let lookup = (repo, number);
        let summary = reviews
            .get(&lookup)
            .map(|r| ReviewSummary::from_reviews(r))
            .unwrap_or_else(|| ReviewSummary::from_reviews(&[]));
        if summary.is_ready_to_merge() {
            let mut pr = build_pr(
                item,
                Column::ReadyToMergeMine,
                &details,
                productive_org_slug,
                now,
            );
            pr.state = PrState::Approved;
            ready_to_merge_mine.push(pr);
        } else {
            review_mine.push(build_pr(
                item,
                Column::ReviewMine,
                &details,
                productive_org_slug,
                now,
            ));
        }
    }

    // Filter waiting-on-author: keep only where my last review is
    // COMMENTED/CHANGES_REQUESTED; flag 🆕 when author pushed since.
    let mut waiting_on_author = Vec::new();
    for item in &r_wait_author_raw {
        let (_, repo, number) = repo_key(item)?;
        let lookup = (repo, number);
        let summary = reviews
            .get(&lookup)
            .map(|r| ReviewSummary::from_reviews(r))
            .unwrap_or_else(|| ReviewSummary::from_reviews(&[]));
        let Some(my_review) = summary.my_latest_review(&user) else {
            continue;
        };
        let state_upper = my_review.state.to_ascii_uppercase();
        if state_upper != "COMMENTED" && state_upper != "CHANGES_REQUESTED" {
            continue;
        }
        let head_date = commit_dates.get(&lookup).copied();
        let has_new_commits = match (head_date, my_review.submitted_at) {
            (Some(h), Some(r)) => Some(h > r),
            _ => None,
        };
        let mut pr = build_pr(
            item,
            Column::WaitingOnAuthor,
            &details,
            productive_org_slug,
            now,
        );
        pr.has_new_commits_since_my_review = has_new_commits;
        waiting_on_author.push(pr);
    }

    let columns = ColumnsData {
        draft_mine,
        review_mine,
        ready_to_merge_mine,
        waiting_on_me,
        waiting_on_author,
    };

    Ok(BoardState {
        user,
        fetched_at: now,
        columns,
    })
}

/// `(owner, repo, number)` from a search item.
fn repo_key(item: &SearchItem) -> Result<(String, String, u64)> {
    let (owner, repo) = parse_repo_url(&item.repository_url).ok_or_else(|| {
        Error::Other(format!("malformed repository_url: {}", item.repository_url))
    })?;
    Ok((owner, repo, item.number))
}

fn collect_keys(lists: &[&Vec<SearchItem>]) -> Result<HashSet<(String, String, u64)>> {
    let mut keys = HashSet::new();
    for list in lists {
        for item in *list {
            keys.insert(repo_key(item)?);
        }
    }
    Ok(keys)
}

fn collect_commit_keys(
    items: &[SearchItem],
    details: &HashMap<(String, u64), PullDetail>,
) -> Result<Vec<(String, String, u64, String)>> {
    let mut out = Vec::new();
    for item in items {
        let (owner, repo, number) = repo_key(item)?;
        if let Some(detail) = details.get(&(repo.clone(), number)) {
            out.push((owner, repo, number, detail.head.sha.clone()));
        }
    }
    Ok(out)
}

async fn fetch_all_reviews(
    client: &GhClient,
    keys: HashSet<(String, String, u64)>,
) -> Result<HashMap<(String, u64), Vec<Review>>> {
    let mut set = tokio::task::JoinSet::new();
    for (owner, repo, number) in keys {
        let c = client.clone();
        set.spawn(async move {
            let res = c.pull_reviews(&owner, &repo, number).await;
            ((repo, number), res)
        });
    }
    let mut out = HashMap::new();
    while let Some(joined) = set.join_next().await {
        let (key, res) = joined.map_err(|e| Error::Other(e.to_string()))?;
        out.insert(key, res?);
    }
    Ok(out)
}

async fn fetch_all_commit_dates(
    client: &GhClient,
    keys: Vec<(String, String, u64, String)>,
) -> Result<HashMap<(String, u64), DateTime<Utc>>> {
    let mut set = tokio::task::JoinSet::new();
    for (owner, repo, number, sha) in keys {
        let c = client.clone();
        set.spawn(async move {
            let res = c.commit_date(&owner, &repo, &sha).await;
            ((repo, number), res)
        });
    }
    let mut out = HashMap::new();
    while let Some(joined) = set.join_next().await {
        let (key, res) = joined.map_err(|e| Error::Other(e.to_string()))?;
        out.insert(key, res?);
    }
    Ok(out)
}

/// Dedupe across columns and fetch each PR's detail endpoint in parallel.
/// Key is `(repo, number)` — all PRs are scoped to the same org.
async fn fetch_all_details(
    client: &GhClient,
    lists: [&Vec<SearchItem>; 4],
) -> Result<HashMap<(String, u64), PullDetail>> {
    let mut keys: HashSet<(String, String, u64)> = HashSet::new();
    for list in lists {
        for item in list {
            let (owner, repo) = parse_repo_url(&item.repository_url).ok_or_else(|| {
                Error::Other(format!("malformed repository_url: {}", item.repository_url))
            })?;
            keys.insert((owner, repo, item.number));
        }
    }

    let mut set = tokio::task::JoinSet::new();
    for (owner, repo, number) in keys {
        let c = client.clone();
        set.spawn(async move {
            let res = c.pull_detail(&owner, &repo, number).await;
            ((repo, number), res)
        });
    }

    let mut details = HashMap::new();
    while let Some(joined) = set.join_next().await {
        let (key, res) = joined.map_err(|e| Error::Other(e.to_string()))?;
        details.insert(key, res?);
    }
    Ok(details)
}

fn build_pr(
    item: &SearchItem,
    col: Column,
    details: &HashMap<(String, u64), PullDetail>,
    productive_org_slug: &str,
    now: DateTime<Utc>,
) -> Pr {
    let repo = parse_repo_url(&item.repository_url)
        .map(|(_, r)| r)
        .unwrap_or_default();
    let detail = details.get(&(repo.clone(), item.number));
    let size = detail.map(|d| classifier::size_bucket(d.additions, d.deletions));
    let base_branch = detail.map(|d| d.base.branch.clone());

    let age_hours = (now - item.created_at).num_seconds() as f64 / 3600.0;
    let age_days = age_hours / 24.0;

    let body = item.body.as_deref().unwrap_or("");
    let productive_task_id = extract_task_id(body, productive_org_slug);

    let state = if item.draft {
        PrState::Draft
    } else {
        PrState::Ready
    };

    Pr {
        number: item.number,
        repo,
        title: item.title.clone(),
        url: item.html_url.clone(),
        author: item.user.login.clone(),
        state,
        created_at: item.created_at,
        age_days,
        size,
        rotting: classifier::rotting_bucket(col, age_hours),
        productive_task_id,
        comments_count: item.comments,
        base_branch,
        has_new_commits_since_my_review: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_repo_url() {
        assert_eq!(
            parse_repo_url("https://api.github.com/repos/productiveio/ai-agent"),
            Some(("productiveio".into(), "ai-agent".into()))
        );
    }

    #[test]
    fn rejects_bad_repo_url() {
        assert_eq!(parse_repo_url("https://example.com/repos/a/b"), None);
        assert_eq!(
            parse_repo_url("https://api.github.com/repos/owner-only"),
            None
        );
    }
}
