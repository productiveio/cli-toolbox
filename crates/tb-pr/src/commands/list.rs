use colored::{ColoredString, Colorize};

use crate::commands::util::humanize_age_hours;
use crate::config::Config;
use crate::core::github::{GhClient, fetch_board_state};
use crate::core::model::{Column, ColumnsData, Pr, RottingBucket, SizeBucket};
use crate::error::{Error, Result};
use toolbox_core::output::truncate;

pub async fn run(column: Option<String>, stale_days: Option<u32>, json: bool) -> Result<()> {
    let filter_col = match column.as_deref() {
        Some(s) => Some(Column::parse(s).ok_or_else(|| {
            Error::Other(format!(
                "unknown column `{s}` — expected one of: draft-mine, review-mine, \
                 ready-to-merge-mine, waiting-on-me, waiting-on-author"
            ))
        })?),
        None => None,
    };

    let config = Config::load()?;
    let client = GhClient::new()?;
    let mut state = fetch_board_state(
        &client,
        &config.github.org,
        &config.productive.org_slug,
        Some(config.github.username_override.as_str()),
    )
    .await?;

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

    render_table(&state);
    Ok(())
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
            "{:<8} {:<24} {:<4} {:<6} {:<60} {}",
            "COLUMN", "REPO#NUM", "SIZE", "AGE", "TITLE", "TASK"
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
            "{:<8} {:<24} {:<4} {:<6} {:<60} {}",
            column_tag(*col),
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
    ];
    let summary: String = counts
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("  ");
    println!();
    println!("{}", summary.dimmed());
    println!(
        "{}",
        format!(
            "user={}  fetched={}",
            state.user,
            state.fetched_at.format("%Y-%m-%dT%H:%M:%SZ")
        )
        .dimmed()
    );
}
