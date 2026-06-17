use std::env;

use clap::Parser;
use colored::Colorize;

use tb_devctl::commands;
use tb_devctl::config;

#[derive(Parser)]
#[command(
    name = "tb-devctl",
    version,
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
        /// Comma-separated list of services, or omit when using --preset
        services: Option<String>,

        /// Use a named preset from tb-devctl.toml
        #[arg(long)]
        preset: Option<String>,

        /// Run in Docker container
        #[arg(long, conflicts_with_all = ["local"])]
        docker: bool,

        /// Run locally from repos/ (or --dir)
        #[arg(long, conflicts_with_all = ["docker"])]
        local: bool,

        /// Service directory override (local mode only)
        #[arg(long, requires = "local")]
        dir: Option<String>,

        /// Run in background (local mode only)
        #[arg(long, requires = "local")]
        bg: bool,
    },

    /// Stop services
    Stop {
        /// Service name (local mode). Omit to stop Docker container.
        service: Option<String>,
    },

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

    /// First-time setup for a service (secrets, schema, seeding)
    Init {
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

    /// Manage Claude Code skill file
    Skill {
        #[command(subcommand)]
        action: toolbox_core::skill::SkillAction,
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

    // `skill` doesn't need a loaded config — handle it before locating tb-devctl.toml
    // so it works from anywhere (e.g. `scripts/install.sh --with-skill`).
    if let Commands::Skill { action } = &cli.command {
        let skill = toolbox_core::skill::SkillConfig {
            tool_name: "tb-devctl",
            content: include_str!("../SKILL.md"),
        };
        if let Err(e) = toolbox_core::skill::run(&skill, action) {
            eprintln!("{} {}", "Error:".red().bold(), e);
            std::process::exit(1);
        }
        return;
    }

    let cwd = env::current_dir().unwrap_or_else(|e| {
        eprintln!(
            "{} Cannot determine current directory: {}",
            "Error:".red().bold(),
            e
        );
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
            preset,
            docker,
            local,
            dir,
            bg,
        } => {
            if let Some(preset_name) = preset {
                commands::preset::run(&cfg, &root, &preset_name)
            } else if let Some(services) = services {
                if docker {
                    let svc_list: Vec<String> =
                        services.split(',').map(|s| s.trim().to_string()).collect();
                    commands::start::docker(&cfg, &root, &svc_list)
                } else if local {
                    commands::local::start(&cfg, &root, &services, dir.as_deref(), bg)
                } else {
                    Err(tb_devctl::error::Error::Other(
                        "Specify --docker or --local mode.".into(),
                    ))
                }
            } else {
                Err(tb_devctl::error::Error::Other(
                    "Specify services or --preset.".into(),
                ))
            }
        }
        Commands::Stop { service } => {
            if let Some(svc) = service {
                commands::local::stop(&root, &svc)
            } else {
                commands::stop::run(&cfg, &root)
            }
        }
        Commands::Restart { service } => commands::stop::restart_service(&cfg, &service),
        Commands::Logs { service } => commands::logs::run(&cfg, &root, &service),
        Commands::Init { service } => commands::init::run(&cfg, &root, &service),
        Commands::Infra { action } => match action {
            InfraAction::Up => commands::infra::up(&cfg, &root),
            InfraAction::Down => commands::infra::down(&cfg, &root),
            InfraAction::Status => commands::infra::status(&cfg, &root),
        },
        Commands::Doctor => commands::doctor::run(&cfg, &root),
        Commands::Skill { .. } => unreachable!("handled before config load"),
    };

    if let Err(e) = result {
        eprintln!("{} {}", "Error:".red().bold(), e);
        std::process::exit(1);
    }
}
