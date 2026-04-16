use std::collections::{HashMap, HashSet};
use std::process::Command;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;

use crate::core::classifier;
use crate::core::model::{
    BoardState, CheckState, Column, ColumnsData, Notification, NotificationReason, Pr, PrState,
};
use crate::core::productive::extract_task_id;
use crate::core::reviews::{Review, ReviewSummary};
use crate::error::{Error, Result};

const API_BASE: &str = "https://api.github.com";
const USER_AGENT: &str = concat!("tb-pr/", env!("CARGO_PKG_VERSION"));
const SEARCH_PER_PAGE: u32 = 100;
/// Hard cap on search pagination. GitHub's search API refuses to return
/// results beyond 1000 anyway, so 10 pages of 100 is the practical ceiling.
const SEARCH_MAX_PAGES: u32 = 10;
/// Upper bound on concurrent per-PR API calls (details, reviews, commit
/// dates). GitHub's secondary rate limiter kicks in around ~100 simultaneous
/// requests; 16 is comfortably below that while still parallel enough to
/// keep `refresh` snappy.
const MAX_CONCURRENT: usize = 16;

/// Full key for a PR across the org: `(owner, repo, number)`. Using only
/// `(repo, number)` was enough while we scope to `org:productiveio`, but
/// consistently including `owner` keeps the code honest for any future case
/// where the config points elsewhere or a mirror-forked repo shares a name.
type PrKey = (String, String, u64);

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

    /// Issue a GET and return the raw response (status already checked).
    async fn send_get(&self, url: &str) -> Result<reqwest::Response> {
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
        Ok(resp)
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        let resp = self.send_get(url).await?;
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

    /// Run a GitHub search, following `Link: rel="next"` until all pages are
    /// exhausted or `SEARCH_MAX_PAGES` is reached. Without this loop the
    /// board silently truncates at 100 items per column — a real problem for
    /// `reviewed-by:@me` across an active org.
    pub async fn search_issues(&self, query: &str) -> Result<Vec<SearchItem>> {
        let mut first = reqwest::Url::parse(&format!("{API_BASE}/search/issues")).unwrap();
        first
            .query_pairs_mut()
            .append_pair("q", query)
            .append_pair("per_page", &SEARCH_PER_PAGE.to_string());

        let mut next_url: Option<String> = Some(first.to_string());
        let mut items: Vec<SearchItem> = Vec::new();
        let mut pages = 0u32;
        while let Some(url) = next_url.take() {
            pages += 1;
            if pages > SEARCH_MAX_PAGES {
                break;
            }
            let resp = self.send_get(&url).await?;
            next_url = parse_link_next(resp.headers());
            let page: SearchResponse = resp.json().await?;
            items.extend(page.items);
        }
        Ok(items)
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

    /// Fetch GitHub Actions check-runs for a commit SHA and roll them up.
    /// Returns `None` when there are no runs (no CI configured) so callers
    /// can skip rendering instead of showing an empty pending state.
    pub async fn check_rollup(
        &self,
        owner: &str,
        repo: &str,
        sha: &str,
    ) -> Result<Option<CheckState>> {
        let resp: CheckRunsResponse = self
            .get(&format!(
                "{API_BASE}/repos/{owner}/{repo}/commits/{sha}/check-runs?per_page=100"
            ))
            .await?;
        Ok(rollup_check_runs(&resp.check_runs))
    }

    /// Fetch unread notifications, filtered to PRs in the given org. Paginates
    /// via `Link: rel="next"` up to the same 10-page cap used for searches.
    pub async fn list_notifications(&self, org: &str) -> Result<Vec<Notification>> {
        let mut first = reqwest::Url::parse(&format!("{API_BASE}/notifications")).unwrap();
        first
            .query_pairs_mut()
            .append_pair("per_page", "50")
            .append_pair("all", "false"); // unread only

        let mut next_url: Option<String> = Some(first.to_string());
        let mut out: Vec<Notification> = Vec::new();
        let mut pages = 0u32;
        while let Some(url) = next_url.take() {
            pages += 1;
            if pages > SEARCH_MAX_PAGES {
                break;
            }
            let resp = self.send_get(&url).await?;
            next_url = parse_link_next(resp.headers());
            let page: Vec<ApiNotification> = resp.json().await?;
            for item in page {
                if let Some(n) = into_pr_notification(item, org) {
                    out.push(n);
                }
            }
        }
        Ok(out)
    }

    /// Resolve a `latest_comment_url` (API) to its `html_url` (web). Used at
    /// open-time so clicking a notification jumps to the specific comment.
    pub async fn resolve_comment_html_url(&self, api_url: &str) -> Result<String> {
        #[derive(Deserialize)]
        struct CommentStub {
            html_url: String,
        }
        let c: CommentStub = self.get(api_url).await?;
        Ok(c.html_url)
    }

    /// Mark a single notification thread as read.
    pub async fn mark_thread_read(&self, thread_id: &str) -> Result<()> {
        let url = format!("{API_BASE}/notifications/threads/{thread_id}");
        let resp = self
            .client
            .patch(&url)
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Api {
                status: status.as_u16(),
                message: body,
            });
        }
        Ok(())
    }

    /// Mark *all* notifications as read at the server. GitHub accepts an
    /// optional `last_read_at`; we send `now` so earlier items are cleared
    /// but a brand-new notification arriving mid-request is still surfaced.
    pub async fn mark_all_notifications_read(&self) -> Result<()> {
        let url = format!("{API_BASE}/notifications");
        let body = serde_json::json!({ "last_read_at": Utc::now().to_rfc3339() });
        let resp = self
            .client
            .put(&url)
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .json(&body)
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(Error::Api {
                status: status.as_u16(),
                message: text,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct ApiNotification {
    id: String,
    reason: String,
    updated_at: DateTime<Utc>,
    subject: ApiSubject,
    repository: ApiRepository,
}

#[derive(Debug, Deserialize)]
struct ApiSubject {
    title: String,
    url: Option<String>,
    latest_comment_url: Option<String>,
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Debug, Deserialize)]
struct ApiRepository {
    name: String,
    owner: ApiOwner,
}

#[derive(Debug, Deserialize)]
struct ApiOwner {
    login: String,
}

/// Reasons we surface in the Mentions inbox. Excludes `review_requested`
/// (already its own "Waiting on me" column) and `state_change` /
/// `subscribed` (too noisy — every PR close/merge event).
const INBOX_REASONS: &[&str] = &["mention", "team_mention", "author", "comment"];

/// Shape the raw `/notifications` payload into our `Notification` struct.
/// Returns `None` when the thread is not a PR in the configured org, when
/// the reason isn't one we care about, or when we can't parse a PR number
/// out of it — those items are dropped silently.
fn into_pr_notification(item: ApiNotification, org: &str) -> Option<Notification> {
    if item.subject.kind != "PullRequest" {
        return None;
    }
    if !item.repository.owner.login.eq_ignore_ascii_case(org) {
        return None;
    }
    if !INBOX_REASONS.iter().any(|r| *r == item.reason) {
        return None;
    }
    let api_url = item.subject.url.as_deref()?;
    let pr_number = api_url
        .rsplit('/')
        .next()
        .and_then(|s| s.parse::<u64>().ok())?;
    let repo = item.repository.name;
    let pr_url = format!(
        "https://github.com/{}/{repo}/pull/{pr_number}",
        item.repository.owner.login
    );
    let age_days = (Utc::now() - item.updated_at).num_seconds() as f64 / 86400.0;
    Some(Notification {
        thread_id: item.id,
        reason: NotificationReason::from_api(&item.reason),
        repo,
        pr_number,
        pr_title: item.subject.title,
        pr_url,
        latest_comment_api_url: item.subject.latest_comment_url,
        updated_at: item.updated_at,
        age_days,
    })
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

#[derive(Debug, Deserialize)]
struct CheckRunsResponse {
    check_runs: Vec<CheckRun>,
}

#[derive(Debug, Deserialize)]
struct CheckRun {
    /// One of: queued | in_progress | completed | pending | waiting
    status: String,
    /// Only set when status=completed. One of: success | failure | neutral |
    /// cancelled | skipped | timed_out | action_required | stale
    #[serde(default)]
    conclusion: Option<String>,
}

/// Roll a list of check-runs into a single state: failure wins over pending
/// wins over success. Missing / skipped / neutral runs count as success so a
/// PR with only skipped checks renders as ✓ rather than ●.
fn rollup_check_runs(runs: &[CheckRun]) -> Option<CheckState> {
    if runs.is_empty() {
        return None;
    }
    let mut has_pending = false;
    let mut has_real_success = false;
    for run in runs {
        if run.status != "completed" {
            has_pending = true;
            continue;
        }
        match run.conclusion.as_deref() {
            Some("failure") | Some("timed_out") | Some("cancelled") | Some("action_required") => {
                return Some(CheckState::Failure);
            }
            Some("success") => has_real_success = true,
            Some("skipped") | Some("neutral") | Some("stale") => {}
            // Unknown or missing conclusion on a completed run — treat as
            // still-settling rather than silently success.
            _ => has_pending = true,
        }
    }
    if has_pending {
        return Some(CheckState::Pending);
    }
    if has_real_success {
        Some(CheckState::Success)
    } else {
        // Only skipped / neutral checks — nothing meaningful to surface.
        None
    }
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

/// Extract the `rel="next"` URL from a GitHub `Link` header, if present.
/// Header shape: `<https://…&page=2>; rel="next", <https://…&page=10>; rel="last"`.
fn parse_link_next(headers: &reqwest::header::HeaderMap) -> Option<String> {
    let link = headers.get("link")?.to_str().ok()?;
    for entry in link.split(',') {
        let (url_part, rel_part) = entry.split_once(';')?;
        if rel_part.contains("rel=\"next\"") {
            let trimmed = url_part.trim();
            let url = trimmed
                .strip_prefix('<')
                .and_then(|s| s.strip_suffix('>'))?;
            return Some(url.to_string());
        }
    }
    None
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

    let (r_draft, r_review_mine_raw, r_wait_me, r_wait_author_raw, r_notifications) = tokio::join!(
        client.search_issues(&q_draft),
        client.search_issues(&q_author_mine),
        client.search_issues(&q_wait_me),
        client.search_issues(&q_wait_author),
        client.list_notifications(org),
    );
    let r_draft = r_draft?;
    let r_review_mine_raw = r_review_mine_raw?;
    let r_wait_me = r_wait_me?;
    let r_wait_author_raw = r_wait_author_raw?;
    // Notifications are nice-to-have — a 403 on /notifications (e.g. token
    // missing `notifications` scope) must not block the main board.
    let notifications = match r_notifications {
        Ok(n) => n,
        Err(e) => {
            eprintln!("warning: failed to fetch notifications: {e}");
            Vec::new()
        }
    };

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

    // CI rollups: only fetch for MY PRs (drafts + author_mine, which splits
    // into review_mine and ready_to_merge_mine). I care when my own CI is
    // red; I don't care about the CI on PRs waiting on me or on the author.
    let check_keys = collect_commit_keys_for(&[&r_draft, &r_review_mine_raw], &details)?;
    let check_states = fetch_all_check_states(client, check_keys).await;

    let now = Utc::now();

    // Draft + waiting-on-me are straight pass-throughs.
    let draft_mine: Vec<Pr> = r_draft
        .iter()
        .map(|item| {
            let mut pr = build_pr(item, Column::DraftMine, &details, productive_org_slug, now);
            if let Ok(key) = repo_key(item) {
                pr.check_state = check_states.get(&key).copied().flatten();
            }
            pr
        })
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
        let key = repo_key(item)?;
        let summary = summarize(&reviews, &key);
        if summary.is_ready_to_merge() {
            let mut pr = build_pr(
                item,
                Column::ReadyToMergeMine,
                &details,
                productive_org_slug,
                now,
            );
            pr.state = PrState::Approved;
            pr.check_state = check_states.get(&key).copied().flatten();
            ready_to_merge_mine.push(pr);
        } else {
            let mut pr = build_pr(item, Column::ReviewMine, &details, productive_org_slug, now);
            pr.check_state = check_states.get(&key).copied().flatten();
            review_mine.push(pr);
        }
    }

    // Filter waiting-on-author: keep only where my last review is
    // COMMENTED/CHANGES_REQUESTED; flag 🆕 when author pushed since.
    let mut waiting_on_author = Vec::new();
    for item in &r_wait_author_raw {
        let key = repo_key(item)?;
        let summary = summarize(&reviews, &key);
        let Some(my_review) = summary.my_latest_review(&user) else {
            continue;
        };
        if !is_wait_on_author_state(&my_review.state) {
            continue;
        }
        let head_date = commit_dates.get(&key).copied();
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
        notifications,
    };

    Ok(BoardState {
        user,
        fetched_at: now,
        columns,
    })
}

/// `(owner, repo, number)` from a search item.
fn repo_key(item: &SearchItem) -> Result<PrKey> {
    let (owner, repo) = parse_repo_url(&item.repository_url).ok_or_else(|| {
        Error::Other(format!("malformed repository_url: {}", item.repository_url))
    })?;
    Ok((owner, repo, item.number))
}

fn collect_keys(lists: &[&Vec<SearchItem>]) -> Result<HashSet<PrKey>> {
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
    details: &HashMap<PrKey, PullDetail>,
) -> Result<Vec<(PrKey, String)>> {
    let mut out = Vec::new();
    for item in items {
        let key = repo_key(item)?;
        if let Some(detail) = details.get(&key) {
            out.push((key, detail.head.sha.clone()));
        }
    }
    Ok(out)
}

/// Build a `ReviewSummary` for `key`, tolerating missing entries (no reviews).
fn summarize(reviews: &HashMap<PrKey, Vec<Review>>, key: &PrKey) -> ReviewSummary {
    reviews
        .get(key)
        .map(|r| ReviewSummary::from_reviews(r))
        .unwrap_or_else(|| ReviewSummary::from_reviews(&[]))
}

/// Does this review state put the PR into the "waiting on author" column?
/// Approved / dismissed reviews don't — the author has no re-review to
/// wait for.
pub(crate) fn is_wait_on_author_state(state: &str) -> bool {
    let s = state.to_ascii_uppercase();
    s == "COMMENTED" || s == "CHANGES_REQUESTED"
}

async fn fetch_all_reviews(
    client: &GhClient,
    keys: HashSet<PrKey>,
) -> Result<HashMap<PrKey, Vec<Review>>> {
    let sem = Arc::new(Semaphore::new(MAX_CONCURRENT));
    let mut set = tokio::task::JoinSet::new();
    for key in keys {
        let c = client.clone();
        let sem = sem.clone();
        set.spawn(async move {
            let _permit = sem.acquire_owned().await;
            let (owner, repo, number) = &key;
            let res = c.pull_reviews(owner, repo, *number).await;
            (key, res)
        });
    }
    let mut out = HashMap::new();
    while let Some(joined) = set.join_next().await {
        let (key, res) = joined.map_err(|e| Error::Other(e.to_string()))?;
        out.insert(key, res?);
    }
    Ok(out)
}

/// Like `collect_commit_keys` but deduplicates across multiple search result
/// lists. Used by the CI rollup pass where we want draft+author-mine keys
/// combined with their head SHAs from the already-fetched details map.
fn collect_commit_keys_for(
    lists: &[&Vec<SearchItem>],
    details: &HashMap<PrKey, PullDetail>,
) -> Result<Vec<(PrKey, String)>> {
    let mut seen: HashSet<PrKey> = HashSet::new();
    let mut out = Vec::new();
    for list in lists {
        for item in *list {
            let key = repo_key(item)?;
            if !seen.insert(key.clone()) {
                continue;
            }
            if let Some(detail) = details.get(&key) {
                out.push((key, detail.head.sha.clone()));
            }
        }
    }
    Ok(out)
}

/// Fetch check-runs for each key and roll them up. Errors are swallowed into
/// `None` per PR — a single repo without Actions configured (or a 404 on a
/// SHA that's already force-pushed over) mustn't tank the whole refresh.
/// Returns a `HashMap<PrKey, Option<CheckState>>` so callers can distinguish
/// "no CI" from "CI not fetched".
async fn fetch_all_check_states(
    client: &GhClient,
    keys: Vec<(PrKey, String)>,
) -> HashMap<PrKey, Option<CheckState>> {
    let sem = Arc::new(Semaphore::new(MAX_CONCURRENT));
    let mut set = tokio::task::JoinSet::new();
    for (key, sha) in keys {
        let c = client.clone();
        let sem = sem.clone();
        set.spawn(async move {
            let _permit = sem.acquire_owned().await;
            let (owner, repo, _) = &key;
            let res = c.check_rollup(owner, repo, &sha).await.unwrap_or(None);
            (key, res)
        });
    }
    let mut out = HashMap::new();
    while let Some(joined) = set.join_next().await {
        if let Ok((key, state)) = joined {
            out.insert(key, state);
        }
    }
    out
}

async fn fetch_all_commit_dates(
    client: &GhClient,
    keys: Vec<(PrKey, String)>,
) -> Result<HashMap<PrKey, DateTime<Utc>>> {
    let sem = Arc::new(Semaphore::new(MAX_CONCURRENT));
    let mut set = tokio::task::JoinSet::new();
    for (key, sha) in keys {
        let c = client.clone();
        let sem = sem.clone();
        set.spawn(async move {
            let _permit = sem.acquire_owned().await;
            let (owner, repo, number) = &key;
            let res = c.commit_date(owner, repo, &sha).await;
            ((owner.clone(), repo.clone(), *number), res)
        });
    }
    let mut out = HashMap::new();
    while let Some(joined) = set.join_next().await {
        let (key, res) = joined.map_err(|e| Error::Other(e.to_string()))?;
        out.insert(key, res?);
    }
    Ok(out)
}

/// Dedupe across columns and fetch each PR's detail endpoint in parallel,
/// bounded by a semaphore to respect GitHub's secondary rate limits.
async fn fetch_all_details(
    client: &GhClient,
    lists: [&Vec<SearchItem>; 4],
) -> Result<HashMap<PrKey, PullDetail>> {
    let mut keys: HashSet<PrKey> = HashSet::new();
    for list in lists {
        for item in list {
            keys.insert(repo_key(item)?);
        }
    }

    let sem = Arc::new(Semaphore::new(MAX_CONCURRENT));
    let mut set = tokio::task::JoinSet::new();
    for key in keys {
        let c = client.clone();
        let sem = sem.clone();
        set.spawn(async move {
            let _permit = sem.acquire_owned().await;
            let (owner, repo, number) = &key;
            let res = c.pull_detail(owner, repo, *number).await;
            (key, res)
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
    details: &HashMap<PrKey, PullDetail>,
    productive_org_slug: &str,
    now: DateTime<Utc>,
) -> Pr {
    let (owner, repo) = parse_repo_url(&item.repository_url).unwrap_or_default();
    let detail = details.get(&(owner, repo.clone(), item.number));
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
        check_state: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(status: &str, conclusion: Option<&str>) -> CheckRun {
        CheckRun {
            status: status.to_string(),
            conclusion: conclusion.map(|s| s.to_string()),
        }
    }

    #[test]
    fn rollup_empty_returns_none() {
        assert!(rollup_check_runs(&[]).is_none());
    }

    #[test]
    fn rollup_failure_wins_over_everything() {
        let runs = vec![
            run("completed", Some("success")),
            run("in_progress", None),
            run("completed", Some("failure")),
        ];
        assert_eq!(rollup_check_runs(&runs), Some(CheckState::Failure));
    }

    #[test]
    fn rollup_pending_when_any_in_progress() {
        let runs = vec![run("completed", Some("success")), run("in_progress", None)];
        assert_eq!(rollup_check_runs(&runs), Some(CheckState::Pending));
    }

    #[test]
    fn rollup_success_when_all_green() {
        let runs = vec![
            run("completed", Some("success")),
            run("completed", Some("success")),
            run("completed", Some("skipped")),
        ];
        assert_eq!(rollup_check_runs(&runs), Some(CheckState::Success));
    }

    #[test]
    fn rollup_all_skipped_is_none() {
        // A PR whose only checks skip (e.g., doc-only paths filtered out)
        // shouldn't show a false ✓ — there's nothing to report.
        let runs = vec![
            run("completed", Some("skipped")),
            run("completed", Some("neutral")),
        ];
        assert!(rollup_check_runs(&runs).is_none());
    }

    #[test]
    fn rollup_timed_out_counts_as_failure() {
        let runs = vec![
            run("completed", Some("timed_out")),
            run("completed", Some("success")),
        ];
        assert_eq!(rollup_check_runs(&runs), Some(CheckState::Failure));
    }

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

    #[test]
    fn wait_on_author_state_only_matches_commented_or_changes_requested() {
        assert!(is_wait_on_author_state("COMMENTED"));
        assert!(is_wait_on_author_state("CHANGES_REQUESTED"));
        assert!(is_wait_on_author_state("commented")); // case-insensitive
        assert!(!is_wait_on_author_state("APPROVED"));
        assert!(!is_wait_on_author_state("DISMISSED"));
        assert!(!is_wait_on_author_state("PENDING"));
        assert!(!is_wait_on_author_state(""));
    }

    #[test]
    fn parse_link_next_extracts_next_url() {
        use reqwest::header::{HeaderMap, HeaderValue};
        let mut h = HeaderMap::new();
        h.insert(
            "link",
            HeaderValue::from_static(
                "<https://api.github.com/search/issues?page=2>; rel=\"next\", \
                 <https://api.github.com/search/issues?page=5>; rel=\"last\"",
            ),
        );
        assert_eq!(
            parse_link_next(&h).as_deref(),
            Some("https://api.github.com/search/issues?page=2")
        );

        // On the last page GitHub omits `next`.
        let mut h2 = HeaderMap::new();
        h2.insert(
            "link",
            HeaderValue::from_static(
                "<https://api.github.com/search/issues?page=1>; rel=\"first\", \
                 <https://api.github.com/search/issues?page=4>; rel=\"prev\"",
            ),
        );
        assert_eq!(parse_link_next(&h2), None);

        // No Link header at all.
        assert_eq!(parse_link_next(&HeaderMap::new()), None);
    }
}
