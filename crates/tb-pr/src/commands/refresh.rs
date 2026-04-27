use colored::Colorize;
use toolbox_core::cache::CacheTtl;

use crate::config::Config;
use crate::core::cache::BoardCache;
use crate::core::github::{GhClient, fetch_board_state, merge_with_previous};
use crate::error::Result;

pub async fn run() -> Result<()> {
    let config = Config::load()?;
    let cache = BoardCache::new()?;
    // Read the prior board *before* clearing — used as a stale-data fallback
    // for any column the upcoming search returns empty (GitHub search index
    // degradations silently zero out columns).
    let prev = cache.load_board(&CacheTtl::Long);
    cache.clear()?;

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

    let c = &state.columns;
    let total = c.draft_mine.len()
        + c.review_mine.len()
        + c.ready_to_merge_mine.len()
        + c.waiting_on_me.len()
        + c.waiting_on_author.len();
    println!(
        "{} {} PR(s) — draft={} review={} ready={} wait-me={} wait-au={}",
        "refreshed".green().bold(),
        total,
        c.draft_mine.len(),
        c.review_mine.len(),
        c.ready_to_merge_mine.len(),
        c.waiting_on_me.len(),
        c.waiting_on_author.len(),
    );
    if !state.column_issues.is_empty() {
        let formatted: Vec<String> = state
            .column_issues
            .iter()
            .map(|i| format!("{} ({})", i.column.short_label(), i.reason))
            .collect();
        eprintln!(
            "{} partial fetch — {}",
            "⚠".yellow().bold(),
            formatted.join(", "),
        );
    }
    Ok(())
}
