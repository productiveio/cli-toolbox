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
        Commands::Search { query } => {
            println!("TODO: search for '{}'", query);
        }
        Commands::Config { action } => match action {
            ConfigAction::Init => tb_session::commands::config_cmd::init()?,
            ConfigAction::Show => tb_session::commands::config_cmd::show()?,
        },
    }

    Ok(())
}
