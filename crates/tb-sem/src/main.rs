use chrono::NaiveDate;
use clap::Parser;

use tb_sem::api::SemaphoreClient;
use tb_sem::commands;
use tb_sem::config::Config;

fn parse_date_to_timestamp(s: &str) -> Option<i64> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|dt| dt.and_utc().timestamp())
}

#[derive(Parser)]
#[command(
    name = "tb-sem",
    disable_version_flag = true,
    about = "Semaphore CI CLI for triage and investigation"
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
    /// List recent workflow runs
    Runs {
        /// Project name or ID
        project: String,
        /// Filter by branch
        #[arg(long)]
        branch: Option<String>,
        /// Show only failed runs
        #[arg(long)]
        failed: bool,
        /// Number of runs to show
        #[arg(long, default_value = "5")]
        limit: usize,
        /// Only show runs after this date (YYYY-MM-DD)
        #[arg(long)]
        after: Option<String>,
        /// Only show runs before this date (YYYY-MM-DD)
        #[arg(long)]
        before: Option<String>,
        /// JSON output
        #[arg(long)]
        json: bool,
        /// UTC timestamps
        #[arg(long)]
        utc: bool,
    },
    /// Show pipeline details
    Pipeline {
        /// Pipeline ID
        pipeline_id: String,
        /// Include job-level details
        #[arg(long)]
        jobs: bool,
        /// JSON output
        #[arg(long)]
        json: bool,
        /// UTC timestamps
        #[arg(long)]
        utc: bool,
    },
    /// Show parsed failure summary for a pipeline
    Failures {
        /// Pipeline ID
        pipeline_id: String,
        /// JSON output
        #[arg(long)]
        json: bool,
    },
    /// Fetch and display job logs
    Logs {
        /// Job ID
        job_id: String,
        /// Filter output by regex
        #[arg(long)]
        grep: Option<String>,
        /// Show only last N lines
        #[arg(long)]
        tail: Option<usize>,
        /// Show only first N lines
        #[arg(long)]
        head: Option<usize>,
        /// Show only cucumber summary
        #[arg(long)]
        summary: bool,
        /// Show only errors
        #[arg(long)]
        errors: bool,
        /// Include ANSI codes (default: strip)
        #[arg(long)]
        raw: bool,
        /// JSON output
        #[arg(long)]
        json: bool,
    },
    /// Show recent deploys and check for overlap
    Deploys {
        /// Project name
        project: String,
        /// Show deploys around a pipeline's run window
        #[arg(long)]
        around: Option<String>,
        /// JSON output
        #[arg(long)]
        json: bool,
        /// UTC timestamps
        #[arg(long)]
        utc: bool,
    },
    /// Full triage of a failed pipeline
    Triage {
        /// Pipeline ID (defaults to latest failed e2e run)
        pipeline_id: Option<String>,
        /// JSON output
        #[arg(long)]
        json: bool,
        /// UTC timestamps
        #[arg(long)]
        utc: bool,
    },
    /// Structured test results from pipeline logs
    Tests {
        /// Pipeline ID
        pipeline_id: String,
        /// Show only failed tests
        #[arg(long)]
        failed: bool,
        /// Show only retried tests
        #[arg(long)]
        retried: bool,
        /// One-line summary only
        #[arg(long)]
        summary: bool,
        /// JSON output
        #[arg(long)]
        json: bool,
    },
    /// List promotion pipelines for a pipeline
    Promotions {
        /// Pipeline ID
        pipeline_id: String,
        /// Filter by promotion name
        #[arg(long)]
        name: Option<String>,
        /// JSON output
        #[arg(long)]
        json: bool,
        /// UTC timestamps
        #[arg(long)]
        utc: bool,
    },
    /// Compare two pipeline runs
    Compare {
        /// First pipeline ID
        pipeline_id_1: String,
        /// Second pipeline ID
        pipeline_id_2: String,
        /// JSON output
        #[arg(long)]
        json: bool,
        /// UTC timestamps
        #[arg(long)]
        utc: bool,
    },
    /// Track a test's pass/fail history across recent runs
    History {
        /// Test name (partial match)
        test_name: String,
        /// Project name (default: e2e-tests)
        #[arg(long, default_value = "e2e-tests")]
        project: String,
        /// Number of runs to check
        #[arg(long, default_value = "10")]
        limit: usize,
        /// JSON output
        #[arg(long)]
        json: bool,
        /// UTC timestamps
        #[arg(long)]
        utc: bool,
    },
    /// Show flaky tests across recent runs
    Flaky {
        /// Project name (default: e2e-tests)
        #[arg(default_value = "e2e-tests")]
        project: String,
        /// Number of runs to check
        #[arg(long, default_value = "10")]
        limit: usize,
        /// JSON output
        #[arg(long)]
        json: bool,
        /// UTC timestamps
        #[arg(long)]
        utc: bool,
    },
    /// AI-optimized context dump
    Prime {
        /// Minimal output for hooks
        #[arg(long)]
        mcp: bool,
        /// UTC timestamps
        #[arg(long)]
        utc: bool,
    },
    /// Health check for CLI setup
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
enum ConfigAction {
    /// Initialize configuration
    Init {
        /// API token
        #[arg(long)]
        token: String,
        /// Organization ID (subdomain)
        #[arg(long)]
        org: String,
    },
    /// Show current configuration
    Show,
    /// Set a config value
    Set {
        /// Config key (token, org_id, timezone)
        key: String,
        /// New value
        value: String,
    },
}

toolbox_core::run_main!(run());

async fn run() -> tb_sem::error::Result<()> {
    let cli = Cli::parse();

    if cli.version {
        toolbox_core::version_check::print_version("tb-sem", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let Some(command) = cli.command else {
        Cli::parse_from(["tb-sem", "--help"]);
        unreachable!()
    };

    // Commands that don't need a loaded config
    if let Commands::Skill { action } = &command {
        let skill = toolbox_core::skill::SkillConfig {
            tool_name: "tb-sem",
            content: include_str!("../SKILL.md"),
        };
        toolbox_core::skill::run(&skill, action).map_err(tb_sem::error::TbSemError::Other)?;
        return Ok(());
    }
    if let Commands::Config { action } = &command {
        match action {
            ConfigAction::Init { token, org } => {
                commands::config_cmd::init_with_org(token, org).await?;
            }
            ConfigAction::Show => {
                commands::config_cmd::show()?;
            }
            ConfigAction::Set { key, value } => {
                commands::config_cmd::set(&key, &value)?;
            }
        }
        return Ok(());
    }

    let config = Config::load()?;
    let client = SemaphoreClient::new(&config);

    match command {
        Commands::Runs {
            project,
            branch,
            failed,
            limit,
            after,
            before,
            json,
            utc,
        } => {
            let after_ts = after.as_deref().and_then(parse_date_to_timestamp);
            let before_ts = before.as_deref().and_then(parse_date_to_timestamp);
            commands::runs::run(
                &client,
                &config,
                &project,
                branch.as_deref(),
                failed,
                limit,
                json,
                utc,
                after_ts,
                before_ts,
            )
            .await?;
        }
        Commands::Pipeline {
            pipeline_id,
            jobs,
            json,
            utc,
        } => {
            commands::pipeline::run(&client, &config, &pipeline_id, jobs, json, utc).await?;
        }
        Commands::Failures { pipeline_id, json } => {
            commands::failures::run(&client, &pipeline_id, json).await?;
        }
        Commands::Logs {
            job_id,
            grep,
            tail,
            head,
            summary,
            errors,
            raw,
            json,
        } => {
            commands::logs::run(
                &client,
                &job_id,
                grep.as_deref(),
                tail,
                head,
                summary,
                errors,
                raw,
                json,
            )
            .await?;
        }
        Commands::Deploys {
            project,
            around,
            json,
            utc,
        } => {
            commands::deploys::run(&client, &config, &project, around.as_deref(), json, utc)
                .await?;
        }
        Commands::Triage {
            pipeline_id,
            json,
            utc,
        } => {
            commands::triage::run(&client, &config, pipeline_id.as_deref(), json, utc).await?;
        }
        Commands::Tests {
            pipeline_id,
            failed,
            retried,
            summary,
            json,
        } => {
            commands::tests::run(&client, &pipeline_id, failed, retried, summary, json).await?;
        }
        Commands::Promotions {
            pipeline_id,
            name,
            json,
            utc,
        } => {
            commands::promotions::run(&client, &config, &pipeline_id, name.as_deref(), json, utc)
                .await?;
        }
        Commands::Compare {
            pipeline_id_1,
            pipeline_id_2,
            json,
            utc,
        } => {
            commands::compare::run(&client, &config, &pipeline_id_1, &pipeline_id_2, json, utc)
                .await?;
        }
        Commands::History {
            test_name,
            project,
            limit,
            json,
            utc,
        } => {
            commands::history::run(&client, &config, &test_name, &project, limit, json, utc)
                .await?;
        }
        Commands::Flaky {
            project,
            limit,
            json,
            utc,
        } => {
            commands::flaky::run(&client, &config, &project, limit, json, utc).await?;
        }
        Commands::Prime { mcp, utc } => {
            commands::prime::run(&client, &config, mcp, utc).await?;
            toolbox_core::version_check::print_update_hint("tb-sem", env!("CARGO_PKG_VERSION"));
        }
        Commands::Doctor => {
            commands::doctor::run(&config).await?;
            toolbox_core::version_check::print_update_hint("tb-sem", env!("CARGO_PKG_VERSION"));
        }
        Commands::Config { .. } | Commands::Skill { .. } => unreachable!(),
    }

    Ok(())
}
