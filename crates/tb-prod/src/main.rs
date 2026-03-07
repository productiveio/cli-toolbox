use std::io::Read as _;

use clap::Parser;


use tb_prod::api::ProductiveClient;
use tb_prod::cache::{self, Cache};
use tb_prod::commands;
use tb_prod::config::Config;

#[derive(Parser)]
#[command(name = "tb-prod", version, about = "Productive.io CLI — compact, AI-optimized")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// JSON output for all commands
    #[arg(long, global = true)]
    json: bool,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// List tasks with filters
    Tasks {
        /// Filter by task list (ID or name)
        #[arg(long)]
        task_list: Option<String>,
        /// Filter by project (ID or name)
        #[arg(long)]
        project: Option<String>,
        /// Filter by assignee (ID or name)
        #[arg(long)]
        assignee: Option<String>,
        /// Status category: open (default), all, closed, started, not-started
        #[arg(long)]
        category: Option<String>,
        /// Text search in task titles
        #[arg(long)]
        search: Option<String>,
    },
    /// Show single task detail
    Task {
        /// Task ID
        id: String,
    },
    /// Create a new task
    Create {
        /// Task title
        #[arg(long)]
        title: String,
        /// Project (ID or name)
        #[arg(long)]
        project: String,
        /// Task list (ID or name)
        #[arg(long)]
        task_list: String,
        /// Workflow status (ID or name)
        #[arg(long)]
        status: Option<String>,
        /// Assignee (ID or name)
        #[arg(long)]
        assignee: Option<String>,
        /// Description (HTML)
        #[arg(long)]
        description: Option<String>,
        /// Read description from file
        #[arg(long)]
        description_file: Option<String>,
        /// Read description from stdin
        #[arg(long)]
        description_stdin: bool,
        /// Due date (YYYY-MM-DD)
        #[arg(long)]
        due_date: Option<String>,
    },
    /// Update a task
    Update {
        /// Task ID
        id: String,
        /// New workflow status (ID or name)
        #[arg(long)]
        status: Option<String>,
        /// New title
        #[arg(long)]
        title: Option<String>,
        /// New assignee (ID or name)
        #[arg(long)]
        assignee: Option<String>,
    },
    /// Add a comment to a task
    Comment {
        /// Task ID
        id: String,
        /// Comment body (HTML or plain text)
        body: Option<String>,
        /// Read body from file
        #[arg(long)]
        body_file: Option<String>,
        /// Read body from stdin
        #[arg(long)]
        body_stdin: bool,
    },
    /// List todos for a task
    Todos {
        /// Task ID
        task_id: String,
    },
    /// Create a todo on a task
    TodoCreate {
        /// Task ID
        task_id: String,
        /// Todo title/description
        #[arg(long)]
        title: String,
        /// Assignee (ID or name)
        #[arg(long)]
        assignee: Option<String>,
    },
    /// Update a todo
    TodoUpdate {
        /// Todo ID
        todo_id: String,
        /// Mark done (true) or undone (false)
        #[arg(long)]
        done: Option<bool>,
        /// New title
        #[arg(long)]
        title: Option<String>,
    },
    /// Project context — statuses, task lists
    Project {
        /// Project ID or name
        project: String,
    },
    /// AI context dump — quick command reference
    Prime,
    /// Manage cache
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },
    /// Health check
    Doctor,
    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(clap::Subcommand)]
enum CacheAction {
    /// Sync all cached data from Productive
    Sync,
    /// Clear cached data
    Clear,
}

#[derive(clap::Subcommand)]
enum ConfigAction {
    /// Create initial config
    Init {
        #[arg(long)]
        token: String,
        #[arg(long)]
        org: String,
        #[arg(long)]
        person_id: Option<String>,
    },
    /// Show current config
    Show,
}

fn read_text_input(inline: Option<&str>, file: Option<&str>, from_stdin: bool) -> std::io::Result<Option<String>> {
    match (inline, file, from_stdin) {
        (Some(d), _, _) => Ok(Some(d.to_string())),
        (_, Some(path), _) => Ok(Some(std::fs::read_to_string(path)?)),
        (_, _, true) => {
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf)?;
            Ok(Some(buf))
        }
        _ => Ok(None),
    }
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Config init works before Config::load()
    if let Commands::Config {
        action: ConfigAction::Init { token, org, person_id },
    } = &cli.command
    {
        commands::config_cmd::init(token, org, person_id.as_deref())?;
        return Ok(());
    }

    let config = Config::load()?;
    let client = ProductiveClient::new(&config);

    match cli.command {
        Commands::Tasks {
            task_list,
            project,
            assignee,
            category,
            search,
        } => {
            let cache = Cache::new(client.org_id())?;
            cache.ensure_fresh(&client).await?;

            let project_id = project.as_deref().map(|p| cache.resolve_project(p)).transpose()?;
            let task_list_id = match task_list.as_deref() {
                Some(tl) => Some(cache::resolve_task_list(&client, tl, project_id.as_deref()).await?),
                None => None,
            };
            let assignee_id = assignee.as_deref().map(|a| cache.resolve_person(a)).transpose()?;

            commands::tasks::run(
                &client,
                &config,
                task_list_id.as_deref(),
                project_id.as_deref(),
                assignee_id.as_deref(),
                category.as_deref(),
                search.as_deref(),
                cli.json,
            )
            .await?;
        }
        Commands::Task { ref id } => {
            commands::task::run(&client, id, cli.json).await?;
        }
        Commands::Create {
            title,
            project,
            task_list,
            status,
            assignee,
            description,
            description_file,
            description_stdin,
            due_date,
        } => {
            let cache = Cache::new(client.org_id())?;
            cache.ensure_fresh(&client).await?;

            let project_id = cache.resolve_project(&project)?;
            let workflow_id = cache.workflow_id_for_project(&project_id)?;
            let task_list_id = cache::resolve_task_list(&client, &task_list, Some(&project_id)).await?;
            let status_id = status.as_deref().map(|s| cache.resolve_workflow_status(s, workflow_id.as_deref())).transpose()?;
            let assignee_id = assignee.as_deref().map(|a| cache.resolve_person(a)).transpose()?;

            let desc = read_text_input(description.as_deref(), description_file.as_deref(), description_stdin)?;

            commands::task_create::run(
                &client,
                &title,
                &project_id,
                &task_list_id,
                status_id.as_deref(),
                assignee_id.as_deref(),
                desc.as_deref(),
                due_date.as_deref(),
                cli.json,
            )
            .await?;
        }
        Commands::Update { ref id, status, title, assignee } => {
            if status.is_none() && title.is_none() && assignee.is_none() {
                return Err("Provide at least one of --status, --title, or --assignee".into());
            }

            let cache = Cache::new(client.org_id())?;
            cache.ensure_fresh(&client).await?;

            let workflow_id = if status.is_some() {
                let task_resp = client.get_task(id).await?;
                let project_id = task_resp.data.relationship_id("project");
                project_id.and_then(|pid| cache.workflow_id_for_project(pid).ok().flatten())
            } else {
                None
            };
            let status_id = status.as_deref().map(|s| cache.resolve_workflow_status(s, workflow_id.as_deref())).transpose()?;
            let assignee_id = assignee.as_deref().map(|a| cache.resolve_person(a)).transpose()?;

            commands::task_update::run(
                &client,
                id,
                status_id.as_deref(),
                title.as_deref(),
                assignee_id.as_deref(),
                cli.json,
            )
            .await?;
        }
        Commands::Comment { ref id, body, body_file, body_stdin } => {
            let resolved_body = read_text_input(body.as_deref(), body_file.as_deref(), body_stdin)?
                .ok_or("Provide BODY, --body-file, or --body-stdin")?;
            commands::task_comment::run(&client, id, &resolved_body, cli.json).await?;
        }
        Commands::Todos { ref task_id } => {
            commands::todos::list(&client, task_id, cli.json).await?;
        }
        Commands::TodoCreate { ref task_id, title, assignee } => {
            let assignee_id = if let Some(ref a) = assignee {
                let cache = Cache::new(client.org_id())?;
                cache.ensure_fresh(&client).await?;
                Some(cache.resolve_person(a)?)
            } else {
                None
            };
            commands::todos::create(&client, task_id, &title, assignee_id.as_deref(), cli.json).await?;
        }
        Commands::TodoUpdate { ref todo_id, done, title } => {
            if done.is_none() && title.is_none() {
                return Err("Provide at least one of --done or --title".into());
            }
            commands::todos::update(&client, todo_id, done, title.as_deref(), cli.json).await?;
        }
        Commands::Project { ref project } => {
            commands::project::run(&client, project, cli.json).await?;
        }
        Commands::Prime => {
            commands::prime::run(&client, &config).await?;
        }
        Commands::Cache { action } => match action {
            CacheAction::Sync => {
                commands::cache_cmd::sync(&client).await?;
            }
            CacheAction::Clear => {
                commands::cache_cmd::clear(client.org_id()).await?;
            }
        },
        Commands::Doctor => {
            commands::doctor::run(&client, &config).await?;
        }
        Commands::Config { action } => match action {
            ConfigAction::Init { .. } => unreachable!(),
            ConfigAction::Show => {
                commands::config_cmd::show(&config);
            }
        },
    }

    Ok(())
}
