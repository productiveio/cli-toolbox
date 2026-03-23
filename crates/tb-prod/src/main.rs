use clap::Parser;

use tb_prod::api::ProductiveClient;
use tb_prod::commands;
use tb_prod::config::Config;
use tb_prod::input;
use tb_prod::schema;

#[derive(Parser)]
#[command(
    name = "tb-prod",
    disable_version_flag = true,
    about = "Productive.io CLI — generic resource operations for all ~84 resource types"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Print version info
    #[arg(short = 'V', long = "version")]
    version: bool,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Describe a resource type — schema, fields, filters, actions
    Describe {
        /// Resource type (e.g. tasks, projects, people)
        resource_type: String,
        /// Include additional sections: schema, actions, related (comma-separated)
        #[arg(long)]
        include: Option<String>,
    },
    /// Query resources with filtering, sorting, and pagination
    #[command(alias = "list")]
    Query {
        /// Resource type
        resource_type: String,
        /// JSON filter (FilterGroup or flat object)
        #[arg(long)]
        filter: Option<String>,
        /// Sort field (prefix with - for descending)
        #[arg(long)]
        sort: Option<String>,
        /// Page number (default: 1)
        #[arg(long, default_value = "1")]
        page: usize,
        /// Include relationships (comma-separated)
        #[arg(long)]
        include: Option<String>,
    },
    /// Get a single resource by ID
    Get {
        /// Resource type
        resource_type: String,
        /// Resource ID
        id: String,
        /// Include relationships (comma-separated)
        #[arg(long)]
        include: Option<String>,
    },
    /// Create a resource from JSON data
    Create {
        /// Resource type
        resource_type: String,
        /// JSON data (object for single, array for bulk)
        #[arg(long)]
        data: Option<String>,
    },
    /// Update a resource by ID
    Update {
        /// Resource type
        resource_type: String,
        /// Resource ID
        id: String,
        /// JSON data (partial fields to update)
        #[arg(long)]
        data: Option<String>,
    },
    /// Delete a resource by ID
    Delete {
        /// Resource type
        resource_type: String,
        /// Resource ID
        id: String,
        /// Actually delete (default: dry run)
        #[arg(long)]
        confirm: bool,
    },
    /// Search resources by keyword
    Search {
        /// Resource type
        resource_type: String,
        /// Search query text
        #[arg(long)]
        query: String,
    },
    /// Execute a custom action on a resource
    Action {
        /// Resource type
        resource_type: String,
        /// Resource ID
        id: String,
        /// Action name (e.g. archive, restore, move)
        action_name: String,
        /// Optional JSON parameters for the action
        #[arg(long)]
        data: Option<String>,
    },
    /// AI context dump — quick command reference
    Prime {
        #[command(subcommand)]
        target: Option<PrimeTarget>,
    },
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
enum PrimeTarget {
    /// Deep project context — statuses, task lists, services
    Project {
        /// Project name or ID
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
        /// API token (prompted interactively if omitted)
        #[arg(long)]
        token: Option<String>,
        /// Organization ID (auto-detected if omitted)
        #[arg(long)]
        org: Option<String>,
    },
    /// Show current config
    Show,
    /// Set a config value
    Set {
        /// Config key (token, org_id, person_id, api_base_url)
        key: String,
        /// New value
        value: String,
    },
}

fn resolve_resource_or_exit(resource_type: &str) -> &'static schema::ResourceDef {
    let s = schema::schema();
    match s.resolve_resource(resource_type) {
        Some(r) => r,
        None => {
            commands::resource::describe::print_all_types();
            tb_prod::json_error::exit_with_error(
                "unknown_resource_type",
                &format!("Unknown resource type: '{}'", resource_type),
            );
        }
    }
}

toolbox_core::run_main!(run());

async fn run() -> tb_prod::error::Result<()> {
    let cli = Cli::parse();

    if cli.version {
        toolbox_core::version_check::print_version("tb-prod", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let Some(command) = cli.command else {
        Cli::parse_from(["tb-prod", "--help"]);
        unreachable!()
    };

    // Commands that don't need a loaded config
    if let Commands::Skill { action } = &command {
        let skill = toolbox_core::skill::SkillConfig {
            tool_name: "tb-prod",
            content: include_str!("../SKILL.md"),
        };
        toolbox_core::skill::run(&skill, action).map_err(tb_prod::error::TbProdError::Other)?;
        return Ok(());
    }
    if let Commands::Config {
        action: ConfigAction::Init { token, org },
    } = &command
    {
        commands::config_cmd::init(token.as_deref(), org.as_deref()).await?;
        return Ok(());
    }

    let config = Config::load()?;
    let client = ProductiveClient::new(&config);

    match command {
        Commands::Describe {
            resource_type,
            include,
        } => {
            let s = schema::schema();
            match s.resolve_resource(&resource_type) {
                Some(resource) => {
                    commands::resource::describe::run(resource, include.as_deref());
                }
                None => {
                    commands::resource::describe::print_all_types();
                }
            }
        }
        Commands::Query {
            resource_type,
            filter,
            sort,
            page,
            include,
        } => {
            let resource = resolve_resource_or_exit(&resource_type);
            let filter_value = match &filter {
                Some(f) => Some(f.clone()),
                None => input::read_json_input(None).map(|v| v.to_string()),
            };
            commands::resource::query::run(
                &client,
                resource,
                filter_value.as_deref(),
                sort.as_deref(),
                Some(page),
                include.as_deref(),
            )
            .await;
        }
        Commands::Get {
            resource_type,
            id,
            include,
        } => {
            let resource = resolve_resource_or_exit(&resource_type);
            commands::resource::get::run(&client, resource, &id, include.as_deref()).await;
        }
        Commands::Create {
            resource_type,
            data,
        } => {
            let resource = resolve_resource_or_exit(&resource_type);
            let json_data = input::require_json_input(data.as_deref(), "create");
            commands::resource::create::run(&client, resource, &json_data).await;
        }
        Commands::Update {
            resource_type,
            id,
            data,
        } => {
            let resource = resolve_resource_or_exit(&resource_type);
            let json_data = input::require_json_input(data.as_deref(), "update");
            commands::resource::update::run(&client, resource, &id, &json_data).await;
        }
        Commands::Delete {
            resource_type,
            id,
            confirm,
        } => {
            let resource = resolve_resource_or_exit(&resource_type);
            commands::resource::delete::run(&client, resource, &id, confirm).await;
        }
        Commands::Search {
            resource_type,
            query,
        } => {
            let resource = resolve_resource_or_exit(&resource_type);
            commands::resource::search::run(&client, resource, &query).await;
        }
        Commands::Action {
            resource_type,
            id,
            action_name,
            data,
        } => {
            let resource = resolve_resource_or_exit(&resource_type);
            let json_data = input::read_json_input(data.as_deref());
            commands::resource::action::run(
                &client,
                resource,
                &id,
                &action_name,
                json_data.as_ref(),
            )
            .await;
        }
        Commands::Prime { target } => {
            match target {
                None => {
                    commands::prime::run(&client, &config).await?;
                }
                Some(PrimeTarget::Project { project }) => {
                    commands::prime::run_project(&client, &project).await?;
                }
            }
            toolbox_core::version_check::print_update_hint("tb-prod", env!("CARGO_PKG_VERSION"));
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
            toolbox_core::version_check::print_update_hint("tb-prod", env!("CARGO_PKG_VERSION"));
        }
        Commands::Config { action } => match action {
            ConfigAction::Init { .. } => unreachable!(),
            ConfigAction::Show => {
                commands::config_cmd::show(&config);
            }
            ConfigAction::Set { key, value } => {
                commands::config_cmd::set(&key, &value)?;
            }
        },
        Commands::Skill { .. } => unreachable!(),
    }

    Ok(())
}
