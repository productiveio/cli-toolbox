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

    /// Start services
    Start {
        /// Comma-separated list of services
        services: String,

        /// Run in Docker container
        #[arg(long)]
        docker: bool,
    },

    /// Stop the Docker dev container
    Stop,

    /// Restart a service inside the running container
    Restart {
        /// Service name
        service: String,
    },

    /// View logs for a service
    Logs {
        /// Service name
        service: String,
    },

    /// Manage shared infrastructure (MySQL, Redis, etc.)
    Infra {
        #[command(subcommand)]
        action: InfraAction,
    },

    /// Diagnose environment health
    Doctor,
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
        Commands::Start {
            services,
            docker,
        } => {
            let svc_list: Vec<String> = services.split(',').map(|s| s.trim().to_string()).collect();
            if docker {
                commands::start::docker(&cfg, &root, &svc_list)
            } else {
                Err(devctl::error::Error::Other(
                    "Local mode (--local) not yet implemented. Use --docker.".into(),
                ))
            }
        }
        Commands::Stop => commands::stop::run(&cfg, &root),
        Commands::Restart { service } => commands::stop::restart_service(&cfg, &service),
        Commands::Logs { service } => commands::logs::run(&cfg, &root, &service),
        Commands::Infra { action } => match action {
            InfraAction::Up => commands::infra::up(&cfg, &root),
            InfraAction::Down => commands::infra::down(&cfg, &root),
            InfraAction::Status => commands::infra::status(&cfg, &root),
        },
        Commands::Doctor => commands::doctor::run(&cfg, &root),
    };

    if let Err(e) = result {
        eprintln!("{} {}", "Error:".red().bold(), e);
        std::process::exit(1);
    }
}
