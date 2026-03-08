use std::time::Instant;

use crate::api::SemaphoreClient;
use crate::config::Config;
use crate::error::Result;

pub async fn run(config: &Config) -> Result<()> {
    let client = SemaphoreClient::new(config);

    // Token & org check
    let start = Instant::now();
    match client.list_projects().await {
        Ok(projects) => {
            let latency = start.elapsed();

            println!("Token:        {} valid", config.masked_token());
            println!("Org access:   {} (OK)", config.org_id);
            println!("API latency:  {}ms", latency.as_millis());
            println!("\nProjects:");

            for (name, proj_config) in &config.projects {
                let accessible = projects.iter().any(|p| p.metadata.id == proj_config.id);
                let status = if accessible {
                    "accessible"
                } else {
                    "NOT FOUND"
                };
                println!("  {:<20} {} ({})", name, &proj_config.id, status);
            }
        }
        Err(e) => {
            println!("Token:        INVALID or expired");
            println!("Error:        {}", e);
        }
    }

    Ok(())
}
