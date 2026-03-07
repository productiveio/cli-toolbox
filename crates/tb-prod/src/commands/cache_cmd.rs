use crate::api::ProductiveClient;
use crate::cache::Cache;
use crate::error::Result;

pub async fn sync(client: &ProductiveClient) -> Result<()> {
    let cache = Cache::new(client.org_id())?;
    cache.sync(client).await
}

pub async fn clear(org_id: &str) -> Result<()> {
    let cache = Cache::new(org_id)?;
    cache.clear()
}
