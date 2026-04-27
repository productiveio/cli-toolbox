use chrono::Utc;
use colored::{ColoredString, Colorize};

use crate::commands::util::humanize_age_hours;
use crate::config::Config;
use crate::core::cache::BoardCache;
use crate::core::github::{GhClient, fetch_board_state, merge_with_previous};
use crate::core::model::{
    BoardState, CheckState, Column, ColumnsData, Notification, Pr, RottingBucket, SizeBucket,
};
use crate::error::{Error, Result};
use toolbox_core::cache::CacheTtl;
use toolbox_core::output::truncate;

pub async fn run(column: Option<String>, stale_days: Option<u32>, json: bool) -> Result<()> {
    let filter_col = match column.as_deref() {
        Some(s) => Some(Column::parse(s).ok_or_else(|| {
            Error::Other(format!(
                "unknown column `{s}` — expected one of: draft-mine, review-mine, \
                 ready-to-merge-mine, waiting-on-me, waiting-on-author, mentions"
            ))
        })?),
        None => None,
    };

    let config = Config::load()?;
    let cache = BoardCache::new()?;
    let mut state = load_or_fetch(&config, &cache).await?;

    if let Some(min_days) = stale_days {
        let cutoff = min_days as f64;
        retain_stale(&mut state.columns.draft_mine, cutoff);
        retain_stale(&mut state.columns.review_mine, cutoff);
        retain_stale(&mut state.columns.ready_to_merge_mine, cutoff);
        retain_stale(&mut state.columns.waiting_on_me, cutoff);
        retain_stale(&mut state.columns.waiting_on_author, cutoff);
    }

    if let Some(col) = filter_col {
        blank_other_columns(&mut state.columns, col);
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&state)?);
        return Ok(());
    }

    // Dedicated mentions view — notifications aren't PRs, don't mix them
    // into the flattened table.
    if filter_col == Some(Column::Mentions) {
        render_mentions(&state);
        return Ok(());
    }

    render_table(&state);
    Ok(())
}

/// Read the board state from the cache when fresh (Medium TTL = 5 min);
/// otherwise fetch from GitHub and persist the new result. Falls back to
/// the prior cache (within Long TTL) for any column the search wipes —
/// see `merge_with_previous`.
async fn load_or_fetch(config: &Config, cache: &BoardCache) -> Result<BoardState> {
    if let Some(cached) = cache.load_board(&CacheTtl::Medium) {
        return Ok(cached);
    }
    let prev = cache.load_board(&CacheTtl::Long);
    let client = GhClient::new()?;
    let fresh = fetch_board_state(
        &client,
        &config.github.org,
        &config.productive.org_slug,
        Some(config.github.username_override.as_str()),
    )
    .await?;
    let state = merge_with_previous(fresh, prev);
    cache.save_board(&state)?;
    Ok(state)
}

fn retain_stale(list: &mut Vec<Pr>, min_days: f64) {
    list.retain(|p| p.age_days >= min_days);
}

fn blank_other_columns(columns: &mut ColumnsData, keep: Column) {
    if keep != Column::DraftMine {
        columns.draft_mine.clear();
    }
    if keep != Column::ReviewMine {
        columns.review_mine.clear();
    }
    if keep != Column::ReadyToMergeMine {
        columns.ready_to_merge_mine.clear();
    }
    if keep != Column::WaitingOnMe {
        columns.waiting_on_me.clear();
    }
    if keep != Column::WaitingOnAuthor {
        columns.waiting_on_author.clear();
    }
    if keep != Column::Mentions {
        columns.notifications.clear();
    }
}

/// Flatten into one list tagged with which column each PR came from,
/// then sort by rotting bucket (critical → fresh) and age desc.
fn flatten(state: &crate::core::model::BoardState) -> Vec<(Column, &Pr)> {
    let mut out: Vec<(Column, &Pr)> = Vec::new();
    for pr in &state.columns.draft_mine {
        out.push((Column::DraftMine, pr));
    }
    for pr in &state.columns.review_mine {
        out.push((Column::ReviewMine, pr));
    }
    for pr in &state.columns.ready_to_merge_mine {
        out.push((Column::ReadyToMergeMine, pr));
    }
    for pr in &state.columns.waiting_on_me {
        out.push((Column::WaitingOnMe, pr));
    }
    for pr in &state.columns.waiting_on_author {
        out.push((Column::WaitingOnAuthor, pr));
    }
    out.sort_by(|a, b| {
        rot_rank(b.1.rotting)
            .cmp(&rot_rank(a.1.rotting))
            .then_with(|| {
                b.1.age_days
                    .partial_cmp(&a.1.age_days)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    out
}

fn rot_rank(b: RottingBucket) -> u8 {
    match b {
        RottingBucket::Fresh => 0,
        RottingBucket::Warming => 1,
        RottingBucket::Stale => 2,
        RottingBucket::Rotting => 3,
        RottingBucket::Critical => 4,
    }
}

fn column_tag(c: Column) -> ColoredString {
    match c {
        Column::DraftMine => "draft".dimmed(),
        Column::ReviewMine => "review".normal(),
        Column::ReadyToMergeMine => "ready".green(),
        Column::WaitingOnMe => "wait-me".cyan().bold(),
        Column::WaitingOnAuthor => "wait-au".normal(),
        // Mentions doesn't appear in the flattened PR table — notifications
        // are rendered separately by list_mentions(). Kept for exhaustiveness.
        Column::Mentions => "mention".cyan(),
    }
}

fn size_tag(s: Option<SizeBucket>) -> String {
    match s {
        Some(SizeBucket::Xs) => "XS".into(),
        Some(SizeBucket::S) => "S".into(),
        Some(SizeBucket::M) => "M".into(),
        Some(SizeBucket::L) => "L".into(),
        Some(SizeBucket::Xl) => "XL".into(),
        None => "-".into(),
    }
}

fn ci_tag(state: Option<CheckState>) -> ColoredString {
    match state {
        Some(CheckState::Success) => "✓".green(),
        Some(CheckState::Failure) => "✗".red().bold(),
        Some(CheckState::Pending) => "●".yellow(),
        None => " ".normal(),
    }
}

fn age_tag(pr: &Pr) -> ColoredString {
    let text = humanize_age_hours(pr.age_days * 24.0);
    match pr.rotting {
        RottingBucket::Fresh => text.dimmed(),
        RottingBucket::Warming => text.green(),
        RottingBucket::Stale => text.yellow(),
        RottingBucket::Rotting => text.truecolor(255, 165, 0),
        RottingBucket::Critical => text.red().bold(),
    }
}

fn render_mentions(state: &BoardState) {
    let notifications: &[Notification] = &state.columns.notifications;
    if notifications.is_empty() {
        println!("{}", "No unread PR notifications.".dimmed());
        return;
    }
    println!(
        "{}",
        format!(
            "{:<11} {:<24} {:<6} {}",
            "REASON", "REPO#NUM", "AGE", "TITLE"
        )
        .bold()
    );
    let mut sorted: Vec<&Notification> = notifications.iter().collect();
    sorted.sort_by(|a, b| {
        b.age_days
            .partial_cmp(&a.age_days)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for n in sorted {
        let repo_num = format!("{}#{}", n.repo, n.pr_number);
        let age = humanize_age_hours(n.age_days * 24.0);
        let title = truncate(&n.pr_title, 80);
        println!(
            "{:<11} {:<24} {:<6} {}",
            n.reason.short_label().cyan(),
            repo_num,
            age,
            title,
        );
    }
}

fn render_table(state: &crate::core::model::BoardState) {
    let rows = flatten(state);
    if rows.is_empty() {
        println!("{}", "No PRs match the current filters.".dimmed());
        return;
    }

    // Header
    println!(
        "{}",
        format!(
            "{:<8} {:<2} {:<24} {:<4} {:<6} {:<60} {}",
            "COLUMN", "CI", "REPO#NUM", "SIZE", "AGE", "TITLE", "TASK"
        )
        .bold()
    );

    for (col, pr) in &rows {
        let repo_num = format!("{}#{}", pr.repo, pr.number);
        let mut title = truncate(&pr.title, 60);
        if pr.has_new_commits_since_my_review == Some(true) {
            title = format!("🆕 {title}");
        }
        let task = pr.productive_task_id.as_deref().unwrap_or("");
        println!(
            "{:<8} {:<2} {:<24} {:<4} {:<6} {:<60} {}",
            column_tag(*col),
            ci_tag(pr.check_state),
            repo_num,
            size_tag(pr.size),
            age_tag(pr),
            title,
            task.dimmed(),
        );
    }

    // Footer with column counts
    let counts = [
        ("draft", state.columns.draft_mine.len()),
        ("review", state.columns.review_mine.len()),
        ("ready", state.columns.ready_to_merge_mine.len()),
        ("wait-me", state.columns.waiting_on_me.len()),
        ("wait-au", state.columns.waiting_on_author.len()),
        ("mentions", state.columns.notifications.len()),
    ];
    let summary: String = counts
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("  ");
    println!();
    println!("{}", summary.dimmed());
    let age_hours = (Utc::now() - state.fetched_at).num_seconds() as f64 / 3600.0;
    let age = humanize_age_hours(age_hours);
    println!(
        "{}",
        format!("user={}  refreshed {} ago", state.user, age).dimmed()
    );
}
