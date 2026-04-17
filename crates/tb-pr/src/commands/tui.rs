use chrono::Utc;
use toolbox_core::cache::CacheTtl;

use crate::config::Config;
use crate::core::cache::BoardCache;
use crate::core::model::{BoardState, ColumnsData};
use crate::error::Result;
use crate::tui::app::{self, FetchCtx};

/// Cache age beyond which the TUI kicks off a background refresh on launch.
/// Mirrors the `CacheTtl::Medium` window used elsewhere — in sync so that a
/// `list` right before opening the TUI doesn't force a redundant fetch.
const FRESH_WINDOW_SECS: i64 = 300;

pub async fn run() -> Result<()> {
    let config = Config::load()?;
    let cache = BoardCache::new()?;

    // Load any cached state, even if stale — stale-while-revalidate.
    // Long TTL here means "give me the file as long as it's not truly
    // ancient"; we decide whether to refresh below based on fetched_at.
    let (state, needs_refresh) = match cache.load_board(&CacheTtl::Long) {
        Some(cached) => {
            let stale = (Utc::now() - cached.fetched_at).num_seconds() > FRESH_WINDOW_SECS;
            (cached, stale)
        }
        None => (empty_state(), true),
    };

    let refresh_interval = config.refresh_interval();
    let ctx = FetchCtx {
        org: config.github.org,
        productive_org_slug: config.productive.org_slug,
        username_override: config.github.username_override,
        refresh_interval,
    };
    app::run(state, ctx, needs_refresh).await
}

/// Placeholder shown before the first successful fetch. Empty columns,
/// empty user (header renders just `tb-pr` while fetching). `fetched_at`
/// is set to epoch so the "refreshed Nm ago" label doesn't lie.
fn empty_state() -> BoardState {
    BoardState {
        user: String::new(),
        fetched_at: chrono::DateTime::<Utc>::from_timestamp(0, 0).unwrap_or_else(Utc::now),
        columns: ColumnsData {
            draft_mine: Vec::new(),
            review_mine: Vec::new(),
            ready_to_merge_mine: Vec::new(),
            waiting_on_me: Vec::new(),
            waiting_on_author: Vec::new(),
            notifications: Vec::new(),
        },
    }
}
