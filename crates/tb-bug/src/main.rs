use clap::Parser;

use tb_bug::api::BugsnagClient;
use tb_bug::commands;
use tb_bug::config::Config;

#[derive(Parser)]
#[command(name = "tb-bug", version, about = "Bugsnag insights CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Bypass response cache
    #[arg(long, global = true)]
    no_cache: bool,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// List errors with filters
    Errors {
        /// Project name or ID
        #[arg(long)]
        project: String,
        /// Filter by status: open, fixed, snoozed, ignored
        #[arg(long)]
        status: Option<String>,
        /// Filter by severity: error, warning, info
        #[arg(long)]
        severity: Option<String>,
        /// Only errors seen after this time (e.g., today, yesterday, 1d, 7d, 24h, 2026-03-01)
        #[arg(long)]
        since: Option<String>,
        /// Filter by release stage (e.g., production)
        #[arg(long)]
        stage: Option<String>,
        /// Filter by error class (substring match)
        #[arg(long)]
        class: Option<String>,
        /// Sort by: last_seen, first_seen, users, events
        #[arg(long)]
        sort: Option<String>,
        /// Max results (default: 30)
        #[arg(long, default_value = "30")]
        limit: usize,
        /// JSON output
        #[arg(long)]
        json: bool,
        /// Show all columns, no truncation
        #[arg(long)]
        long: bool,
    },
    /// AI-optimized context dump
    Prime {
        /// Project name or ID (omit for overview of all configured projects)
        #[arg(long)]
        project: Option<String>,
    },
    /// List projects in organization
    Projects {
        /// JSON output
        #[arg(long)]
        json: bool,
    },
    /// List recent events for an error
    Events {
        /// Project name or ID
        #[arg(long)]
        project: String,
        /// Error ID
        error_id: String,
        /// Max results (default: 30)
        #[arg(long, default_value = "30")]
        limit: usize,
        /// JSON output
        #[arg(long)]
        json: bool,
    },
    /// Fetch full detail (raw JSON)
    Fetch {
        #[command(subcommand)]
        target: FetchTarget,
    },
    /// List releases with error counts
    Releases {
        /// Project name or ID
        #[arg(long)]
        project: String,
        /// Max results (default: 30)
        #[arg(long, default_value = "30")]
        limit: usize,
        /// JSON output
        #[arg(long)]
        json: bool,
    },
    /// Project-level trend data
    Trends {
        /// Project name or ID
        #[arg(long)]
        project: String,
        /// JSON output
        #[arg(long)]
        json: bool,
    },
    /// Crash-free session rates over time
    Stability {
        /// Project name or ID
        #[arg(long)]
        project: String,
        /// JSON output
        #[arg(long)]
        json: bool,
    },
    /// Reports
    Report {
        #[command(subcommand)]
        kind: ReportKind,
    },
    /// Search error classes and messages
    Search {
        /// Project name or ID
        #[arg(long)]
        project: String,
        /// Search query
        query: String,
        /// Max results (default: 30)
        #[arg(long, default_value = "30")]
        limit: usize,
        /// JSON output
        #[arg(long)]
        json: bool,
    },
    /// Health check for CLI setup
    Doctor,
    /// Manage Claude Code skill file
    Skill {
        #[command(subcommand)]
        action: toolbox_core::skill::SkillAction,
    },
    /// Clear the response cache
    CacheClear,
    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(clap::Subcommand)]
enum ConfigAction {
    /// Create initial config file
    Init {
        /// Bugsnag auth token
        #[arg(long)]
        token: String,
        /// Organization ID
        #[arg(long)]
        org: String,
    },
    /// Display current config
    Show,
    /// Add a named project
    AddProject {
        /// Project name (alias for CLI use)
        name: String,
        /// Bugsnag project ID
        project_id: String,
    },
    /// Remove a named project
    RemoveProject {
        /// Project name to remove
        name: String,
    },
}

#[derive(clap::Subcommand)]
enum ReportKind {
    /// Combined stability + errors + release overview
    Dashboard {
        /// Project name or ID
        #[arg(long)]
        project: String,
    },
    /// Open errors sorted by impact
    Open {
        /// Project name or ID
        #[arg(long)]
        project: String,
        /// Max results (default: 30)
        #[arg(long, default_value = "30")]
        limit: usize,
        /// JSON output
        #[arg(long)]
        json: bool,
    },
}

#[derive(clap::Subcommand)]
enum FetchTarget {
    /// Fetch full error detail
    Error {
        /// Project name or ID
        #[arg(long)]
        project: String,
        /// Error ID
        error_id: String,
    },
    /// Fetch full event detail
    Event {
        /// Project name or ID
        #[arg(long)]
        project: String,
        /// Event ID
        event_id: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("--project")
                && let Ok(config) = Config::load() {
                    let names: Vec<_> = config.projects.keys().map(|s| s.as_str()).collect();
                    if !names.is_empty() {
                        eprintln!("{e}");
                        eprintln!("Available projects: {}\n", names.join(", "));
                        std::process::exit(2);
                    }
                }
            e.exit();
        }
    };

    // Commands that don't need a loaded config
    if let Commands::Skill { action } = &cli.command {
        let skill = toolbox_core::skill::SkillConfig {
            tool_name: "tb-bug",
            content: include_str!("../SKILL.md"),
        };
        toolbox_core::skill::run(&skill, action).map_err(|e| e.to_string())?;
        return Ok(());
    }
    if let Commands::Config { action: ConfigAction::Init { token, org } } = &cli.command {
        commands::config_cmd::init(token, org)?;
        return Ok(());
    }

    let config = Config::load()?;
    let client = BugsnagClient::new(&config, cli.no_cache)?;

    match &cli.command {
        Commands::Errors {
            project, status, severity, since, stage, class,
            sort, limit, json, long,
        } => {
            commands::errors::run(
                &client,
                &config,
                project,
                status.as_deref(),
                severity.as_deref(),
                since.as_deref(),
                stage.as_deref(),
                class.as_deref(),
                sort.as_deref(),
                *limit,
                *json,
                *long,
            )
            .await?;
        }
        Commands::Prime { project } => {
            commands::prime::run(&client, &config, project.as_deref()).await?;
        }
        Commands::Projects { json } => {
            commands::projects::run(&client, &config, *json).await?;
        }
        Commands::Events { project, error_id, limit, json } => {
            commands::events::run(&client, &config, project, error_id, *limit, *json).await?;
        }
        Commands::Fetch { target } => {
            match target {
                FetchTarget::Error { project, error_id } => {
                    commands::fetch::run_error(&client, &config, project, error_id).await?;
                }
                FetchTarget::Event { project, event_id } => {
                    commands::fetch::run_event(&client, &config, project, event_id).await?;
                }
            }
        }
        Commands::Releases { project, limit, json } => {
            commands::releases::run(&client, &config, project, *limit, *json).await?;
        }
        Commands::Trends { project, json } => {
            commands::trends::run(&client, &config, project, *json).await?;
        }
        Commands::Stability { project, json } => {
            commands::stability::run(&client, &config, project, *json).await?;
        }
        Commands::Report { kind } => {
            match kind {
                ReportKind::Dashboard { project } => {
                    commands::report::run_dashboard(&client, &config, project).await?;
                }
                ReportKind::Open { project, limit, json } => {
                    commands::report::run_open(&client, &config, project, *limit, *json).await?;
                }
            }
        }
        Commands::Search { project, query, limit, json } => {
            commands::search::run(&client, &config, project, query, *limit, *json).await?;
        }
        Commands::Doctor => {
            commands::doctor::run(&client, &config).await?;
        }
        Commands::CacheClear => {
            client.clear_cache()?;
            println!("Cache cleared.");
        }
        Commands::Skill { .. } => unreachable!(),
        Commands::Config { action } => {
            match action {
                ConfigAction::Init { .. } => unreachable!(),
                ConfigAction::Show => {
                    commands::config_cmd::show(&config);
                }
                ConfigAction::AddProject { name, project_id } => {
                    let mut cfg = Config::load()?;
                    commands::config_cmd::add_project(&mut cfg, name, project_id)?;
                }
                ConfigAction::RemoveProject { name } => {
                    let mut cfg = Config::load()?;
                    commands::config_cmd::remove_project(&mut cfg, name)?;
                }
            }
        }
    }

    Ok(())
}
