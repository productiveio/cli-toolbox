use crate::api::SemaphoreClient;
use crate::config::Config;
use crate::error::Result;
use crate::output;

pub async fn run(client: &SemaphoreClient, config: &Config, mcp: bool, utc: bool) -> Result<()> {
    let tz = if utc {
        chrono_tz::UTC
    } else {
        config.timezone()
    };

    // Get latest run for each project
    let mut project_statuses = Vec::new();
    for (name, proj) in &config.projects {
        let workflows = client
            .list_workflows(&proj.id, None, None, None)
            .await
            .unwrap_or_default();

        if let Some(wf) = workflows.first() {
            let ppl = client.get_pipeline(&wf.initial_ppl_id, false).await.ok();
            let result = ppl
                .as_ref()
                .map(|p| p.result.to_uppercase())
                .unwrap_or_else(|| "?".to_string());
            let time = output::epoch_to_local(wf.created_at.seconds, &tz);
            project_statuses.push((name.clone(), time, result));
        }
    }

    if mcp {
        // Minimal output for hooks
        let mut parts = Vec::new();
        parts.push("# Semaphore CI Active".to_string());
        let names: Vec<&str> = config.projects.keys().map(|s| s.as_str()).collect();
        parts.push(format!("Projects: {}", names.join(", ")));
        for (name, time, result) in &project_statuses {
            if result == "FAILED" {
                parts.push(format!("Last {} run: {} -- {}", name, time, result));
            }
        }
        parts.push("Commands: `tb-sem triage`, `tb-sem runs <project> --failed`".to_string());
        println!("{}", parts.join("\n"));
    } else {
        println!("# Semaphore CI Active\n");
        println!("Organization: {}", config.org_id);
        println!("Timezone: {}\n", config.timezone);

        println!("## Projects");
        for (name, time, result) in &project_statuses {
            println!("  {:<16} Last run: {} -- {}", name, time, result);
        }

        println!("\n## Quick Commands");
        println!("- `tb-sem triage` - Full triage of latest failed e2e run");
        println!("- `tb-sem runs e2e-tests --failed --limit 5` - Recent failed runs");
        println!("- `tb-sem failures <pipeline-id>` - Parsed failure summary");
        println!("- `tb-sem deploys api --around <pipeline-id>` - Deploy overlap check");

        println!("\n## E2E Triage Workflow");
        println!("1. `tb-sem runs e2e-tests --failed` -> find the run");
        println!("2. `tb-sem failures <pipeline-id>` -> see what failed and why");
        println!("3. `tb-sem deploys api --around <pipeline-id>` -> check deploy overlap");
        println!("Or just: `tb-sem triage` for all-in-one");
    }

    Ok(())
}
