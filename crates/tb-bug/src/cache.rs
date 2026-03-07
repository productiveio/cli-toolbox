use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use crate::error::Result;

pub struct Cache {
    dir: PathBuf,
}

/// TTL categories for different endpoint types.
pub enum CacheTtl {
    /// Organizations, projects — rarely change
    Long,
    /// Error lists — change frequently, protect against rapid re-calls
    Short,
    /// Single error/event detail, stability, trends, releases
    Medium,
}

impl CacheTtl {
    pub fn duration(&self) -> Duration {
        match self {
            CacheTtl::Long => Duration::from_secs(3600),   // 1 hour
            CacheTtl::Short => Duration::from_secs(120),   // 2 minutes
            CacheTtl::Medium => Duration::from_secs(300),  // 5 minutes
        }
    }
}

impl Cache {
    pub fn new() -> Result<Self> {
        let dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("tb-bug");
        std::fs::create_dir_all(&dir)?;
        let cache = Self { dir };
        cache.evict_stale();
        Ok(cache)
    }

    /// Remove files older than MAX_AGE.
    fn evict_stale(&self) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else { return };
        for entry in entries.flatten() {
            let is_stale = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| SystemTime::now().duration_since(t).ok())
                .is_some_and(|age| age > CacheTtl::Long.duration());
            if is_stale {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    /// Get cached response for a URL if it exists and is within TTL.
    pub fn get(&self, url: &str, ttl: &CacheTtl) -> Option<String> {
        let path = self.path_for(url);
        let metadata = std::fs::metadata(&path).ok()?;
        let modified = metadata.modified().ok()?;
        let age = SystemTime::now().duration_since(modified).ok()?;
        if age > ttl.duration() {
            return None;
        }
        std::fs::read_to_string(&path).ok()
    }

    /// Store a response in the cache.
    pub fn set(&self, url: &str, body: &str) {
        let path = self.path_for(url);
        let _ = std::fs::write(&path, body);
    }

    /// Clear the entire cache directory.
    pub fn clear(&self) -> Result<()> {
        if self.dir.exists() {
            std::fs::remove_dir_all(&self.dir)?;
            std::fs::create_dir_all(&self.dir)?;
        }
        Ok(())
    }

    fn path_for(&self, url: &str) -> PathBuf {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        url.hash(&mut hasher);
        self.dir.join(format!("{:x}.json", hasher.finish()))
    }
}
