use crate::api::BugsnagClient;
use crate::config::Config;
use crate::error::Result;
use crate::output;

pub async fn run(client: &BugsnagClient, config: &Config, json: bool) -> Result<()> {
    let projects = client.list_projects(&config.org_id).await?;

    if json {
        println!("{}", output::render_json(&projects));
    } else {
        println!(
            "{:<30} {:<20} {:<12} {:>8}",
            "NAME", "SLUG", "LANGUAGE", "OPEN"
        );
        for p in &projects {
            println!(
                "{:<30} {:<20} {:<12} {:>8}",
                p.name, p.slug, p.language, p.open_error_count
            );
        }
    }

    Ok(())
}
