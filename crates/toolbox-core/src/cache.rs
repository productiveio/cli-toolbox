use std::path::PathBuf;
use std::time::{Duration, SystemTime};

pub struct Cache {
    dir: PathBuf,
}

pub enum CacheTtl {
    /// 1 hour — rarely changing data (project lists, immutable detail records)
    Long,
    /// 5 minutes — dashboards, reports, aggregate stats
    Medium,
    /// 2 minutes — list views, queue items, metrics
    Short,
}

impl CacheTtl {
    pub fn duration(&self) -> Duration {
        match self {
            CacheTtl::Long => Duration::from_secs(3600),
            CacheTtl::Medium => Duration::from_secs(300),
            CacheTtl::Short => Duration::from_secs(120),
        }
    }
}

impl Cache {
    /// Create a cache at `~/.cache/{tool_name}/`, evicting stale entries on startup.
    pub fn new(tool_name: &str) -> std::io::Result<Self> {
        let dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(tool_name);
        std::fs::create_dir_all(&dir)?;
        let cache = Self { dir };
        cache.evict_stale();
        Ok(cache)
    }

    fn evict_stale(&self) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
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

    /// Get a cached response if it exists and is fresher than `ttl`.
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

    /// Store a response body in the cache.
    pub fn set(&self, url: &str, body: &str) {
        let path = self.path_for(url);
        let _ = std::fs::write(&path, body);
    }

    /// Returns (file_count, total_bytes).
    pub fn size(&self) -> (usize, u64) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return (0, 0);
        };
        let mut count = 0usize;
        let mut bytes = 0u64;
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                count += 1;
                bytes += meta.len();
            }
        }
        (count, bytes)
    }

    /// Delete all cached files.
    pub fn clear(&self) -> std::io::Result<()> {
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
