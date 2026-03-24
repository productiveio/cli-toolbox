use crate::api::ProductiveClient;
use crate::error::Result;
use crate::generic_cache::GenericCache;

pub async fn sync(client: &ProductiveClient) -> Result<()> {
    let cache = GenericCache::new(client.org_id())?;
    cache.sync_org(client).await
}

pub async fn clear(org_id: &str) -> Result<()> {
    let cache = GenericCache::new(org_id)?;
    cache.clear_all()
}
