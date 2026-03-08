use crate::api::ProductiveClient;
use crate::config::Config;
use crate::error::Result;

pub async fn run(client: &ProductiveClient, config: &Config) -> Result<()> {
    println!("tb-prod doctor");
    println!("  org_id:    {}", config.org_id);
    println!(
        "  person_id: {}",
        config.person_id.as_deref().unwrap_or("(not set)")
    );
    println!("  token:     {}", config.masked_token());
    println!("  base_url:  {}", config.base_url());

    // Quick connectivity check — fetch one task to verify auth
    let query = crate::api::Query::new();
    match client.get_page("/tasks", &query, 1, 1).await {
        Ok(resp) => {
            let total = resp
                .meta
                .get("total_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            println!("  status:    OK (total tasks visible: {})", total);
        }
        Err(e) => {
            println!("  status:    FAILED — {}", e);
        }
    }

    Ok(())
}
