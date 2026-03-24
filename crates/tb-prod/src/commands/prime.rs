use crate::api::{ProductiveClient, Query};
use crate::config::Config;
use crate::error::Result;
use crate::generic_cache::GenericCache;
use crate::schema;

pub async fn run(client: &ProductiveClient, config: &Config) -> Result<()> {
    let s = schema::schema();
    let cache = GenericCache::new(client.org_id())?;

    // Ensure org cache is fresh
    if cache.is_org_stale("projects") {
        cache.sync_org(client).await?;
    }

    // Look up user name from cache by ID
    let person_id = config.person_id.as_deref().unwrap_or("?");
    let user_name = config
        .person_id
        .as_deref()
        .and_then(|pid| {
            let people = cache.read_org_cache("people").ok()?;
            people.into_iter().find(|r| r.id == pid).map(|r| {
                let first = r.fields.get("first_name").map(|s| s.as_str()).unwrap_or("");
                let last = r.fields.get("last_name").map(|s| s.as_str()).unwrap_or("");
                format!("{} {}", first, last).trim().to_string()
            })
        })
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "Unknown".to_string());

    // --- Header ---
    println!("# Productive.io Context (org: {})\n", client.org_id());
    println!("## User");
    println!("{} (person_id: {})\n", user_name, person_id);

    // --- Commands ---
    println!("## Commands\n");
    println!("```");
    println!("tb-prod describe <type> [--include schema,actions,related]");
    println!(
        "tb-prod query <type> [--filter <json>] [--sort <field>] [--page <n>] [--include <rels>]"
    );
    println!("tb-prod get <type> <id> [--include <rels>]");
    println!("tb-prod create <type> --data <json>");
    println!("tb-prod update <type> <id> --data <json>");
    println!("tb-prod delete <type> <id> [--confirm]");
    println!("tb-prod search <type> --query <text>");
    println!("tb-prod action <type> <id> <action-name> [--data <json>]");
    println!("tb-prod prime [project <name>]");
    println!("```\n");

    // --- Resource types by domain ---
    println!("## Resource Types\n");
    let grouped = s.resources_by_domain();
    for (domain, resources) in &grouped {
        println!("### {}", domain);
        for r in resources {
            println!("- **{}** — {}", r.type_name, r.description_short);
        }
        println!();
    }

    // --- Notes ---
    println!("## Notes\n");
    println!("- Default output is CSV with resolved relationship names (use `--format json` for raw JSON)");
    println!("- Default filters auto-apply (e.g. tasks auto-scoped to open tasks in active projects). Only add filters you need.");
    println!("- `--filter` accepts JSON: flat `{{\"field\": \"value\"}}` or operator `{{\"field\": {{\"not_eq\": \"value\"}}}}`");
    println!(
        "- Filter values for cacheable types (projects, people, etc.) auto-resolve names to IDs"
    );
    println!("- `tb-prod describe <type>` for full field/filter/action details");
    println!("- `tb-prod prime project <name>` for deep project context");
    println!("- `tb-prod cache sync` to refresh cached data");

    // --- Common queries ---
    println!("\n## Common Queries\n");
    println!("```");
    println!(
        "tb-prod query tasks --filter '{{\"assignee_id\": \"{}\"}}'",
        person_id
    );
    println!("tb-prod query projects");
    println!("tb-prod query time_entries --filter '{{\"person_id\": \"{}\"}}'", person_id);
    println!("tb-prod query bookings --filter '{{\"person_id\": \"{}\"}}'", person_id);
    println!("```");

    Ok(())
}

pub async fn run_project(client: &ProductiveClient, project_name_or_id: &str) -> Result<()> {
    let cache = GenericCache::new(client.org_id())?;

    // Ensure org cache is fresh for project resolution
    if cache.is_org_stale("projects") {
        cache.sync_org(client).await?;
    }

    // Resolve project — try cache first, fall back to API for non-active projects
    let project_id = match cache.resolve_name("projects", project_name_or_id, None) {
        Ok(id) => id,
        Err(_) if !project_name_or_id.chars().all(|c| c.is_ascii_digit()) => {
            // Cache miss — project may be archived/non-active (cache only has active).
            // Query ALL projects from API and search by name.
            eprintln!("Not found in cache (active projects only), searching all projects...");
            let resp = client.get_all("/projects", &Query::new(), 10).await?;
            let needle = project_name_or_id.to_lowercase();
            let matches: Vec<_> = resp
                .data
                .iter()
                .filter(|r| r.attr_str("name").to_lowercase().contains(&needle))
                .collect();
            match matches.len() {
                0 => {
                    return Err(crate::error::TbProdError::Other(format!(
                        "No project matching '{}'.",
                        project_name_or_id
                    )));
                }
                1 => matches[0].id.clone(),
                _ => {
                    let list: Vec<String> = matches
                        .iter()
                        .map(|r| format!("  {} ({})", r.attr_str("name"), r.id))
                        .collect();
                    return Err(crate::error::TbProdError::Other(format!(
                        "Ambiguous project '{}'. Matches:\n{}",
                        project_name_or_id,
                        list.join("\n")
                    )));
                }
            }
        }
        Err(e) => return Err(e),
    };

    // Fetch project with workflow
    let path = format!("/projects/{}?include=workflow", project_id);
    let project_resp = client.get_one(&path).await?;
    let project = &project_resp.data;

    let project_name = project.attr_str("name");
    let workflow_id = project.relationship_id("workflow").map(|s| s.to_string());

    println!("# Project: {} (ID: {})\n", project_name, project_id);
    println!(
        "Status: {}",
        if project.attr_str("status") == "1" {
            "Active"
        } else {
            "Archived"
        }
    );

    // Workflow statuses
    if let Some(wf_id) = &workflow_id {
        let status_query = Query::new().filter_array("workflow_id", wf_id);
        let statuses_resp = client
            .get_all("/workflow_statuses", &status_query, 5)
            .await?;

        println!("\n## Workflow Statuses\n");
        for status in &statuses_resp.data {
            let category = match status
                .attributes
                .get("category_id")
                .and_then(|v| {
                    v.as_i64()
                        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                })
                .unwrap_or(0)
            {
                1 => "not started",
                2 => "started",
                3 => "closed",
                _ => "unknown",
            };
            println!(
                "- {} (ID: {}, {})",
                status.attr_str("name"),
                status.id,
                category
            );
        }
    }

    // Task lists with folders
    let tl_query = Query::new()
        .filter("project_id", &project_id)
        .filter("status", "1")
        .include("board");
    let task_lists_resp = client.get_all("/task_lists", &tl_query, 5).await?;

    println!("\n## Task Lists\n");
    for tl in &task_lists_resp.data {
        let folder_name = tl
            .relationship_id("board")
            .and_then(|fid| {
                task_lists_resp
                    .included
                    .iter()
                    .find(|r| r.resource_type == "folders" && r.id == fid)
            })
            .map(|f| f.attr_str("name").to_string());

        let prefix = folder_name.map(|f| format!("[{}] ", f)).unwrap_or_default();

        println!("- {}{} (ID: {})", prefix, tl.attr_str("name"), tl.id);
    }

    // Warm project-scoped cache
    eprintln!("\nWarming project cache...");
    cache
        .sync_project(client, &project_id, workflow_id.as_deref())
        .await?;
    eprintln!("Project cache warmed.");

    Ok(())
}
