use serde::{Serialize, de::DeserializeOwned};

use crate::core::model::BoardState;
use crate::error::{Error, Result};
use toolbox_core::cache::{Cache as CoreCache, CacheTtl};

const BOARD_KEY: &str = "board:state";

/// Thin wrapper around `toolbox_core::cache::Cache` with typed helpers
/// for the tb-pr data model. The underlying store is file-based at
/// `~/.cache/tb-pr/`; values are serialized as JSON.
pub struct BoardCache {
    inner: CoreCache,
}

impl BoardCache {
    pub fn new() -> Result<Self> {
        let inner =
            CoreCache::new("tb-pr").map_err(|e| Error::Other(format!("cache init: {e}")))?;
        Ok(Self { inner })
    }

    pub fn load_board(&self, ttl: &CacheTtl) -> Option<BoardState> {
        let body = self.inner.get(BOARD_KEY, ttl)?;
        serde_json::from_str(&body).ok()
    }

    pub fn save_board(&self, state: &BoardState) -> Result<()> {
        let body = serde_json::to_string(state)?;
        self.inner.set(BOARD_KEY, &body);
        Ok(())
    }

    pub fn load_show<T: DeserializeOwned>(&self, pr_url: &str, ttl: &CacheTtl) -> Option<T> {
        let body = self.inner.get(&show_key(pr_url), ttl)?;
        serde_json::from_str(&body).ok()
    }

    pub fn save_show<T: Serialize>(&self, pr_url: &str, payload: &T) -> Result<()> {
        let body = serde_json::to_string(payload)?;
        self.inner.set(&show_key(pr_url), &body);
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        self.inner
            .clear()
            .map_err(|e| Error::Other(format!("cache clear: {e}")))
    }
}

fn show_key(pr_url: &str) -> String {
    format!("show:{pr_url}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_keys_differ_per_url() {
        let a = show_key("https://github.com/productiveio/a/pull/1");
        let b = show_key("https://github.com/productiveio/a/pull/2");
        assert_ne!(a, b);
        assert!(a.starts_with("show:"));
    }
}
