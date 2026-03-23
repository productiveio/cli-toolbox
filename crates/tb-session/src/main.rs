use clap::Parser;

use tb_session::config::Config;

#[derive(Parser)]
#[command(
    name = "tb-session",
    disable_version_flag = true,
    about = "Claude Code session search CLI"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Bypass index cache (force rebuild)
    #[arg(long, global = true)]
    no_cache: bool,

    /// Output as JSON
    #[arg(long, global = true)]
    json: bool,

    /// Print version info
    #[arg(short = 'V', long = "version")]
    version: bool,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Full-text search across sessions
    Search {
        /// Search query
        query: String,

        /// Filter by git branch name
        #[arg(long)]
        branch: Option<String>,

        /// Filter by project path (substring match)
        #[arg(long)]
        project: Option<String>,

        /// Search across all projects (default: current directory only)
        #[arg(long)]
        all_projects: bool,

        /// Maximum number of results
        #[arg(long)]
        limit: Option<usize>,

        /// Only sessions modified on or after this date (ISO 8601)
        #[arg(long)]
        after: Option<String>,

        /// Only sessions modified on or before this date (ISO 8601)
        #[arg(long)]
        before: Option<String>,
    },

    /// List sessions by metadata (no full-text search)
    List {
        /// Filter by git branch name
        #[arg(long)]
        branch: Option<String>,

        /// List sessions across all projects
        #[arg(long)]
        all_projects: bool,

        /// Maximum number of results per page
        #[arg(long)]
        limit: Option<usize>,

        /// Page number (starts at 1)
        #[arg(long, default_value = "1")]
        page: usize,

        /// Only sessions modified on or after this date
        #[arg(long)]
        after: Option<String>,

        /// Only sessions modified on or before this date
        #[arg(long)]
        before: Option<String>,
    },

    /// Show session detail and conversation preview
    Show {
        /// Session ID (full or prefix)
        session_id: String,
    },

    /// Resume a session (execs into claude --resume)
    Resume {
        /// Session ID
        session_id: String,
    },

    /// Rebuild the search index
    Index {
        /// Index all projects (default: current dir only)
        #[arg(long)]
        all_projects: bool,
    },

    /// Verify setup and diagnose issues
    Doctor,

    /// Delete the index database for a clean rebuild
    CacheClear,

    /// AI-optimized context dump
    Prime,

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
enum ConfigAction {
    /// Write default config to disk
    Init,
    /// Show current config and resolved paths
    Show,
}

fn main() {
    if let Err(e) = run() {
        use colored::Colorize;
        eprintln!("{} {e}", "Error:".red().bold());
        std::process::exit(1);
    }
}

fn resolve_and_freshen(
    conn: &rusqlite::Connection,
    projects_dir: &std::path::Path,
    all_projects: bool,
) -> tb_session::error::Result<Vec<String>> {
    if all_projects {
        tb_session::index::ensure_fresh(conn, projects_dir, None)?;
        return Ok(vec![]);
    }
    let cwd = std::env::current_dir()?;
    let repo_paths: Vec<String> = tb_session::git::repo_paths(&cwd)
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    for path in &repo_paths {
        tb_session::index::ensure_fresh(conn, projects_dir, Some(std::path::Path::new(path)))?;
    }
    Ok(repo_paths)
}

fn run() -> tb_session::error::Result<()> {
    let cli = Cli::parse();

    if cli.version {
        toolbox_core::version_check::print_version("tb-session", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let Some(command) = cli.command else {
        Cli::parse_from(["tb-session", "--help"]);
        unreachable!()
    };

    match command {
        Commands::Search {
            query,
            branch,
            project,
            all_projects,
            limit,
            after,
            before,
        } => {
            let config = Config::load()?;
            let conn = tb_session::index::open_db(cli.no_cache)?;
            let projects_dir = config.projects_dir();
            let repo_paths = resolve_and_freshen(&conn, &projects_dir, all_projects)?;
            let effective_limit = limit.unwrap_or(config.default_limit);

            tb_session::commands::search::run(
                &conn,
                &query,
                branch.as_deref(),
                after.as_deref(),
                before.as_deref(),
                project.as_deref(),
                &repo_paths,
                effective_limit,
                cli.json,
            )?;
        }
        Commands::List {
            branch,
            all_projects,
            limit,
            page,
            after,
            before,
        } => {
            let config = Config::load()?;
            let conn = tb_session::index::open_db(cli.no_cache)?;
            let projects_dir = config.projects_dir();
            let repo_paths = resolve_and_freshen(&conn, &projects_dir, all_projects)?;
            let effective_limit = limit.unwrap_or(config.default_limit);

            tb_session::commands::list::run(
                &conn,
                branch.as_deref(),
                after.as_deref(),
                before.as_deref(),
                &repo_paths,
                effective_limit,
                page,
                cli.json,
            )?;
        }
        Commands::Show { session_id } => {
            let config = Config::load()?;
            let conn = tb_session::index::open_db(cli.no_cache)?;
            let projects_dir = config.projects_dir();
            tb_session::index::ensure_fresh(&conn, &projects_dir, None)?;
            tb_session::commands::show::run(&conn, &session_id, cli.json)?;
        }
        Commands::Resume { session_id } => {
            let config = Config::load()?;
            let conn = tb_session::index::open_db(cli.no_cache)?;
            let projects_dir = config.projects_dir();
            tb_session::index::ensure_fresh(&conn, &projects_dir, None)?;
            tb_session::commands::resume::run(&conn, &session_id)?;
        }
        Commands::Index { all_projects } => {
            tb_session::commands::index_cmd::run(all_projects)?;
        }
        Commands::Doctor => {
            tb_session::commands::doctor::run()?;
            toolbox_core::version_check::print_update_hint("tb-session", env!("CARGO_PKG_VERSION"));
        }
        Commands::CacheClear => {
            tb_session::commands::cache_clear::run()?;
        }
        Commands::Prime => {
            tb_session::commands::prime::run()?;
            toolbox_core::version_check::print_update_hint("tb-session", env!("CARGO_PKG_VERSION"));
        }
        Commands::Skill { action } => {
            let skill = toolbox_core::skill::SkillConfig {
                tool_name: "tb-session",
                content: include_str!("../SKILL.md"),
            };
            toolbox_core::skill::run(&skill, &action)
                .map_err(tb_session::error::Error::Other)?;
        }
        Commands::Config { action } => match action {
            ConfigAction::Init => tb_session::commands::config_cmd::init()?,
            ConfigAction::Show => tb_session::commands::config_cmd::show()?,
        },
    }

    Ok(())
}
