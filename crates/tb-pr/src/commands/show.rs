use chrono::{DateTime, Utc};
use colored::Colorize;
use serde::{Deserialize, Serialize};

use crate::commands::util::{humanize_age_hours, parse_pr_ref};
use crate::config::Config;
use crate::core::cache::BoardCache;
use crate::core::classifier;
use crate::core::github::{GhClient, PullDetail};
use crate::core::model::{PrState, SizeBucket};
use crate::core::productive::extract_task_id;
use crate::core::reviews::{Review, ReviewSummary};
use crate::error::Result;
use toolbox_core::cache::CacheTtl;

/// Cached payload for `tb-pr show` — the three API calls combined so we
/// don't hit GitHub on repeated invocations within the TTL.
#[derive(Serialize, Deserialize)]
struct CachedShow {
    detail: PullDetail,
    reviews: Vec<Review>,
    head_date: Option<DateTime<Utc>>,
    me: String,
}

pub async fn run(pr_ref: &str, json: bool) -> Result<()> {
    let config = Config::load()?;
    let client = GhClient::new()?;
    let pr = parse_pr_ref(pr_ref, &config.github.org)?;
    let cache = BoardCache::new()?;
    let cache_key = pr.web_url();

    let CachedShow {
        detail,
        reviews,
        head_date,
        me,
    } = match cache.load_show::<CachedShow>(&cache_key, &CacheTtl::Medium) {
        Some(c) => c,
        None => {
            let (detail, reviews, me) = tokio::try_join!(
                client.pull_detail(&pr.owner, &pr.repo, pr.number),
                client.pull_reviews(&pr.owner, &pr.repo, pr.number),
                async {
                    if config.github.username_override.is_empty() {
                        client.user_login().await
                    } else {
                        Ok(config.github.username_override.clone())
                    }
                }
            )?;
            // The head-commit date powers the "🆕 author pushed after your
            // review" indicator; it's a nice-to-have, so don't fail the whole
            // `show` when it errors — but do warn to stderr instead of
            // silently swallowing with `.ok()`.
            let head_date = match client
                .commit_date(&pr.owner, &pr.repo, &detail.head.sha)
                .await
            {
                Ok(d) => Some(d),
                Err(e) => {
                    eprintln!(
                        "warning: could not fetch head commit date ({e}); \
                         '🆕 new commits' indicator will be omitted"
                    );
                    None
                }
            };
            let fresh = CachedShow {
                detail,
                reviews,
                head_date,
                me,
            };
            cache.save_show(&cache_key, &fresh)?;
            fresh
        }
    };

    let summary = ReviewSummary::from_reviews(&reviews);

    let state = if detail.draft {
        PrState::Draft
    } else if summary.is_ready_to_merge() {
        PrState::Approved
    } else {
        PrState::Ready
    };
    let size = classifier::size_bucket(detail.additions, detail.deletions);
    let author = detail.user.as_ref().map(|u| u.login.clone());
    let task = extract_task_id(
        detail.body.as_deref().unwrap_or(""),
        &config.productive.org_slug,
    );

    if json {
        let payload = ShowPayload {
            owner: &pr.owner,
            repo: &pr.repo,
            number: pr.number,
            url: &pr.web_url(),
            title: detail.title.clone(),
            author: author.clone(),
            state,
            additions: detail.additions,
            deletions: detail.deletions,
            size,
            base_branch: detail.base.branch.clone(),
            head_sha: detail.head.sha.clone(),
            comments_count: detail.comments,
            productive_task_id: task.clone(),
            ready_to_merge: summary.is_ready_to_merge(),
            reviews: reviews
                .iter()
                .map(|r| ReviewPayload {
                    user: r.user.login.clone(),
                    state: r.state.clone(),
                    submitted_at: r.submitted_at.map(|t| t.to_rfc3339()),
                })
                .collect(),
            my_latest_review: summary.my_latest_review(&me).map(|r| r.state.clone()),
        };
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    println!("{}", format!("{}#{}", pr.repo, pr.number).bold());
    println!("{}", detail.title.bold());
    println!("{}", pr.web_url().underline().blue());
    println!();

    let age_text = detail
        .created_at
        .map(|t| {
            let hours = (Utc::now() - t).num_seconds() as f64 / 3600.0;
            humanize_age_hours(hours)
        })
        .unwrap_or_default();

    println!("{:<14}{}", "State:".dimmed(), colorize_state(&state));
    println!(
        "{:<14}{} ({})",
        "Size:".dimmed(),
        size_tag(size),
        format!("+{} -{}", detail.additions, detail.deletions).dimmed()
    );
    if let Some(a) = &author {
        println!("{:<14}{}", "Author:".dimmed(), a);
    }
    if !age_text.is_empty() {
        println!("{:<14}{} ago", "Created:".dimmed(), age_text);
    }
    println!("{:<14}{}", "Comments:".dimmed(), detail.comments);
    println!("{:<14}{}", "Base:".dimmed(), detail.base.branch);
    let head_short: String = detail.head.sha.chars().take(12).collect();
    println!("{:<14}{}", "Head:".dimmed(), head_short);

    if let Some(t) = &task {
        let url = format!(
            "https://app.productive.io/{}/tasks/{}",
            config.productive.org_slug, t
        );
        println!("{:<14}{}", "Productive:".dimmed(), url.blue().underline());
    }

    println!();
    println!("{}", "Reviews:".bold());
    if reviews.is_empty() {
        println!("  {}", "(none yet)".dimmed());
    } else {
        let mut by_user: std::collections::HashMap<String, &crate::core::reviews::Review> =
            std::collections::HashMap::new();
        for r in &reviews {
            let entry = by_user.entry(r.user.login.clone()).or_insert(r);
            if r.submitted_at > entry.submitted_at {
                *entry = r;
            }
        }
        let mut latest: Vec<_> = by_user.values().collect();
        latest.sort_by_key(|r| r.submitted_at);
        for r in latest {
            let when = r
                .submitted_at
                .map(|t| {
                    let hours = (Utc::now() - t).num_seconds() as f64 / 3600.0;
                    format!("{} ago", humanize_age_hours(hours))
                })
                .unwrap_or_else(|| "—".to_string());
            println!(
                "  {:<20} {:<20} {}",
                r.user.login,
                colorize_review_state(&r.state),
                when.dimmed()
            );
        }
    }

    if let Some(my_review) = summary.my_latest_review(&me) {
        let when = my_review
            .submitted_at
            .map(|t| {
                let hours = (Utc::now() - t).num_seconds() as f64 / 3600.0;
                format!("{} ago", humanize_age_hours(hours))
            })
            .unwrap_or_else(|| "—".to_string());
        let new_commits = match (head_date, my_review.submitted_at) {
            (Some(h), Some(r)) if h > r => " 🆕 author pushed after your review".yellow(),
            _ => "".normal(),
        };
        println!();
        println!(
            "{:<14}{} ({}){}",
            "Your review:".dimmed(),
            colorize_review_state(&my_review.state),
            when,
            new_commits,
        );
    }

    Ok(())
}

fn colorize_state(state: &PrState) -> colored::ColoredString {
    match state {
        PrState::Draft => "draft".dimmed(),
        PrState::Ready => "ready".normal(),
        PrState::Approved => "approved".green().bold(),
    }
}

fn colorize_review_state(s: &str) -> colored::ColoredString {
    match s.to_ascii_uppercase().as_str() {
        "APPROVED" => "APPROVED".green().bold(),
        "CHANGES_REQUESTED" => "CHANGES_REQUESTED".red().bold(),
        "COMMENTED" => "COMMENTED".normal(),
        "DISMISSED" => "DISMISSED".dimmed(),
        "PENDING" => "PENDING".dimmed(),
        other => other.to_string().normal(),
    }
}

fn size_tag(s: SizeBucket) -> &'static str {
    match s {
        SizeBucket::Xs => "XS",
        SizeBucket::S => "S",
        SizeBucket::M => "M",
        SizeBucket::L => "L",
        SizeBucket::Xl => "XL",
    }
}

#[derive(Serialize)]
struct ShowPayload<'a> {
    owner: &'a str,
    repo: &'a str,
    number: u64,
    url: &'a str,
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    author: Option<String>,
    state: PrState,
    additions: u64,
    deletions: u64,
    size: SizeBucket,
    base_branch: String,
    head_sha: String,
    comments_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    productive_task_id: Option<String>,
    ready_to_merge: bool,
    reviews: Vec<ReviewPayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    my_latest_review: Option<String>,
}

#[derive(Serialize)]
struct ReviewPayload {
    user: String,
    state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    submitted_at: Option<String>,
}
