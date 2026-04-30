use clap::Parser;

use tb_bug::api::BugsnagClient;
use tb_bug::commands;
use tb_bug::config::Config;

#[derive(Parser)]
#[command(
    name = "tb-bug",
    disable_version_flag = true,
    about = "Bugsnag insights CLI"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Bypass response cache
    #[arg(long, global = true)]
    no_cache: bool,

    /// Print version info
    #[arg(short = 'V', long = "version")]
    version: bool,
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
        #[command(flatten)]
        time: toolbox_core::time_range::TimeRange,
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
    /// Mutate an error's state (fix, ignore, discard, snooze)
    Error {
        #[command(subcommand)]
        action: ErrorAction,
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
        /// Bugsnag auth token (prompted interactively if omitted)
        #[arg(long)]
        token: Option<String>,
        /// Organization ID (auto-detected if omitted)
        #[arg(long)]
        org: Option<String>,
        /// Project slugs to add (comma-separated, e.g. "api,app,ai-agent")
        #[arg(long)]
        projects: Option<String>,
    },
    /// Display current config
    Show,
    /// Set a config value (use "project" key to manage projects)
    Set {
        /// Config key (token, org_id, project)
        key: String,
        /// New value (optional for project — launches interactive selection)
        value: Option<String>,
        /// Add a project by slug (only with key=project)
        #[arg(long)]
        add: Option<String>,
        /// Remove a project by slug (only with key=project)
        #[arg(long)]
        remove: Option<String>,
    },
}

#[derive(clap::Subcommand)]
enum ErrorAction {
    /// Mark errors as fixed
    Fix {
        /// Project name or ID
        #[arg(long)]
        project: String,
        /// Error IDs (one or more)
        #[arg(required = true)]
        error_ids: Vec<String>,
    },
    /// Ignore errors
    Ignore {
        /// Project name or ID
        #[arg(long)]
        project: String,
        /// Error IDs (one or more)
        #[arg(required = true)]
        error_ids: Vec<String>,
    },
    /// Discard errors (drops future events for this error class)
    Discard {
        /// Project name or ID
        #[arg(long)]
        project: String,
        /// Error IDs (one or more)
        #[arg(required = true)]
        error_ids: Vec<String>,
        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,
    },
    /// Snooze errors for a duration or until N more events
    Snooze {
        /// Project name or ID
        #[arg(long)]
        project: String,
        /// Error IDs (one or more)
        #[arg(required = true)]
        error_ids: Vec<String>,
        /// Snooze for a duration like 7d, 24h, 30m (default: 7d)
        #[arg(long, conflicts_with = "events")]
        r#for: Option<String>,
        /// Snooze until N more events occur
        #[arg(long)]
        events: Option<u64>,
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

toolbox_core::run_main!(run());

async fn run() -> tb_bug::error::Result<()> {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("--project")
                && let Ok(config) = Config::load()
            {
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

    if cli.version {
        toolbox_core::version_check::print_version("tb-bug", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let Some(ref command) = cli.command else {
        Cli::parse_from(["tb-bug", "--help"]);
        unreachable!()
    };

    // Commands that don't need a loaded config
    if let Commands::Skill { action } = command {
        let skill = toolbox_core::skill::SkillConfig {
            tool_name: "tb-bug",
            content: include_str!("../SKILL.md"),
        };
        toolbox_core::skill::run(&skill, action).map_err(tb_bug::error::TbBugError::Other)?;
        return Ok(());
    }
    if let Commands::Config {
        action:
            ConfigAction::Init {
                token,
                org,
                projects,
            },
    } = command
    {
        commands::config_cmd::init(token.as_deref(), org.as_deref(), projects.as_deref()).await?;
        return Ok(());
    }

    let config = Config::load()?;
    let client = BugsnagClient::new(&config, cli.no_cache)?;

    match command {
        Commands::Errors {
            project,
            status,
            severity,
            time,
            stage,
            class,
            sort,
            limit,
            json,
            long,
        } => {
            commands::errors::run(
                &client,
                &config,
                project,
                status.as_deref(),
                severity.as_deref(),
                time,
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
            toolbox_core::version_check::print_update_hint("tb-bug", env!("CARGO_PKG_VERSION"));
        }
        Commands::Projects { json } => {
            commands::projects::run(&client, &config, *json).await?;
        }
        Commands::Events {
            project,
            error_id,
            limit,
            json,
        } => {
            commands::events::run(&client, &config, project, error_id, *limit, *json).await?;
        }
        Commands::Fetch { target } => match target {
            FetchTarget::Error { project, error_id } => {
                commands::fetch::run_error(&client, &config, project, error_id).await?;
            }
            FetchTarget::Event { project, event_id } => {
                commands::fetch::run_event(&client, &config, project, event_id).await?;
            }
        },
        Commands::Releases {
            project,
            limit,
            json,
        } => {
            commands::releases::run(&client, &config, project, *limit, *json).await?;
        }
        Commands::Trends { project, json } => {
            commands::trends::run(&client, &config, project, *json).await?;
        }
        Commands::Stability { project, json } => {
            commands::stability::run(&client, &config, project, *json).await?;
        }
        Commands::Report { kind } => match kind {
            ReportKind::Dashboard { project } => {
                commands::report::run_dashboard(&client, &config, project).await?;
            }
            ReportKind::Open {
                project,
                limit,
                json,
            } => {
                commands::report::run_open(&client, &config, project, *limit, *json).await?;
            }
        },
        Commands::Search {
            project,
            query,
            limit,
            json,
        } => {
            commands::search::run(&client, &config, project, query, *limit, *json).await?;
        }
        Commands::Error { action } => {
            use commands::error_action::{Op, SnoozeRule, parse_duration};
            match action {
                ErrorAction::Fix { project, error_ids } => {
                    commands::error_action::run(
                        &client,
                        &config,
                        project,
                        error_ids,
                        Op::Fix,
                        true,
                    )
                    .await?;
                }
                ErrorAction::Ignore { project, error_ids } => {
                    commands::error_action::run(
                        &client,
                        &config,
                        project,
                        error_ids,
                        Op::Ignore,
                        true,
                    )
                    .await?;
                }
                ErrorAction::Discard {
                    project,
                    error_ids,
                    yes,
                } => {
                    commands::error_action::run(
                        &client,
                        &config,
                        project,
                        error_ids,
                        Op::Discard,
                        *yes,
                    )
                    .await?;
                }
                ErrorAction::Snooze {
                    project,
                    error_ids,
                    r#for,
                    events,
                } => {
                    let rule = if let Some(n) = events {
                        SnoozeRule::Events(*n)
                    } else {
                        let dur = r#for.as_deref().unwrap_or("7d");
                        let secs = parse_duration(dur).map_err(tb_bug::error::TbBugError::Other)?;
                        SnoozeRule::Seconds(secs)
                    };
                    commands::error_action::run_snooze(&client, &config, project, error_ids, rule)
                        .await?;
                }
            }
        }
        Commands::Doctor => {
            commands::doctor::run(&client, &config).await?;
            toolbox_core::version_check::print_update_hint("tb-bug", env!("CARGO_PKG_VERSION"));
        }
        Commands::CacheClear => {
            client.clear_cache()?;
            println!("Cache cleared.");
        }
        Commands::Skill { .. } => unreachable!(),
        Commands::Config { action } => match action {
            ConfigAction::Init { .. } => unreachable!(),
            ConfigAction::Show => {
                commands::config_cmd::show(&config);
            }
            ConfigAction::Set {
                key,
                value,
                add,
                remove,
            } => {
                commands::config_cmd::set(
                    key,
                    value.as_deref(),
                    add.as_deref(),
                    remove.as_deref(),
                    &config,
                    &client,
                )
                .await?;
            }
        },
    }

    Ok(())
}
