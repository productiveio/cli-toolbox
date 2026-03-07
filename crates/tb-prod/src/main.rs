use clap::Parser;

use tb_prod::api::ProductiveClient;
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
        /// Filter by task list ID
        #[arg(long)]
        task_list: Option<String>,
        /// Filter by project ID
        #[arg(long)]
        project: Option<String>,
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
        /// Project ID (required)
        #[arg(long)]
        project: String,
        /// Task list ID (required)
        #[arg(long)]
        task_list: String,
        /// Workflow status ID (optional — defaults to project's default)
        #[arg(long)]
        status: Option<String>,
        /// Assignee person ID
        #[arg(long)]
        assignee: Option<String>,
        /// Description (HTML)
        #[arg(long)]
        description: Option<String>,
        /// Read description from file
        #[arg(long)]
        description_file: Option<String>,
        /// Due date (YYYY-MM-DD)
        #[arg(long)]
        due_date: Option<String>,
    },
    /// Update a task's workflow status
    Update {
        /// Task ID
        id: String,
        /// New workflow status ID
        #[arg(long)]
        status: String,
    },
    /// Add a comment to a task
    Comment {
        /// Task ID
        id: String,
        /// Comment body (HTML or plain text)
        body: String,
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

    match &cli.command {
        Commands::Tasks {
            task_list,
            project,
            category,
            search,
        } => {
            commands::tasks::run(
                &client,
                &config,
                task_list.as_deref(),
                project.as_deref(),
                category.as_deref(),
                search.as_deref(),
                cli.json,
            )
            .await?;
        }
        Commands::Task { id } => {
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
            due_date,
        } => {
            let desc = match (description.as_deref(), description_file.as_deref()) {
                (Some(d), _) => Some(d.to_string()),
                (_, Some(path)) => Some(std::fs::read_to_string(path)?),
                _ => None,
            };
            commands::task_create::run(
                &client,
                title,
                project,
                task_list,
                status.as_deref(),
                assignee.as_deref(),
                desc.as_deref(),
                due_date.as_deref(),
                cli.json,
            )
            .await?;
        }
        Commands::Update { id, status } => {
            commands::task_update::run(&client, id, status, cli.json).await?;
        }
        Commands::Comment { id, body } => {
            commands::task_comment::run(&client, id, body, cli.json).await?;
        }
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
