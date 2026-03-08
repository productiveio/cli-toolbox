use crate::api::ProductiveClient;
use crate::cache::Cache;
use crate::config::Config;
use crate::error::Result;

pub async fn run(client: &ProductiveClient, config: &Config) -> Result<()> {
    let cache = Cache::new(client.org_id())?;
    cache.ensure_fresh(client).await?;

    let people = cache.people()?;

    let user_name = config
        .person_id
        .as_deref()
        .and_then(|pid| people.iter().find(|p| p.id == pid))
        .map(|p| format!("{} {}", p.first_name, p.last_name))
        .unwrap_or_else(|| "Unknown".to_string());
    let person_id = config.person_id.as_deref().unwrap_or("?");

    println!("# Productive.io Context (org: {})\n", client.org_id());

    println!("## User");
    println!("{} (person_id: {})\n", user_name, person_id);

    println!("## Commands");
    println!("- `tb-prod project show \"X\"` — project context (statuses, task lists)");
    println!("- `tb-prod task list` — my open tasks");
    println!("- `tb-prod task list --project \"X\" --category all` — all tasks in project");
    println!(
        "- `tb-prod task list --task-list ID --category all --search \"keyword\"` — search in task list"
    );
    println!("- `tb-prod task show ID` — task detail with subtasks, todos, comments");
    println!(
        "- `tb-prod task create --title \"...\" --project \"X\" --task-list \"Y\"` — create task"
    );
    println!("- `tb-prod task update ID --status \"Done\"` — update status");
    println!("- `tb-prod task update ID --title \"...\" --assignee \"Name\"` — update fields");
    println!("- `tb-prod comment add ID \"message\"` — add comment");
    println!("- `tb-prod comment add ID --body-stdin` — comment from stdin");
    println!("- `tb-prod todo create TASK_ID --title \"...\"` — add todo");
    println!("- `tb-prod todo update TODO_ID --done true` — complete todo");
    println!("- `tb-prod todo list TASK_ID` — list todos\n");

    println!("## Notes");
    println!("- `--project`, `--status`, `--assignee`, `--task-list` accept names (fuzzy matched)");
    println!(
        "- Ambiguous or wrong names show available options — just guess and the CLI will help"
    );
    println!("- `tb-prod cache sync` to refresh cached data");

    Ok(())
}
