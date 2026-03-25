use std::env;

use clap::Parser;
use colored::Colorize;

use devctl::commands;
use devctl::config;

#[derive(Parser)]
#[command(
    name = "devctl",
    about = "Local dev environment orchestrator for Productive services"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Show status of all services and infrastructure
    Status,

    /// Manage shared infrastructure (MySQL, Redis, etc.)
    Infra {
        #[command(subcommand)]
        action: InfraAction,
    },
}

#[derive(clap::Subcommand)]
enum InfraAction {
    /// Start shared infrastructure
    Up,
    /// Stop shared infrastructure
    Down,
    /// Check infrastructure status
    Status,
}

fn main() {
    let cli = Cli::parse();

    let cwd = env::current_dir().unwrap_or_else(|e| {
        eprintln!("{} Cannot determine current directory: {}", "Error:".red().bold(), e);
        std::process::exit(1);
    });

    let (cfg, root) = match config::find_and_load(&cwd) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("{} {}", "Error:".red().bold(), e);
            std::process::exit(1);
        }
    };

    let result = match cli.command {
        Commands::Status => commands::status::run(&cfg, &root),
        Commands::Infra { action } => match action {
            InfraAction::Up => commands::infra::up(&cfg, &root),
            InfraAction::Down => commands::infra::down(&cfg, &root),
            InfraAction::Status => commands::infra::status(&cfg, &root),
        },
    };

    if let Err(e) = result {
        eprintln!("{} {}", "Error:".red().bold(), e);
        std::process::exit(1);
    }
}
