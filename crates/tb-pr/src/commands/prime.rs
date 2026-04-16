use chrono::Utc;
use toolbox_core::cache::CacheTtl;

use crate::commands::util::humanize_age_hours;
use crate::config::Config;
use crate::core::cache::BoardCache;
use crate::core::github::{GhClient, fetch_board_state};
use crate::core::model::{BoardState, Notification, Pr, SizeBucket};
use crate::error::Result;

pub async fn run() -> Result<()> {
    let config = Config::load()?;
    let cache = BoardCache::new()?;
    let state = match cache.load_board(&CacheTtl::Medium) {
        Some(cached) => cached,
        None => {
            let client = GhClient::new()?;
            let fresh = fetch_board_state(
                &client,
                &config.github.org,
                &config.productive.org_slug,
                Some(config.github.username_override.as_str()),
            )
            .await?;
            cache.save_board(&fresh)?;
            fresh
        }
    };
    print!("{}", render(&state));
    Ok(())
}

fn render(state: &BoardState) -> String {
    let c = &state.columns;
    let age_hours = (Utc::now() - state.fetched_at).num_seconds() as f64 / 3600.0;
    let age = humanize_age_hours(age_hours);

    let mut out = String::new();
    out.push_str("# tb-pr live state\n\n");
    out.push_str(&format!(
        "refreshed {age} ago · {}@productiveio\n\n",
        state.user
    ));

    // One-line summary counts.
    out.push_str(&format!(
        "- {} waiting on me{}\n",
        c.waiting_on_me.len(),
        oldest_suffix(&c.waiting_on_me),
    ));
    out.push_str(&format!(
        "- {} of my PRs in review ({} ready to merge)\n",
        c.review_mine.len(),
        c.ready_to_merge_mine.len(),
    ));
    out.push_str(&format!(
        "- {} waiting on author (for my re-review){}\n",
        c.waiting_on_author.len(),
        oldest_suffix(&c.waiting_on_author),
    ));
    if !c.notifications.is_empty() {
        out.push_str(&format!(
            "- {} unread PR notifications{}\n",
            c.notifications.len(),
            oldest_notification_suffix(&c.notifications),
        ));
    }
    if !c.draft_mine.is_empty() {
        out.push_str(&format!("- {} of my drafts\n", c.draft_mine.len()));
    }

    append_section(&mut out, "Waiting on me (urgent first)", &c.waiting_on_me);
    append_section(&mut out, "Ready to merge", &c.ready_to_merge_mine);
    append_section(&mut out, "Waiting on author", &c.waiting_on_author);
    append_notifications(&mut out, &c.notifications);

    out
}

fn append_notifications(out: &mut String, notifications: &[Notification]) {
    if notifications.is_empty() {
        return;
    }
    out.push_str("\n## Mentions (urgent first)\n");
    let mut sorted: Vec<&Notification> = notifications.iter().collect();
    sorted.sort_by(|a, b| {
        b.age_days
            .partial_cmp(&a.age_days)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for n in sorted {
        let age = humanize_age_hours(n.age_days * 24.0);
        out.push_str(&format!(
            "- {} {}#{} — {} ({age})\n",
            n.reason.short_label(),
            n.repo,
            n.pr_number,
            n.pr_title,
        ));
    }
}

fn oldest_notification_suffix(notifications: &[Notification]) -> String {
    let max = notifications
        .iter()
        .map(|n| n.age_days)
        .fold(0.0_f64, f64::max);
    if max < 0.01 {
        return String::new();
    }
    format!(" (oldest: {})", humanize_age_hours(max * 24.0))
}

/// Append a `## Title` section with a bulleted PR list. Sorted by age desc
/// so the oldest (most urgent) PR surfaces first.
fn append_section(out: &mut String, title: &str, prs: &[Pr]) {
    if prs.is_empty() {
        return;
    }
    out.push_str(&format!("\n## {title}\n"));
    let mut sorted: Vec<&Pr> = prs.iter().collect();
    sorted.sort_by(|a, b| {
        b.age_days
            .partial_cmp(&a.age_days)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for pr in sorted {
        out.push_str(&format_pr(pr));
        out.push('\n');
    }
}

fn format_pr(pr: &Pr) -> String {
    let age = humanize_age_hours(pr.age_days * 24.0);
    let size = match pr.size {
        Some(SizeBucket::Xs) => "XS",
        Some(SizeBucket::S) => "S",
        Some(SizeBucket::M) => "M",
        Some(SizeBucket::L) => "L",
        Some(SizeBucket::Xl) => "XL",
        None => "-",
    };
    let mut extras = String::new();
    if let Some(task) = &pr.productive_task_id {
        extras.push_str(&format!(", [P-{task}]"));
    }
    if pr.comments_count > 0 {
        extras.push_str(&format!(", 💬{}", pr.comments_count));
    }
    if pr.has_new_commits_since_my_review == Some(true) {
        extras.push_str(", 🆕");
    }
    format!(
        "- {}#{} — {} ({age}, {size}{extras})",
        pr.repo, pr.number, pr.title
    )
}

fn oldest_suffix(prs: &[Pr]) -> String {
    let max = prs.iter().map(|p| p.age_days).fold(0.0_f64, f64::max);
    if max < 0.01 {
        return String::new();
    }
    format!(" (oldest: {})", humanize_age_hours(max * 24.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ColumnsData, PrState, RottingBucket, SizeBucket};

    fn pr(repo: &str, number: u64, title: &str, age_days: f64) -> Pr {
        Pr {
            number,
            repo: repo.to_string(),
            title: title.to_string(),
            url: format!("https://github.com/productiveio/{repo}/pull/{number}"),
            author: "ilucin".to_string(),
            state: PrState::Ready,
            created_at: Utc::now() - chrono::Duration::seconds((age_days * 86400.0) as i64),
            age_days,
            size: Some(SizeBucket::M),
            rotting: RottingBucket::Warming,
            productive_task_id: Some("1234".to_string()),
            comments_count: 2,
            base_branch: None,
            has_new_commits_since_my_review: None,
        }
    }

    #[test]
    fn renders_summary_and_sections() {
        let state = BoardState {
            user: "ilucin".to_string(),
            fetched_at: Utc::now(),
            columns: ColumnsData {
                draft_mine: vec![pr("ai-agent", 1, "Spike", 15.0)],
                review_mine: vec![pr("api", 2, "Feature A", 1.0)],
                ready_to_merge_mine: vec![pr("frontend", 9, "Ready one", 0.5)],
                waiting_on_me: vec![
                    pr("frontend", 1234, "Fix billing form", 5.0),
                    pr("api", 567, "Add webhook endpoint", 2.0),
                ],
                waiting_on_author: vec![],
                notifications: vec![],
            },
        };
        let s = render(&state);
        assert!(s.contains("2 waiting on me"));
        assert!(s.contains("1 of my drafts"));
        assert!(s.contains("## Waiting on me"));
        assert!(s.contains("frontend#1234"));
        assert!(s.contains("[P-1234]"));
        // Sorted by age desc — the 5d PR comes before the 2d one.
        let wait_me = s.split("## Waiting on me").nth(1).unwrap();
        let first = wait_me.find("#1234").unwrap();
        let second = wait_me.find("#567").unwrap();
        assert!(first < second);
    }

    #[test]
    fn empty_sections_are_omitted() {
        let state = BoardState {
            user: "ilucin".to_string(),
            fetched_at: Utc::now(),
            columns: ColumnsData {
                draft_mine: vec![],
                review_mine: vec![],
                ready_to_merge_mine: vec![],
                waiting_on_me: vec![],
                waiting_on_author: vec![],
                notifications: vec![],
            },
        };
        let s = render(&state);
        assert!(s.contains("0 waiting on me"));
        assert!(!s.contains("## Waiting on me"));
    }
}
