use std::collections::{HashMap, HashSet};
use std::process::Command;

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::core::classifier;
use crate::core::model::{BoardState, Column, ColumnsData, Pr, PrState};
use crate::core::productive::extract_task_id;
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
            let message = resp.text().await.unwrap_or_default();
            return Err(Error::Api {
                status: status.as_u16(),
                message,
            });
        }
        Ok(resp.json::<T>().await?)
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
}

#[derive(Debug, Deserialize, Clone)]
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

#[derive(Debug, Deserialize, Clone)]
pub struct SearchUser {
    pub login: String,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    items: Vec<SearchItem>,
}

#[derive(Debug, Deserialize)]
pub struct PullDetail {
    #[serde(default)]
    pub additions: u64,
    #[serde(default)]
    pub deletions: u64,
    pub base: BaseRef,
}

#[derive(Debug, Deserialize)]
pub struct BaseRef {
    #[serde(rename = "ref")]
    pub branch: String,
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
/// details (additions, deletions, base branch) in parallel.
///
/// M2 does not yet split `review_mine` from `ready_to_merge_mine` — that
/// requires the reviews API and lands in M3.
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
    let q_wait_me = format!("is:pr is:open review-requested:@me org:{org}");
    let q_wait_author = format!("is:pr is:open reviewed-by:@me -author:@me org:{org}");

    let (r_draft, r_review_mine, r_wait_me, r_wait_author) = tokio::try_join!(
        client.search_issues(&q_draft),
        client.search_issues(&q_author_mine),
        client.search_issues(&q_wait_me),
        client.search_issues(&q_wait_author),
    )?;

    let details = fetch_all_details(
        client,
        [&r_draft, &r_review_mine, &r_wait_me, &r_wait_author],
    )
    .await?;

    let now = Utc::now();
    let build = |items: &[SearchItem], col: Column| -> Vec<Pr> {
        items
            .iter()
            .map(|item| build_pr(item, col, &details, productive_org_slug, now))
            .collect()
    };

    let columns = ColumnsData {
        draft_mine: build(&r_draft, Column::DraftMine),
        review_mine: build(&r_review_mine, Column::ReviewMine),
        ready_to_merge_mine: Vec::new(),
        waiting_on_me: build(&r_wait_me, Column::WaitingOnMe),
        waiting_on_author: build(&r_wait_author, Column::WaitingOnAuthor),
    };

    Ok(BoardState {
        user,
        fetched_at: now,
        columns,
    })
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
