use std::io::Read as _;

use clap::Parser;

use tb_prod::api::ProductiveClient;
use tb_prod::cache::{self, Cache};
use tb_prod::commands;
use tb_prod::config::Config;

#[derive(Parser)]
#[command(
    name = "tb-prod",
    version,
    about = "Productive.io CLI — compact, AI-optimized"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// JSON output for all commands
    #[arg(long, global = true)]
    json: bool,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Manage tasks
    Task {
        #[command(subcommand)]
        action: TaskAction,
    },
    /// Manage todos
    Todo {
        #[command(subcommand)]
        action: TodoAction,
    },
    /// Manage comments
    Comment {
        #[command(subcommand)]
        action: CommentAction,
    },
    /// Project information
    Project {
        #[command(subcommand)]
        action: ProjectAction,
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
    /// Manage Claude Code skill file
    Skill {
        #[command(subcommand)]
        action: toolbox_core::skill::SkillAction,
    },
    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(clap::Subcommand)]
enum TaskAction {
    /// List tasks with filters
    List {
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
    Show {
        /// Task ID
        id: String,
    },
    /// Create a new task
    Create {
        /// Task title
        #[arg(long, required_unless_present = "batch")]
        title: Option<String>,
        /// Project (ID or name)
        #[arg(long, required_unless_present = "batch")]
        project: Option<String>,
        /// Task list (ID or name)
        #[arg(long, required_unless_present = "batch")]
        task_list: Option<String>,
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
        /// Batch create from JSON file (array of task objects)
        #[arg(long, conflicts_with_all = ["title", "project", "task_list", "status", "assignee", "description", "description_file", "description_stdin", "due_date"])]
        batch: Option<String>,
        /// Validate and show resolved payload without creating
        #[arg(long)]
        dry_run: bool,
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
}

#[derive(clap::Subcommand)]
enum TodoAction {
    /// List todos for a task
    List {
        /// Task ID
        task_id: String,
    },
    /// Create a todo on a task
    Create {
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
    Update {
        /// Todo ID
        todo_id: String,
        /// Mark done (true) or undone (false)
        #[arg(long)]
        done: Option<bool>,
        /// New title
        #[arg(long)]
        title: Option<String>,
    },
}

#[derive(clap::Subcommand)]
enum CommentAction {
    /// Add a comment
    Add {
        /// Entity ID (e.g. task ID)
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
}

#[derive(clap::Subcommand)]
enum ProjectAction {
    /// Show project context — statuses, task lists
    Show {
        /// Project ID or name
        project: String,
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
        /// Organization ID (auto-detected if omitted)
        #[arg(long)]
        org: Option<String>,
    },
    /// Show current config
    Show,
}

fn read_text_input(
    inline: Option<&str>,
    file: Option<&str>,
    from_stdin: bool,
) -> std::io::Result<Option<String>> {
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

toolbox_core::run_main!(run());

async fn run() -> tb_prod::error::Result<()> {
    let cli = Cli::parse();

    // Commands that don't need a loaded config
    if let Commands::Skill { action } = &cli.command {
        let skill = toolbox_core::skill::SkillConfig {
            tool_name: "tb-prod",
            content: include_str!("../SKILL.md"),
        };
        toolbox_core::skill::run(&skill, action).map_err(tb_prod::error::TbProdError::Other)?;
        return Ok(());
    }
    if let Commands::Config {
        action: ConfigAction::Init { token, org },
    } = &cli.command
    {
        commands::config_cmd::init(token, org.as_deref()).await?;
        return Ok(());
    }

    let config = Config::load()?;
    let client = ProductiveClient::new(&config);

    match cli.command {
        Commands::Task { action } => match action {
            TaskAction::List {
                task_list,
                project,
                assignee,
                category,
                search,
            } => {
                let cache = Cache::new(client.org_id())?;
                cache.ensure_fresh(&client).await?;

                let project_id = project
                    .as_deref()
                    .map(|p| cache.resolve_project(p))
                    .transpose()?;
                let task_list_id = match task_list.as_deref() {
                    Some(tl) => {
                        Some(cache::resolve_task_list(&client, tl, project_id.as_deref()).await?)
                    }
                    None => None,
                };
                let assignee_id = assignee
                    .as_deref()
                    .map(|a| cache.resolve_person(a))
                    .transpose()?;

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
            TaskAction::Show { id } => {
                commands::task::run(&client, &id, cli.json).await?;
            }
            TaskAction::Create {
                title,
                project,
                task_list,
                status,
                assignee,
                description,
                description_file,
                description_stdin,
                due_date,
                batch,
                dry_run,
            } => {
                let cache = Cache::new(client.org_id())?;
                cache.ensure_fresh(&client).await?;

                if let Some(batch_file) = batch {
                    let content = std::fs::read_to_string(&batch_file).map_err(|e| {
                        tb_prod::error::TbProdError::Other(format!(
                            "Cannot read batch file '{}': {}",
                            batch_file, e
                        ))
                    })?;
                    commands::task_batch::run(&client, &cache, &content, dry_run, cli.json).await?;
                } else {
                    let title = title.as_ref().unwrap();
                    let project = project.as_ref().unwrap();
                    let task_list = task_list.as_ref().unwrap();

                    let project_id = cache.resolve_project(project)?;
                    let workflow_id = cache.workflow_id_for_project(&project_id)?;
                    let task_list_id =
                        cache::resolve_task_list(&client, task_list, Some(&project_id)).await?;
                    let status_id = status
                        .as_deref()
                        .map(|s| cache.resolve_workflow_status(s, workflow_id.as_deref()))
                        .transpose()?;
                    let assignee_id = assignee
                        .as_deref()
                        .map(|a| cache.resolve_person(a))
                        .transpose()?;

                    let desc = read_text_input(
                        description.as_deref(),
                        description_file.as_deref(),
                        description_stdin,
                    )?;

                    if dry_run {
                        let resolved = serde_json::json!({
                            "title": title,
                            "project_id": project_id,
                            "task_list_id": task_list_id,
                            "workflow_status_id": status_id,
                            "assignee_id": assignee_id,
                            "description": desc.as_deref().map(|d| if d.len() > 100 { format!("{}...", &d[..100]) } else { d.to_string() }),
                            "due_date": due_date,
                        });
                        println!("{}", serde_json::to_string_pretty(&resolved)?);
                        eprintln!("Dry run — no task created.");
                    } else {
                        commands::task_create::run(
                            &client,
                            title,
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
                }
            }
            TaskAction::Update {
                id,
                status,
                title,
                assignee,
            } => {
                if status.is_none() && title.is_none() && assignee.is_none() {
                    return Err(tb_prod::error::TbProdError::Other(
                        "Provide at least one of --status, --title, or --assignee".into(),
                    ));
                }

                let cache = Cache::new(client.org_id())?;
                cache.ensure_fresh(&client).await?;

                let workflow_id = if status.is_some() {
                    let task_resp = client.get_task(&id).await?;
                    let project_id = task_resp.data.relationship_id("project");
                    project_id.and_then(|pid| cache.workflow_id_for_project(pid).ok().flatten())
                } else {
                    None
                };
                let status_id = status
                    .as_deref()
                    .map(|s| cache.resolve_workflow_status(s, workflow_id.as_deref()))
                    .transpose()?;
                let assignee_id = assignee
                    .as_deref()
                    .map(|a| cache.resolve_person(a))
                    .transpose()?;

                commands::task_update::run(
                    &client,
                    &id,
                    status_id.as_deref(),
                    title.as_deref(),
                    assignee_id.as_deref(),
                    cli.json,
                )
                .await?;
            }
        },
        Commands::Todo { action } => match action {
            TodoAction::List { task_id } => {
                commands::todos::list(&client, &task_id, cli.json).await?;
            }
            TodoAction::Create {
                task_id,
                title,
                assignee,
            } => {
                let assignee_id = if let Some(ref a) = assignee {
                    let cache = Cache::new(client.org_id())?;
                    cache.ensure_fresh(&client).await?;
                    Some(cache.resolve_person(a)?)
                } else {
                    None
                };
                commands::todos::create(
                    &client,
                    &task_id,
                    &title,
                    assignee_id.as_deref(),
                    cli.json,
                )
                .await?;
            }
            TodoAction::Update {
                todo_id,
                done,
                title,
            } => {
                if done.is_none() && title.is_none() {
                    return Err(tb_prod::error::TbProdError::Other(
                        "Provide at least one of --done or --title".into(),
                    ));
                }
                commands::todos::update(&client, &todo_id, done, title.as_deref(), cli.json)
                    .await?;
            }
        },
        Commands::Comment { action } => match action {
            CommentAction::Add {
                id,
                body,
                body_file,
                body_stdin,
            } => {
                let resolved_body =
                    read_text_input(body.as_deref(), body_file.as_deref(), body_stdin)?.ok_or(
                        tb_prod::error::TbProdError::Other(
                            "Provide BODY, --body-file, or --body-stdin".into(),
                        ),
                    )?;
                commands::task_comment::run(&client, &id, &resolved_body, cli.json).await?;
            }
        },
        Commands::Project { action } => match action {
            ProjectAction::Show { project } => {
                commands::project::run(&client, &project, cli.json).await?;
            }
        },
        Commands::Prime => {
            commands::prime::run(&client, &config).await?;
            toolbox_core::version_check::check("tb-prod", env!("CARGO_PKG_VERSION")).await;
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
        Commands::Skill { .. } => unreachable!(),
    }

    Ok(())
}
