use clap::Parser;

#[derive(Parser)]
#[command(
    name = "tb-pr",
    disable_version_flag = true,
    about = "GitHub PR radar — kanban TUI + CLI for the Productive org"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Output as JSON (where supported)
    #[arg(long, global = true)]
    json: bool,

    /// Print version info
    #[arg(short = 'V', long = "version")]
    version: bool,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Launch the interactive kanban TUI (default)
    Tui,

    /// List PRs as a table or JSON
    List {
        /// Filter to a single column (draft-mine, review-mine, ready-to-merge-mine, waiting-on-me, waiting-on-author)
        #[arg(long)]
        column: Option<String>,

        /// Only PRs older than N days
        #[arg(long)]
        stale_days: Option<u32>,
    },

    /// Show detail view of a single PR
    Show {
        /// PR number or full URL
        pr_ref: String,
    },

    /// Force a full fetch and update the cache
    Refresh,

    /// Open a PR URL in the default browser
    Open {
        /// PR number or full URL
        pr_ref: String,
    },

    /// AI-optimized context dump
    Prime,

    /// Verify setup and diagnose issues
    Doctor,

    /// Manage the Claude Code skill file
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

toolbox_core::run_main!(run());

async fn run() -> tb_pr::error::Result<()> {
    let cli = Cli::parse();

    if cli.version {
        toolbox_core::version_check::print_version("tb-pr", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let command = cli.command.unwrap_or(Commands::Tui);

    match command {
        Commands::Tui => tb_pr::commands::tui::run().await,
        Commands::List { column, stale_days } => {
            tb_pr::commands::list::run(column, stale_days, cli.json).await
        }
        Commands::Show { pr_ref } => tb_pr::commands::show::run(&pr_ref, cli.json).await,
        Commands::Refresh => tb_pr::commands::refresh::run().await,
        Commands::Open { pr_ref } => tb_pr::commands::open::run(&pr_ref),
        Commands::Prime => {
            tb_pr::commands::prime::run().await?;
            toolbox_core::version_check::print_update_hint("tb-pr", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Commands::Doctor => {
            tb_pr::commands::doctor::run()?;
            toolbox_core::version_check::print_update_hint("tb-pr", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Commands::Skill { action } => {
            let skill = toolbox_core::skill::SkillConfig {
                tool_name: "tb-pr",
                content: include_str!("../SKILL.md"),
            };
            toolbox_core::skill::run(&skill, &action).map_err(tb_pr::error::Error::Other)
        }
        Commands::Config { action } => match action {
            ConfigAction::Init => tb_pr::commands::config_cmd::init(),
            ConfigAction::Show => tb_pr::commands::config_cmd::show(),
        },
    }
}
