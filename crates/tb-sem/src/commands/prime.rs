use crate::api::SemaphoreClient;
use crate::config::Config;
use crate::error::Result;

pub async fn run(_client: &SemaphoreClient, config: &Config, mcp: bool, _utc: bool) -> Result<()> {
    let names: Vec<&str> = config.projects.keys().map(|s| s.as_str()).collect();
    let p1 = names.first().copied().unwrap_or("<project>");
    let p2 = if names.len() > 1 { names[1] } else { p1 };

    if mcp {
        let mut parts = Vec::new();
        parts.push("# Semaphore CI Active".to_string());
        parts.push(format!("Projects: {}", names.join(", ")));
        parts.push(format!(
            "Commands: `tb-sem triage {}`, `tb-sem runs {} --failed`",
            p1, p1
        ));
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
        println!(
            "- `tb-sem triage {} --branch master` - Full triage of latest failed run",
            p1
        );
        println!(
            "- `tb-sem runs {} --failed --limit 5` - Recent failed runs (last 7 days, all branches)",
            p1
        );
        println!("- `tb-sem failures <pipeline-id>` - Parsed failure summary");
        println!(
            "- `tb-sem deploys {} --branch master --around <pipeline-id>` - Deploy overlap check",
            p2
        );

        println!("\n## Triage Workflow\n");
        println!("1. `tb-sem runs {} --failed` -> find the run", p1);
        println!("2. `tb-sem failures <pipeline-id>` -> see what failed and why");
        println!(
            "3. `tb-sem deploys {} --branch master --around <pipeline-id>` -> check deploy overlap",
            p2
        );
        println!(
            "Or: `tb-sem triage {} --deploy-project {}` for all-in-one",
            p1, p2
        );

        println!("\n## Branch Filtering\n");
        println!("- `deploys` requires `--branch` (deploys are branch-specific)");
        println!(
            "- `runs`, `flaky`, `history` — `--branch` is optional; without it, returns cross-branch results from the last 7 days"
        );
        println!(
            "- `branches` — lists recently active branches (default: last 7 days, use `--days` to adjust)"
        );
    }

    Ok(())
}
