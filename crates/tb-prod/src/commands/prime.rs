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
    println!("- `tb-prod project \"X\"` — project context (statuses, task lists)");
    println!("- `tb-prod tasks` — my open tasks");
    println!("- `tb-prod tasks --project \"X\" --category all` — all tasks in project");
    println!("- `tb-prod tasks --task-list ID --category all --search \"keyword\"` — search in task list");
    println!("- `tb-prod task ID` — task detail with subtasks, todos, comments");
    println!("- `tb-prod create --title \"...\" --project \"X\" --task-list \"Y\"` — create task");
    println!("- `tb-prod update ID --status \"Done\"` — update status");
    println!("- `tb-prod update ID --title \"...\" --assignee \"Name\"` — update fields");
    println!("- `tb-prod comment ID \"message\"` — add comment");
    println!("- `tb-prod comment ID --body-stdin` — comment from stdin");
    println!("- `tb-prod todo-create TASK_ID --title \"...\"` — add todo");
    println!("- `tb-prod todo-update TODO_ID --done true` — complete todo");
    println!("- `tb-prod todos TASK_ID` — list todos\n");

    println!("## Notes");
    println!("- `--project`, `--status`, `--assignee`, `--task-list` accept names (fuzzy matched)");
    println!("- Ambiguous or wrong names show available options — just guess and the CLI will help");
    println!("- `tb-prod cache sync` to refresh cached data");

    Ok(())
}
