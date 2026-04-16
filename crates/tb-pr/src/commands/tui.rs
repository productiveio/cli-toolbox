use toolbox_core::cache::CacheTtl;

use crate::config::Config;
use crate::core::cache::BoardCache;
use crate::core::github::{GhClient, fetch_board_state};
use crate::error::Result;
use crate::tui::app::{self, FetchCtx};

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
    let refresh_interval = config.refresh_interval();
    let ctx = FetchCtx {
        org: config.github.org,
        productive_org_slug: config.productive.org_slug,
        username_override: config.github.username_override,
        refresh_interval,
    };
    app::run(state, ctx).await
}
