use colored::Colorize;

use crate::config::Config;
use crate::core::cache::BoardCache;
use crate::core::github::{GhClient, fetch_board_state};
use crate::error::Result;

pub async fn run() -> Result<()> {
    let config = Config::load()?;
    let cache = BoardCache::new()?;
    cache.clear()?;

    let client = GhClient::new()?;
    let state = fetch_board_state(
        &client,
        &config.github.org,
        &config.productive.org_slug,
        Some(config.github.username_override.as_str()),
    )
    .await?;
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
    Ok(())
}
