use std::time::Instant;

use crate::api::BugsnagClient;
use crate::config::Config;
use crate::error::Result;
use crate::output;

pub async fn run(client: &BugsnagClient, config: &Config) -> Result<()> {
    let start = Instant::now();
    match client.list_organizations().await {
        Ok(orgs) => {
            let latency = start.elapsed();
            println!("Token:        {} valid", config.masked_token());

            let org_name = orgs
                .iter()
                .find(|o| o.id == config.org_id)
                .map(|o| o.name.as_str())
                .unwrap_or("?");
            println!("Organization: {} ({})", config.org_id, org_name);
            println!("API latency:  {}ms", latency.as_millis());

            let projects = client.list_projects(&config.org_id).await?;
            println!("\nConfigured projects:");
            for (name, proj_config) in &config.projects {
                let found = projects.iter().find(|p| p.id == proj_config.id);
                let status = if found.is_some() { "OK" } else { "NOT FOUND" };
                let open_errors = found
                    .map(|p| format!("{} open errors", p.open_error_count))
                    .unwrap_or_default();
                let short_id = output::truncate(&proj_config.id, 12);
                println!("  {:<20} {} ({}) {}", name, short_id, status, open_errors);
            }
        }
        Err(e) => {
            println!("Token:        INVALID or expired");
            println!("Error:        {}", e);
        }
    }

    Ok(())
}
