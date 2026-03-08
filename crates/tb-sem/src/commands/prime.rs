use crate::api::SemaphoreClient;
use crate::config::Config;
use crate::error::Result;

pub async fn run(_client: &SemaphoreClient, config: &Config, mcp: bool, _utc: bool) -> Result<()> {
    let names: Vec<&str> = config.projects.keys().map(|s| s.as_str()).collect();

    if mcp {
        let mut parts = Vec::new();
        parts.push("# Semaphore CI Active".to_string());
        parts.push(format!("Projects: {}", names.join(", ")));
        parts.push("Commands: `tb-sem triage`, `tb-sem runs <project> --failed`".to_string());
        println!("{}", parts.join("\n"));
    } else {
        println!("# Semaphore CI Active\n");
        println!("Organization: {}", config.org_id);
        println!("Timezone: {}\n", config.timezone);

        println!("## Projects\n");
        for name in &names {
            println!("- {}", name);
        }

        println!("\n## Quick Commands\n");
        println!("- `tb-sem triage` - Full triage of latest failed e2e run");
        println!("- `tb-sem runs e2e-tests --failed --limit 5` - Recent failed runs");
        println!("- `tb-sem failures <pipeline-id>` - Parsed failure summary");
        println!("- `tb-sem deploys api --around <pipeline-id>` - Deploy overlap check");

        println!("\n## E2E Triage Workflow\n");
        println!("1. `tb-sem runs e2e-tests --failed` -> find the run");
        println!("2. `tb-sem failures <pipeline-id>` -> see what failed and why");
        println!("3. `tb-sem deploys api --around <pipeline-id>` -> check deploy overlap");
        println!("Or just: `tb-sem triage` for all-in-one");
    }

    Ok(())
}
