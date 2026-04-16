use clap::Parser;
use colored::Colorize;
use tb_lf::api::{DevPortalClient, PaginatedResponse};
use tb_lf::cache::CacheTtl;
use tb_lf::cli::Pagination;
use tb_lf::config::{self, Config};
use tb_lf::output;
use tb_lf::types::*;
use toolbox_core::time_range::TimeRange;

/// Convert (Option<i64>, --no-* flag) into Nullable<i64>.
fn to_nullable(value: Option<i64>, clear: bool) -> Nullable<i64> {
    if clear {
        Nullable::Null
    } else {
        match value {
            Some(v) => Nullable::Value(v),
            None => Nullable::Absent,
        }
    }
}

#[derive(Parser)]
#[command(
    name = "tb-lf",
    disable_version_flag = true,
    about = "Langfuse/DevPortal insights CLI"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Output as JSON
    #[arg(long, global = true)]
    json: bool,

    /// DevPortal project name or ID
    #[arg(long, global = true)]
    project: Option<String>,

    /// Bypass cache
    #[arg(long, global = true)]
    no_cache: bool,

    /// Print version info
    #[arg(short = 'V', long = "version")]
    version: bool,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// List traces
    #[command(
        after_help = "Examples:\n  tb-lf traces --from 1d\n  tb-lf traces --triage flagged --limit 50\n  tb-lf traces --name my-agent --env production\n  tb-lf traces --stats --from 7d"
    )]
    Traces {
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        user: Option<String>,
        #[arg(long)]
        session: Option<String>,
        #[arg(long)]
        env: Option<String>,
        #[arg(long)]
        triage: Option<String>,
        #[arg(long)]
        satisfaction: Option<String>,
        #[arg(long)]
        sort: Option<String>,
        /// Show trace stats instead of list
        #[arg(long)]
        stats: bool,
        #[command(flatten)]
        time: TimeRange,
        #[command(flatten)]
        pagination: Pagination,
    },
    /// Fetch a single trace (Langfuse proxy)
    #[command(
        after_help = "Examples:\n  tb-lf trace abc123 --project production\n  tb-lf trace abc123 --project production --full\n  tb-lf trace abc123 --project production --observations"
    )]
    Trace {
        id: String,
        /// Show full JSON
        #[arg(long)]
        full: bool,
        /// Include observations
        #[arg(long)]
        observations: bool,
    },
    /// List sessions
    #[command(
        after_help = "Examples:\n  tb-lf sessions --from 7d\n  tb-lf sessions --user user@example.com\n  tb-lf sessions --stats"
    )]
    Sessions {
        #[arg(long)]
        user: Option<String>,
        #[arg(long)]
        env: Option<String>,
        #[arg(long)]
        satisfaction: Option<String>,
        #[arg(long)]
        sort: Option<String>,
        #[arg(long)]
        stats: bool,
        #[command(flatten)]
        time: TimeRange,
        #[command(flatten)]
        pagination: Pagination,
    },
    /// Show all traces in a session
    #[command(
        after_help = "Examples:\n  tb-lf session my-session-id\n  tb-lf session my-session-id --json"
    )]
    Session { id: String },
    /// List observations
    #[command(
        after_help = "Examples:\n  tb-lf observations --trace abc123\n  tb-lf observations --type GENERATION --model gpt-4\n  tb-lf observations --level ERROR\n  tb-lf observations --from 7d"
    )]
    Observations {
        #[arg(long)]
        trace: Option<String>,
        #[arg(long)]
        r#type: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        level: Option<String>,
        #[arg(long)]
        env: Option<String>,
        #[command(flatten)]
        time: TimeRange,
    },
    /// Fetch a single observation (Langfuse proxy)
    #[command(
        after_help = "Examples:\n  tb-lf observation abc123 --project production\n  tb-lf observation abc123 --project production --json"
    )]
    Observation { id: String },
    /// List scores
    #[command(
        after_help = "Examples:\n  tb-lf scores --trace abc123\n  tb-lf scores --name correctness --source EVAL\n  tb-lf scores --json | jq '.[] | select(.value < 0.5)'"
    )]
    Scores {
        #[arg(long)]
        trace: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        env: Option<String>,
        /// Max results per page
        #[arg(long, default_value = "20")]
        limit: u32,
        #[command(flatten)]
        time: TimeRange,
    },
    /// List comments
    #[command(
        after_help = "Examples:\n  tb-lf comments --trace abc123\n  tb-lf comments --type trace\n  tb-lf comments --from 7d\n  tb-lf comments --json"
    )]
    Comments {
        #[arg(long)]
        trace: Option<String>,
        #[arg(long)]
        r#type: Option<String>,
        #[arg(long)]
        object: Option<String>,
        #[command(flatten)]
        time: TimeRange,
    },
    /// Show dashboard overview
    #[command(
        after_help = "Examples:\n  tb-lf dashboard\n  tb-lf dashboard --from 2025-01-01 --to 2025-01-31\n  tb-lf dashboard --json"
    )]
    Dashboard {
        #[command(flatten)]
        time: TimeRange,
    },
    /// Show daily metrics
    #[command(
        alias = "metrics",
        after_help = "Examples:\n  tb-lf daily-metrics --days 14\n  tb-lf daily-metrics --env production --from 30d\n  tb-lf daily-metrics --json | jq '.[] | .date'"
    )]
    DailyMetrics {
        /// Number of days back
        #[arg(long)]
        days: Option<u32>,
        #[arg(long)]
        env: Option<String>,
        #[command(flatten)]
        time: TimeRange,
    },
    /// Trace-level scoring metrics (aggregated or per-trace)
    #[command(
        after_help = "Examples:\n  tb-lf trace-metrics\n  tb-lf trace-metrics --group-by day --from 7d\n  tb-lf trace-metrics --group-by outcome\n  tb-lf trace-metrics --group-by agent_type --env production\n  tb-lf trace-metrics --group-by flag:my_flag --from 14d\n  tb-lf trace-metrics abc123def456"
    )]
    TraceMetrics {
        /// Trace Langfuse ID — show per-trace detail instead of aggregates
        #[arg()]
        trace_id: Option<String>,
        /// Grouping: day, outcome, agent_type, flag:<name>
        #[arg(long, default_value = "day")]
        group_by: String,
        #[arg(long)]
        env: Option<String>,
        /// Filter by outcome: successful, error, context_overflow, etc.
        #[arg(long)]
        outcome: Option<String>,
        #[command(flatten)]
        time: TimeRange,
    },
    /// List triage queue items
    #[command(
        after_help = "Examples:\n  tb-lf queue --status pending_review\n  tb-lf queue --category bug --confidence high\n  tb-lf queue --from 7d\n  tb-lf queue --from 2026-03-01 --to 2026-03-10\n  tb-lf queue --full --limit 5"
    )]
    Queue {
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        category: Option<String>,
        #[arg(long)]
        confidence: Option<String>,
        #[arg(long)]
        run: Option<String>,
        #[arg(long)]
        feature: Option<String>,
        /// Filter by team ID, or "unassigned" for items with no team
        #[arg(long)]
        team: Option<String>,
        /// Show full AI reasoning
        #[arg(long)]
        full: bool,
        #[command(flatten)]
        time: TimeRange,
        #[command(flatten)]
        pagination: Pagination,
    },
    /// Triage queue statistics
    #[command(after_help = "Examples:\n  tb-lf queue-stats\n  tb-lf queue-stats --json")]
    QueueStats,
    /// Show a single queue item
    #[command(after_help = "Examples:\n  tb-lf queue-item 42\n  tb-lf queue-item 42 --json")]
    QueueItem { id: i64 },
    /// Update a queue item
    #[command(
        after_help = "Examples:\n  tb-lf queue-update 42 --status confirmed\n  tb-lf queue-update 42 --team 3 --note 'needs follow-up'\n  tb-lf queue-update 42 --no-team                        Clear team\n  tb-lf queue-update 42 --category bug --reviewed-by tibor"
    )]
    QueueUpdate {
        id: i64,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        category: Option<String>,
        #[arg(long)]
        note: Option<String>,
        #[arg(long)]
        feature: Option<i64>,
        /// Clear the feature assignment
        #[arg(long, conflicts_with = "feature")]
        no_feature: bool,
        #[arg(long)]
        team: Option<i64>,
        /// Clear the team assignment
        #[arg(long, conflicts_with = "team")]
        no_team: bool,
        #[arg(long)]
        reviewed_by: Option<String>,
    },
    /// Bulk-update queue items matching filters
    #[command(
        after_help = "Examples:\n  tb-lf queue-bulk-update --ids 1,2,3 --set-team 5\n  tb-lf queue-bulk-update --filter-status pending_review --set-team 3\n  tb-lf queue-bulk-update --filter-category bug --set-status confirmed --set-reviewed-by tibor\n  tb-lf queue-bulk-update --filter-confidence high --filter-category feature_request --set-team 5 --dry-run"
    )]
    QueueBulkUpdate {
        /// Update specific items by ID (comma-separated), skipping filter query
        #[arg(long, value_delimiter = ',', conflicts_with_all = ["filter_status", "filter_category", "filter_confidence", "filter_feature", "filter_team"])]
        ids: Vec<i64>,
        #[arg(long)]
        filter_status: Option<String>,
        #[arg(long)]
        filter_category: Option<String>,
        #[arg(long)]
        filter_confidence: Option<String>,
        #[arg(long)]
        filter_feature: Option<String>,
        #[arg(long)]
        filter_team: Option<String>,
        #[arg(long)]
        set_status: Option<String>,
        #[arg(long)]
        set_category: Option<String>,
        #[arg(long)]
        set_note: Option<String>,
        #[arg(long)]
        set_feature: Option<i64>,
        /// Clear the feature assignment
        #[arg(long, conflicts_with = "set_feature")]
        no_feature: bool,
        #[arg(long)]
        set_team: Option<i64>,
        /// Clear the team assignment
        #[arg(long, conflicts_with = "set_team")]
        no_team: bool,
        #[arg(long)]
        set_reviewed_by: Option<String>,
        /// Preview changes without applying them
        #[arg(long)]
        dry_run: bool,
        #[command(flatten)]
        time: TimeRange,
    },
    /// Delete a queue item
    #[command(after_help = "Examples:\n  tb-lf queue-delete 42")]
    QueueDelete { id: i64 },
    /// List teams
    #[command(after_help = "Examples:\n  tb-lf teams\n  tb-lf teams --json")]
    Teams {
        #[arg(long)]
        status: Option<String>,
    },
    /// List triage runs
    #[command(
        after_help = "Examples:\n  tb-lf triage-runs\n  tb-lf triage-runs --status completed --limit 5\n  tb-lf triage-runs --from 7d\n  tb-lf triage-runs --json"
    )]
    TriageRuns {
        #[arg(long)]
        status: Option<String>,
        #[arg(long, default_value = "20")]
        limit: u32,
        #[command(flatten)]
        time: TimeRange,
    },
    /// Triage run statistics
    #[command(
        after_help = "Examples:\n  tb-lf triage-runs-stats\n  tb-lf triage-runs-stats --json"
    )]
    TriageRunsStats,
    /// Eval runs and coverage
    Eval {
        #[command(subcommand)]
        action: EvalAction,
    },
    /// Search traces
    #[command(
        after_help = "Examples:\n  tb-lf search \"login error\"\n  tb-lf search \"john smith\" --ids-only\n  tb-lf search \"timeout\" --from 3d --limit 50"
    )]
    Search {
        query: String,
        /// Output only trace IDs (for piping)
        #[arg(long)]
        ids_only: bool,
        #[command(flatten)]
        time: TimeRange,
        #[command(flatten)]
        pagination: Pagination,
    },
    /// List distinct trace names
    #[command(after_help = "Examples:\n  tb-lf tags\n  tb-lf tags --from 7d\n  tb-lf tags --json")]
    Tags {
        #[command(flatten)]
        time: TimeRange,
    },
    /// List tracked features
    #[command(
        after_help = "Examples:\n  tb-lf features\n  tb-lf features --category billing --status active\n  tb-lf features --json"
    )]
    Features {
        #[arg(long)]
        category: Option<String>,
        #[arg(long)]
        team: Option<String>,
        #[arg(long)]
        status: Option<String>,
    },
    /// Queue items for a feature
    #[command(after_help = "Examples:\n  tb-lf feature-items 5\n  tb-lf feature-items 5 --json")]
    FeatureItems { id: i64 },
    /// AI-optimized context block
    #[command(
        after_help = "Examples:\n  tb-lf prime --project production\n  tb-lf prime --mcp\n  tb-lf prime --json"
    )]
    Prime {
        /// Minimal output for MCP hook injection
        #[arg(long)]
        mcp: bool,
    },
    /// Cheat sheet for human users
    #[command(after_help = "Examples:\n  tb-lf human")]
    Human,
    /// Domain knowledge reference
    #[command(
        after_help = "Examples:\n  tb-lf explain traces\n  tb-lf explain evaluations\n  tb-lf explain --json"
    )]
    Explain {
        /// Topic: entities, relationships, traces, scores, observations, sessions, evaluations, triage, features
        topic: Option<String>,
    },
    /// Configuration management
    #[command(
        after_help = "Examples:\n  tb-lf config show\n  tb-lf config set url https://devportal.example.com\n  tb-lf config set project production"
    )]
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },
    /// Health check
    #[command(after_help = "Examples:\n  tb-lf doctor\n  tb-lf doctor --json")]
    Doctor,
    /// Manage Claude Code skill file
    Skill {
        #[command(subcommand)]
        action: toolbox_core::skill::SkillAction,
    },
    /// List feature flags seen in traces
    #[command(after_help = "Examples:\n  tb-lf flags\n  tb-lf flags --json")]
    Flags,
    /// Compare cohort stats for a feature flag (ON vs OFF)
    #[command(
        name = "flag-cohort",
        after_help = "Examples:\n  tb-lf flag-cohort aiAgentLazyOutput --from 7d\n  tb-lf flag-cohort aiAgentLazyOutput --from 2026-03-20 --to 2026-03-25\n  tb-lf flag-cohort aiAgentLazyOutput --from 7d --name agent-generation\n  tb-lf flag-cohort aiAgentLazyOutput --from 7d --json"
    )]
    FlagCohort {
        /// Flag name to compare
        flag_name: String,
        /// Filter by trace name
        #[arg(long)]
        name: Option<String>,
        /// Filter by environment
        #[arg(long)]
        env: Option<String>,
        #[command(flatten)]
        time: TimeRange,
    },
    /// Download trace summaries for a flag cohort as JSON
    #[command(
        name = "flag-traces",
        after_help = "Examples:\n  tb-lf flag-traces aiAgentLazyOutput --from 7d\n  tb-lf flag-traces aiAgentLazyOutput --value false --from 7d\n  tb-lf flag-traces aiAgentLazyOutput --from 7d --name agent-generation"
    )]
    FlagTraces {
        /// Flag name to filter by
        flag_name: String,
        /// Flag value to filter (default: true)
        #[arg(long, default_value = "true")]
        value: String,
        /// Filter by trace name
        #[arg(long)]
        name: Option<String>,
        /// Filter by environment
        #[arg(long)]
        env: Option<String>,
        #[command(flatten)]
        time: TimeRange,
        #[command(flatten)]
        pagination: Pagination,
    },
}

#[derive(clap::Subcommand)]
enum EvalAction {
    /// List eval runs
    #[command(
        after_help = "Examples:\n  tb-lf eval runs\n  tb-lf eval runs --status failed --branch main\n  tb-lf eval runs --from 7d\n  tb-lf eval runs --mode regression --limit 10"
    )]
    Runs {
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        branch: Option<String>,
        #[arg(long)]
        mode: Option<String>,
        #[arg(long, default_value = "20")]
        limit: u32,
        #[command(flatten)]
        time: TimeRange,
    },
    /// Show a single eval run
    #[command(
        after_help = "Examples:\n  tb-lf eval run 42\n  tb-lf eval run 42 --failed\n  tb-lf eval run 42 --full"
    )]
    Run {
        id: i64,
        /// Show only failed items
        #[arg(long)]
        failed: bool,
        /// Include conversation logs and errors
        #[arg(long)]
        full: bool,
    },
    /// Score trends across git revisions
    #[command(
        after_help = "Examples:\n  tb-lf eval revisions\n  tb-lf eval revisions --branch main --limit 10\n  tb-lf eval revisions --json"
    )]
    Revisions {
        #[arg(long)]
        branch: Option<String>,
        #[arg(long)]
        mode: Option<String>,
        #[arg(long, default_value = "20")]
        limit: u32,
    },
    /// Test suite coverage
    #[command(
        after_help = "Examples:\n  tb-lf eval suites\n  tb-lf eval suites --mode regression\n  tb-lf eval suites --json"
    )]
    Suites {
        #[arg(long)]
        mode: Option<String>,
        #[arg(long)]
        branch: Option<String>,
    },
    /// Test case coverage
    #[command(
        after_help = "Examples:\n  tb-lf eval cases\n  tb-lf eval cases --suite my-suite --limit 20\n  tb-lf eval cases --json"
    )]
    Cases {
        #[arg(long)]
        suite: Option<String>,
        #[arg(long)]
        mode: Option<String>,
        #[arg(long)]
        branch: Option<String>,
        #[arg(long, default_value = "50")]
        limit: u32,
    },
    /// Flaky test detection
    #[command(
        after_help = "Examples:\n  tb-lf eval flaky\n  tb-lf eval flaky --last-n 50\n  tb-lf eval flaky --branch main --json"
    )]
    Flaky {
        /// Sample size for flaky detection
        #[arg(long, default_value = "20")]
        last_n: u32,
        #[arg(long)]
        mode: Option<String>,
        #[arg(long)]
        branch: Option<String>,
    },
}

#[derive(clap::Subcommand)]
enum ConfigAction {
    /// Initialize configuration
    Init {
        /// DevPortal base URL (default: https://devportal.productive.io/)
        #[arg(long)]
        url: Option<String>,
        /// API token (prompted interactively if omitted)
        #[arg(long)]
        token: Option<String>,
        /// Default project name or ID
        #[arg(long)]
        project: Option<String>,
    },
    /// Show current configuration
    Show,
    /// Set a config value
    Set {
        /// Config key (url, token, project)
        key: String,
        /// New value (optional for project — launches interactive selection)
        value: Option<String>,
    },
}

toolbox_core::run_main!(run());

async fn run() -> tb_lf::error::Result<()> {
    let cli = Cli::parse();

    if cli.version {
        toolbox_core::version_check::print_version("tb-lf", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let Some(command) = cli.command else {
        Cli::parse_from(["tb-lf", "--help"]);
        unreachable!()
    };

    // Commands that don't need API access
    if let Commands::Config { ref action } = command {
        return handle_config(action.as_ref()).await;
    }
    if let Commands::Skill { ref action } = command {
        let skill = toolbox_core::skill::SkillConfig {
            tool_name: "tb-lf",
            content: include_str!("../SKILL.md"),
        };
        toolbox_core::skill::run(&skill, action).map_err(tb_lf::error::TbLfError::Other)?;
        return Ok(());
    }

    let config = Config::load()?;
    let client = DevPortalClient::new(&config, cli.no_cache)?;
    let project_id =
        config::resolve_project(&client, cli.project.as_deref(), config.project.as_deref()).await?;
    let pid = project_id.map(|id| id.to_string());

    match command {
        Commands::Traces {
            name,
            user,
            session,
            env,
            triage,
            satisfaction,
            sort,
            stats,
            time,
            pagination,
        } => {
            if stats {
                let mut params: Vec<(&str, Option<String>)> =
                    vec![("project_id", pid), ("name", name), ("environment", env)];
                time.push_date_params_inclusive_or_exit(&mut params);
                let path = DevPortalClient::build_path("/traces/stats", &params);
                let s: TraceStats = client.get(&path, CacheTtl::Short).await?;
                if cli.json {
                    println!("{}", output::render_json(&s));
                    return Ok(());
                }
                println!("{}\n", "Trace Stats".bold());
                println!("  Total traces: {}", s.total_traces.unwrap_or(0));
                println!(
                    "  Total cost:   {}",
                    s.total_cost.map(output::fmt_cost).unwrap_or_default()
                );
                println!("  Avg duration: {}ms", s.avg_duration_ms.unwrap_or(0.0));
                println!("  Max duration: {}ms", s.max_duration_ms.unwrap_or(0.0));
                return Ok(());
            }

            let mut params: Vec<(&str, Option<String>)> = vec![
                ("project_id", pid),
                ("name", name),
                ("user_id", user),
                ("session_id", session),
                ("environment", env),
                ("triage_status", triage),
                ("satisfaction", satisfaction),
                ("sort", sort),
            ];
            time.push_date_params_inclusive_or_exit(&mut params);
            pagination.push_params(&mut params);
            let path = DevPortalClient::build_path("/traces", &params);
            let resp: PaginatedResponse<Trace> = client.get(&path, CacheTtl::Short).await?;

            if cli.json {
                println!("{}", output::render_json(&resp.data));
                return Ok(());
            }

            if resp.data.is_empty() {
                println!(
                    "{}",
                    output::empty_hint("traces", "Try widening filters or check `tb-lf doctor`.")
                );
                return Ok(());
            }

            println!("{}\n", "Traces".bold());
            for t in &resp.data {
                let name = t
                    .display_name
                    .as_deref()
                    .or(t.name.as_deref())
                    .unwrap_or("(unnamed)");
                let cost = t.cost_usd.map(output::fmt_cost).unwrap_or_default();
                let latency = t
                    .latency_ms
                    .map(|l| format!("{:.0}ms", l))
                    .unwrap_or_default();
                let triage_str = match t.triage_status.as_deref() {
                    Some("flagged") => " FLAGGED".red().to_string(),
                    Some("dismissed") => " dismissed".dimmed().to_string(),
                    _ => String::new(),
                };
                let time = output::relative_time(&t.timestamp);

                println!(
                    "  {} {}  {}  {}  {}{}",
                    t.langfuse_id.dimmed(),
                    output::truncate(name, 40).bold(),
                    cost,
                    latency,
                    time.dimmed(),
                    triage_str,
                );
                if let Some(q) = &t.user_query {
                    println!("    {} {}", ">".dimmed(), output::truncate(q, 80));
                }
            }

            if let Some(hint) =
                output::pagination_hint(pagination.page, pagination.limit, resp.meta.total)
            {
                println!("\n  {}", hint.dimmed());
            }
            println!(
                "\n  {}",
                "Run `tb-lf trace <id>` for full details.".dimmed()
            );
        }

        Commands::Trace {
            id,
            full,
            observations,
        } => {
            let project_id = project_id.ok_or_else(|| {
                tb_lf::error::TbLfError::Config("--project required for trace fetch.".into())
            })?;
            let path = format!("/langfuse/traces/{}?project_id={}", id, project_id);
            let trace: serde_json::Value = client.get(&path, CacheTtl::Long).await?;

            if cli.json || full {
                println!("{}", output::render_json(&trace));
            } else {
                // Formatted key fields
                if let Some(input) = trace.get("input") {
                    println!("{}", "Input:".bold());
                    println!("  {}", output::truncate(&input.to_string(), 200));
                }
                if let Some(out) = trace.get("output") {
                    println!("{}", "Output:".bold());
                    println!("  {}", output::truncate(&out.to_string(), 200));
                }
                if let Some(meta) = trace.get("metadata") {
                    println!("{} {}", "Metadata:".bold(), meta);
                }
                if let Some(tags) = trace.get("tags") {
                    println!("{} {}", "Tags:".bold(), tags);
                }
                if let Some(scores) = trace.get("scores") {
                    println!("{} {}", "Scores:".bold(), scores);
                }
                println!("\n  {}", "Use --full for complete JSON.".dimmed());
            }

            if observations {
                let obs_path = DevPortalClient::build_path(
                    "/observations",
                    &[
                        ("project_id", Some(project_id.to_string())),
                        ("trace_id", Some(id)),
                    ],
                );
                let obs: Vec<Observation> = client.get(&obs_path, CacheTtl::Short).await?;
                println!("\n{} ({})\n", "Observations".bold(), obs.len());
                for o in &obs {
                    let kind = o.observation_type.as_deref().unwrap_or("?");
                    let name = o.name.as_deref().unwrap_or("(unnamed)");
                    let model = o.model.as_deref().unwrap_or("");
                    let tokens = o
                        .total_tokens
                        .map(|t| format!("{} tok", t))
                        .unwrap_or_default();
                    let cost = o.cost_usd.map(output::fmt_cost).unwrap_or_default();
                    let latency = o
                        .latency_ms
                        .map(|l| format!("{:.0}ms", l))
                        .unwrap_or_default();
                    println!(
                        "  {} [{}] {}  {}  {}  {}",
                        name.bold(),
                        kind,
                        model.dimmed(),
                        tokens,
                        cost,
                        latency
                    );
                }
            }
        }

        Commands::Sessions {
            user,
            env,
            satisfaction,
            sort,
            stats,
            time,
            pagination,
        } => {
            if stats {
                let mut params: Vec<(&str, Option<String>)> =
                    vec![("project_id", pid), ("environment", env)];
                time.push_date_params_inclusive_or_exit(&mut params);
                let path = DevPortalClient::build_path("/sessions/stats", &params);
                let s: serde_json::Value = client.get(&path, CacheTtl::Short).await?;
                println!("{}", output::render_json(&s));
                return Ok(());
            }

            let mut params: Vec<(&str, Option<String>)> = vec![
                ("project_id", pid),
                ("user_id", user),
                ("environment", env),
                ("satisfaction", satisfaction),
                ("sort", sort),
            ];
            time.push_date_params_inclusive_or_exit(&mut params);
            pagination.push_params(&mut params);
            let path = DevPortalClient::build_path("/sessions", &params);
            let resp: PaginatedResponse<Session> = client.get(&path, CacheTtl::Short).await?;

            if cli.json {
                println!("{}", output::render_json(&resp.data));
                return Ok(());
            }

            if resp.data.is_empty() {
                println!(
                    "{}",
                    output::empty_hint("sessions", "Try widening filters.")
                );
                return Ok(());
            }

            println!("{}\n", "Sessions".bold());
            for s in &resp.data {
                let cost = s.total_cost_usd.map(output::fmt_cost).unwrap_or_default();
                let time = output::relative_time(&s.last_trace_at);
                let users = s
                    .user_ids
                    .as_ref()
                    .map(|ids| ids.join(", "))
                    .unwrap_or_default();

                println!(
                    "  {} {} traces  {}  {}  {}",
                    s.session_id.bold(),
                    s.trace_count,
                    cost,
                    time.dimmed(),
                    users.dimmed(),
                );
            }

            if let Some(hint) =
                output::pagination_hint(pagination.page, pagination.limit, resp.meta.total)
            {
                println!("\n  {}", hint.dimmed());
            }
        }

        Commands::Session { id } => {
            let path =
                DevPortalClient::build_path(&format!("/sessions/{}", id), &[("project_id", pid)]);
            let traces: Vec<Trace> = client.get(&path, CacheTtl::Short).await?;

            if cli.json {
                println!("{}", output::render_json(&traces));
                return Ok(());
            }

            if traces.is_empty() {
                println!(
                    "{}",
                    output::empty_hint("traces in session", "Check the session ID.")
                );
                return Ok(());
            }

            println!("{} ({})\n", "Session".bold(), id);
            for t in &traces {
                let name = t
                    .display_name
                    .as_deref()
                    .or(t.name.as_deref())
                    .unwrap_or("(unnamed)");
                let cost = t.cost_usd.map(output::fmt_cost).unwrap_or_default();
                let latency = t
                    .latency_ms
                    .map(|l| format!("{:.0}ms", l))
                    .unwrap_or_default();

                println!(
                    "  {} {}  {}  {}",
                    t.langfuse_id.dimmed(),
                    name.bold(),
                    cost,
                    latency
                );
            }

            println!(
                "\n  {}",
                "Run `tb-lf trace <id> --project <p>` to inspect a trace.".dimmed()
            );
        }

        Commands::Observations {
            trace,
            r#type,
            model,
            level,
            env,
            time,
        } => {
            let mut params: Vec<(&str, Option<String>)> = vec![
                ("project_id", pid),
                ("trace_id", trace),
                ("type", r#type),
                ("model", model),
                ("level", level),
                ("environment", env),
            ];
            time.push_date_params_inclusive_or_exit(&mut params);
            let path = DevPortalClient::build_path("/observations", &params);
            let obs: Vec<Observation> = client.get(&path, CacheTtl::Short).await?;

            if cli.json {
                println!("{}", output::render_json(&obs));
                return Ok(());
            }

            if obs.is_empty() {
                println!(
                    "{}",
                    output::empty_hint("observations", "Try different filters.")
                );
                return Ok(());
            }

            println!("{} ({})\n", "Observations".bold(), obs.len());
            for o in &obs {
                let kind = o.observation_type.as_deref().unwrap_or("?");
                let name = o.name.as_deref().unwrap_or("(unnamed)");
                let model = o.model.as_deref().unwrap_or("");
                let tokens = o
                    .total_tokens
                    .map(|t| format!("{} tok", t))
                    .unwrap_or_default();
                let cost = o.cost_usd.map(output::fmt_cost).unwrap_or_default();
                let latency = o
                    .latency_ms
                    .map(|l| format!("{:.0}ms", l))
                    .unwrap_or_default();

                println!(
                    "  {} [{}] {}  {}  {}  {}",
                    name.bold(),
                    kind,
                    model.dimmed(),
                    tokens,
                    cost,
                    latency
                );
            }
        }

        Commands::Observation { id } => {
            let project_id = project_id.ok_or_else(|| {
                tb_lf::error::TbLfError::Config("--project required for observation fetch.".into())
            })?;
            let path = format!("/langfuse/observations/{}?project_id={}", id, project_id);
            let obs: serde_json::Value = client.get(&path, CacheTtl::Long).await?;
            println!("{}", output::render_json(&obs));
        }

        Commands::Scores {
            trace,
            name,
            source,
            env,
            limit,
            time,
        } => {
            let mut params: Vec<(&str, Option<String>)> = vec![
                ("project_id", pid),
                ("trace_id", trace),
                ("name", name),
                ("source", source),
                ("environment", env),
                ("per_page", Some(limit.to_string())),
            ];
            time.push_date_params_inclusive_or_exit(&mut params);
            let path = DevPortalClient::build_path("/scores", &params);
            let scores: Vec<Score> = client.get(&path, CacheTtl::Short).await?;

            if cli.json {
                println!("{}", output::render_json(&scores));
                return Ok(());
            }

            if scores.is_empty() {
                println!("{}", output::empty_hint("scores", "Try different filters."));
                return Ok(());
            }

            println!("{} ({})\n", "Scores".bold(), scores.len());
            for s in &scores {
                let val = s
                    .value
                    .map(output::score_color)
                    .or(s.string_value.clone())
                    .unwrap_or_default();
                let source = s.source.as_deref().unwrap_or("");
                let time = output::relative_time(&s.timestamp);

                println!(
                    "  {} {}  {}  {}  {}",
                    s.name.bold(),
                    val,
                    source.dimmed(),
                    s.trace_langfuse_id.dimmed(),
                    time.dimmed()
                );
                if let Some(c) = &s.comment {
                    println!("    {}", output::truncate(c, 80).dimmed());
                }
            }
        }

        Commands::Comments {
            trace,
            r#type,
            object,
            time,
        } => {
            let mut params: Vec<(&str, Option<String>)> = vec![
                ("project_id", pid),
                ("trace_id", trace),
                ("object_type", r#type),
                ("object_id", object),
            ];
            time.push_date_params_inclusive_or_exit(&mut params);
            let path = DevPortalClient::build_path("/comments", &params);
            let comments: Vec<Comment> = client.get(&path, CacheTtl::Short).await?;

            if cli.json {
                println!("{}", output::render_json(&comments));
                return Ok(());
            }

            if comments.is_empty() {
                println!(
                    "{}",
                    output::empty_hint("comments", "Try different filters.")
                );
                return Ok(());
            }

            println!("{} ({})\n", "Comments".bold(), comments.len());
            for c in &comments {
                let author = c.author.as_deref().unwrap_or("unknown");
                let obj_type = c.object_type.as_deref().unwrap_or("");
                let content = c.content.as_deref().unwrap_or("");
                let time = c
                    .created_at
                    .as_deref()
                    .map(output::relative_time)
                    .unwrap_or_default();

                println!(
                    "  {} [{}]  {}",
                    author.bold(),
                    obj_type.dimmed(),
                    time.dimmed()
                );
                println!("    {}", output::truncate(content, 100));
            }
        }

        Commands::Dashboard { time } => {
            let mut params: Vec<(&str, Option<String>)> = vec![("project_id", pid)];
            time.push_date_params_inclusive_or_exit(&mut params);
            let path = DevPortalClient::build_path("/dashboard", &params);
            let dash: Dashboard = client.get(&path, CacheTtl::Medium).await?;

            if cli.json {
                println!("{}", output::render_json(&dash));
                return Ok(());
            }

            println!("{}\n", "Dashboard".bold());
            if let Some(kpi) = dash.get("kpi") {
                let items = [
                    ("Sessions", "sessions"),
                    ("Unique Users", "unique_users"),
                    ("Total Cost", "total_cost"),
                    ("Avg Cost", "avg_cost"),
                    ("Satisfaction", "satisfaction"),
                    ("Latency p50", "latency_p50"),
                ];
                for (label, key) in items {
                    if let Some(obj) = kpi.get(key) {
                        let val = obj.get("value").and_then(|v| v.as_f64());
                        let prev = obj.get("previous").and_then(|v| v.as_f64());
                        let val_str = val.map(|v| format!("{:.2}", v)).unwrap_or("—".into());
                        let change = match (val, prev) {
                            (Some(v), Some(p)) if p > 0.0 => {
                                let pct = ((v - p) / p) * 100.0;
                                if pct >= 0.0 {
                                    format!("+{:.0}%", pct).green().to_string()
                                } else {
                                    format!("{:.0}%", pct).red().to_string()
                                }
                            }
                            _ => String::new(),
                        };
                        println!("  {:<16} {}  {}", label, val_str.bold(), change);
                    }
                }
            }

            if let Some(feedback) = dash.get("feedback") {
                println!("\n  {}", "Feedback".bold());
                let pos = feedback
                    .get("positive")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let neg = feedback
                    .get("negative")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let total = feedback
                    .get("total_sessions")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                println!(
                    "    {} positive  {} negative  {} total sessions",
                    pos.to_string().green(),
                    neg.to_string().red(),
                    total
                );
            }

            println!(
                "\n  {}",
                "Run `tb-lf traces` to drill into individual traces.".dimmed()
            );
            println!(
                "  {}",
                "Run `tb-lf daily-metrics` for daily trends.".dimmed()
            );
        }

        Commands::DailyMetrics { days, env, time } => {
            let effective_time = if let Some(d) = days {
                TimeRange {
                    from: Some(format!("{}d", d)),
                    to: time.to,
                    ..Default::default()
                }
            } else if !time.has_from() {
                TimeRange {
                    from: Some("7d".into()),
                    ..Default::default()
                }
            } else {
                time
            };

            let mut params: Vec<(&str, Option<String>)> =
                vec![("project_id", pid), ("environment", env)];
            effective_time.push_date_params_inclusive_or_exit(&mut params);
            let path = DevPortalClient::build_path("/daily_metrics", &params);
            let metrics: Vec<DailyMetric> = client.get(&path, CacheTtl::Short).await?;

            if cli.json {
                println!("{}", output::render_json(&metrics));
                return Ok(());
            }

            if metrics.is_empty() {
                println!(
                    "{}",
                    output::empty_hint("metrics", "Try a wider date range.")
                );
                return Ok(());
            }

            println!("{}\n", "Daily Metrics".bold());
            println!(
                "  {:<12} {:>8} {:>6} {:>10} {:>10} {:>8}",
                "Date", "Traces", "Users", "Cost", "Latency", "Errors"
            );
            println!("  {}", "─".repeat(60));
            for m in &metrics {
                println!(
                    "  {:<12} {:>8} {:>6} {:>10} {:>10} {:>8}",
                    m.date,
                    m.trace_count.unwrap_or(0),
                    m.unique_users.unwrap_or(0),
                    m.total_cost_usd.map(output::fmt_cost).unwrap_or_default(),
                    m.avg_latency_ms
                        .map(|l| format!("{:.0}ms", l))
                        .unwrap_or_default(),
                    m.error_count.unwrap_or(0),
                );
            }
        }

        Commands::TraceMetrics {
            trace_id,
            group_by,
            env,
            outcome,
            time,
        } => {
            if let Some(tid) = trace_id {
                // Per-trace detail
                let path = DevPortalClient::build_path(
                    &format!("/trace_metrics/{}", tid),
                    &[("project_id", pid)],
                );
                let detail: TraceMetricDetail = client.get(&path, CacheTtl::Short).await?;

                if cli.json {
                    println!("{}", output::render_json(&detail));
                    return Ok(());
                }

                // Trace info
                if let Some(t) = &detail.trace {
                    println!("{}\n", "Trace".bold());
                    if let Some(id) = &t.langfuse_id {
                        println!("  {:<16} {}", "ID".dimmed(), id);
                    }
                    if let Some(ts) = &t.timestamp {
                        println!("  {:<16} {}", "Timestamp".dimmed(), ts);
                    }
                    if let Some(env) = &t.environment {
                        println!("  {:<16} {}", "Environment".dimmed(), env);
                    }
                    if let Some(cost) = t.cost_usd {
                        println!("  {:<16} {}", "Cost".dimmed(), output::fmt_cost(cost));
                    }
                    if let Some(lat) = t.latency_ms {
                        println!("  {:<16} {:.0}ms", "Latency".dimmed(), lat);
                    }
                    if let Some(q) = &t.user_query {
                        println!("  {:<16} {}", "Query".dimmed(), output::truncate(q, 120));
                    }
                }

                // Metrics
                if let Some(m) = &detail.metrics {
                    println!("\n{}\n", "Metrics".bold());
                    println!("  {:<24} {}", "Turns".dimmed(), m.turn_count.unwrap_or(0));
                    println!(
                        "  {:<24} {}",
                        "Tool calls".dimmed(),
                        m.tool_call_count.unwrap_or(0)
                    );
                    println!(
                        "  {:<24} {}",
                        "Tool errors".dimmed(),
                        m.tool_error_count.unwrap_or(0)
                    );
                    println!(
                        "  {:<24} {}",
                        "Unique tools".dimmed(),
                        m.unique_tool_count.unwrap_or(0)
                    );
                    println!(
                        "  {:<24} {}",
                        "Input tokens".dimmed(),
                        m.total_input_tokens.unwrap_or(0)
                    );
                    println!(
                        "  {:<24} {}",
                        "Output tokens".dimmed(),
                        m.total_output_tokens.unwrap_or(0)
                    );
                    if let Some(dr) = m.describe_resource_calls {
                        println!(
                            "  {:<24} {} ({} tokens)",
                            "describe_resource".dimmed(),
                            dr,
                            m.describe_resource_tokens.unwrap_or(0)
                        );
                    }
                    if let Some(true) = m.has_retry_pattern {
                        println!("  {:<24} {}", "Retry pattern".dimmed(), "yes".yellow());
                    }
                    if let Some(true) = m.has_errors {
                        println!("  {:<24} {}", "Has errors".dimmed(), "yes".red());
                    }
                    if let Some(agent) = &m.agent_type {
                        println!("  {:<24} {}", "Agent type".dimmed(), agent);
                    }
                    if let Some(out) = &m.outcome {
                        println!("  {:<24} {}", "Outcome".dimmed(), out);
                    }

                    // Tool breakdown
                    if let Some(tb) = &m.tool_breakdown
                        && let Some(obj) = tb.as_object()
                        && !obj.is_empty()
                    {
                        println!("\n  {}", "Tool breakdown:".dimmed());
                        let mut tools: Vec<_> = obj.iter().collect();
                        tools.sort_by(|a, b| {
                            b.1.as_u64().unwrap_or(0).cmp(&a.1.as_u64().unwrap_or(0))
                        });
                        for (name, count) in tools {
                            println!("    {:<28} {:>4}", name, count);
                        }
                    }
                } else {
                    println!("\n{}", "No metrics computed for this trace yet.".dimmed());
                }

                // Flags
                if let Some(flags) = &detail.flags
                    && !flags.is_empty()
                {
                    println!("\n{}\n", "Flags".bold());
                    for (k, v) in flags {
                        println!("  {:<28} {}", k.dimmed(), v);
                    }
                }

                // Triage
                if let Some(triage) = &detail.triage {
                    println!("\n{}\n", "Triage".bold());
                    if let Some(cat) = &triage.category {
                        println!("  {:<16} {}", "Category".dimmed(), cat);
                    }
                    if let Some(conf) = &triage.confidence {
                        println!("  {:<16} {}", "Confidence".dimmed(), conf);
                    }
                }
            } else {
                // Aggregated metrics
                let effective_time = if !time.has_from() {
                    TimeRange {
                        from: Some("7d".into()),
                        ..Default::default()
                    }
                } else {
                    time
                };

                let mut params: Vec<(&str, Option<String>)> = vec![
                    ("project_id", pid),
                    ("environment", env),
                    ("group_by", Some(group_by.clone())),
                    ("outcome", outcome),
                ];
                effective_time.push_date_params_inclusive_or_exit(&mut params);
                let path = DevPortalClient::build_path("/trace_metrics", &params);
                let resp: TraceMetricAggregateResponse = client.get(&path, CacheTtl::Short).await?;

                if cli.json {
                    println!("{}", output::render_json(&resp));
                    return Ok(());
                }

                if resp.data.is_empty() {
                    println!(
                        "{}",
                        output::empty_hint(
                            "trace-metrics",
                            "Try a wider date range or check that the metrics job has run."
                        )
                    );
                    return Ok(());
                }

                println!(
                    "{} {}\n",
                    "Trace Metrics".bold(),
                    format!("(group_by: {})", group_by).dimmed()
                );
                println!(
                    "  {:<16} {:>7} {:>6} {:>6} {:>7} {:>7} {:>9} {:>9} {:>8}",
                    "Group",
                    "Traces",
                    "Turns",
                    "Tools",
                    "Err%",
                    "Retry%",
                    "Cost",
                    "Latency",
                    "Success%"
                );
                println!("  {}", "─".repeat(90));
                for row in &resp.data {
                    let err_pct = row
                        .error_rate
                        .map(|r| format!("{:.1}%", r * 100.0))
                        .unwrap_or_default();
                    let retry_pct = row
                        .retry_pattern_rate
                        .map(|r| format!("{:.1}%", r * 100.0))
                        .unwrap_or_default();
                    let success_pct = row
                        .success_rate
                        .map(|r| format!("{:.1}%", r * 100.0))
                        .unwrap_or_default();
                    println!(
                        "  {:<16} {:>7} {:>6} {:>6} {:>7} {:>7} {:>9} {:>9} {:>8}",
                        output::truncate(&row.group, 16),
                        row.trace_count.unwrap_or(0),
                        row.avg_turn_count
                            .map(|v| format!("{:.1}", v))
                            .unwrap_or_default(),
                        row.avg_tool_calls
                            .map(|v| format!("{:.1}", v))
                            .unwrap_or_default(),
                        err_pct,
                        retry_pct,
                        row.avg_cost_usd.map(output::fmt_cost).unwrap_or_default(),
                        row.avg_latency_ms
                            .map(|l| format!("{:.0}ms", l))
                            .unwrap_or_default(),
                        success_pct,
                    );
                }
            }
        }

        Commands::Queue {
            status,
            category,
            confidence,
            run,
            feature,
            team,
            full,
            time,
            pagination,
        } => {
            let mut params: Vec<(&str, Option<String>)> = vec![
                ("project_id", pid),
                ("status", status),
                ("category", category),
                ("confidence", confidence),
                ("triage_run_id", run),
                ("feature_id", feature),
                ("team_id", team),
            ];
            time.push_date_params_inclusive_or_exit(&mut params);
            pagination.push_params(&mut params);
            let path = DevPortalClient::build_path("/queue_items", &params);
            let items: Vec<QueueItem> = client.get(&path, CacheTtl::Short).await?;

            if cli.json {
                println!("{}", output::render_json(&items));
                return Ok(());
            }

            if items.is_empty() {
                println!(
                    "{}",
                    output::empty_hint("queue items", "Try different filters.")
                );
                return Ok(());
            }

            println!("{} ({})\n", "Queue Items".bold(), items.len());
            for item in &items {
                let status = item.status.as_deref().unwrap_or("?");
                let status_colored = match status {
                    "pending_review" => status.yellow().to_string(),
                    "confirmed" => status.green().to_string(),
                    "dismissed" => status.dimmed().to_string(),
                    _ => status.to_string(),
                };
                let cat = item.ai_category.as_deref().unwrap_or("");
                let conf = item.ai_confidence.as_deref().unwrap_or("");
                let trace = item.trace_langfuse_id.as_deref().unwrap_or("");
                let team_name = item
                    .team
                    .as_ref()
                    .or(item.ai_team.as_ref())
                    .map(|t| t.name.as_str())
                    .unwrap_or("");

                println!(
                    "  {} [{}] {} {}  {}  {}",
                    trace.dimmed(),
                    status_colored,
                    cat,
                    conf.dimmed(),
                    team_name.cyan(),
                    item.reviewed_by.as_deref().unwrap_or("").dimmed()
                );

                if full {
                    if let Some(reasoning) = &item.ai_reasoning {
                        println!("    {}", reasoning);
                    }
                } else if let Some(reasoning) = &item.ai_reasoning {
                    println!("    {}", output::truncate(reasoning, 80).dimmed());
                }
            }
        }

        Commands::QueueStats => {
            let path = DevPortalClient::build_path("/queue_items/stats", &[("project_id", pid)]);
            let stats: serde_json::Value = client.get(&path, CacheTtl::Short).await?;
            if cli.json {
                println!("{}", output::render_json(&stats));
            } else {
                println!("{}\n", "Queue Stats".bold());
                let total = stats.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
                println!("  Total: {}\n", total);
                for section in [
                    "by_status",
                    "by_category",
                    "by_ai_category",
                    "by_ai_confidence",
                ] {
                    if let Some(obj) = stats.get(section).and_then(|v| v.as_object()) {
                        if obj.is_empty() {
                            continue;
                        }
                        println!("  {}:", section);
                        for (k, v) in obj {
                            if !k.is_empty() {
                                println!("    {}: {}", k, v);
                            }
                        }
                        println!();
                    }
                }
            }
        }

        Commands::QueueItem { id } => {
            let path = DevPortalClient::build_path(
                &format!("/queue_items/{}", id),
                &[("project_id", pid)],
            );
            let item: QueueItem = client.get(&path, CacheTtl::Short).await?;

            if cli.json {
                println!("{}", output::render_json(&item));
                return Ok(());
            }

            println!("{} #{}\n", "Queue Item".bold(), id);
            println!(
                "  Trace:      {}",
                item.trace_langfuse_id.as_deref().unwrap_or("—")
            );
            println!("  Status:     {}", item.status.as_deref().unwrap_or("—"));
            println!(
                "  Category:   {} (AI: {})",
                item.category.as_deref().unwrap_or("—"),
                item.ai_category.as_deref().unwrap_or("—")
            );
            println!(
                "  Confidence: {}",
                item.ai_confidence.as_deref().unwrap_or("—")
            );
            let team_display = item
                .team
                .as_ref()
                .map(|t| format!("{} (id:{})", t.name, t.id))
                .unwrap_or_else(|| "—".into());
            let ai_team_display = item
                .ai_team
                .as_ref()
                .map(|t| t.name.clone())
                .unwrap_or_else(|| "—".into());
            println!("  Team:       {} (AI: {})", team_display, ai_team_display);
            let feature_display = item
                .feature
                .as_ref()
                .map(|f| format!("{} (id:{})", f.name, f.id))
                .unwrap_or_else(|| "—".into());
            let ai_feature_display = item
                .ai_feature
                .as_ref()
                .map(|f| f.name.clone())
                .unwrap_or_else(|| "—".into());
            println!(
                "  Feature:    {} (AI: {})",
                feature_display, ai_feature_display
            );
            println!(
                "  Reviewed:   {}",
                item.reviewed_by.as_deref().unwrap_or("—")
            );
            if let Some(note) = &item.note {
                println!("  Note:       {}", note);
            }
            println!("  Source:     {}", item.source.as_deref().unwrap_or("—"));
            println!(
                "  Created:    {}",
                item.created_at.as_deref().unwrap_or("—")
            );
            if let Some(reasoning) = &item.ai_reasoning {
                println!("\n  {}", "AI Reasoning:".bold());
                println!("  {}", reasoning);
            }
            if let Some(trace_id) = &item.trace_langfuse_id {
                println!(
                    "\n  {}",
                    format!("Run `tb-lf trace {}` to see the full trace.", trace_id).dimmed()
                );
            }
        }

        Commands::QueueUpdate {
            id,
            status,
            category,
            note,
            feature,
            no_feature,
            team,
            no_team,
            reviewed_by,
        } => {
            let update = QueueItemUpdate {
                status,
                category,
                note,
                feature_id: to_nullable(feature, no_feature),
                team_id: to_nullable(team, no_team),
                reviewed_by,
                ..Default::default()
            };
            let path = format!("/queue_items/{}", id);
            let item: QueueItem = client.patch(&path, &update).await?;

            if cli.json {
                println!("{}", output::render_json(&item));
                return Ok(());
            }

            println!("{} Queue item #{} updated.", "OK".green().bold(), id);
            println!(
                "  Status: {}  Category: {}  Team: {}",
                item.status.as_deref().unwrap_or("—"),
                item.category.as_deref().unwrap_or("—"),
                item.team.as_ref().map(|t| t.name.as_str()).unwrap_or("—"),
            );
        }

        Commands::QueueBulkUpdate {
            ids,
            filter_status,
            filter_category,
            filter_confidence,
            filter_feature,
            filter_team,
            set_status,
            set_category,
            set_note,
            set_feature,
            no_feature,
            set_team,
            no_team,
            set_reviewed_by,
            dry_run,
            time,
        } => {
            let target_ids: Vec<i64> = if !ids.is_empty() {
                ids
            } else {
                // Fetch all matching items, paginating through all pages
                let mut collected_ids: Vec<i64> = Vec::new();
                let mut page = 1u32;
                loop {
                    let mut params: Vec<(&str, Option<String>)> = vec![
                        ("project_id", pid.clone()),
                        ("status", filter_status.clone()),
                        ("category", filter_category.clone()),
                        ("confidence", filter_confidence.clone()),
                        ("feature_id", filter_feature.clone()),
                        ("team_id", filter_team.clone()),
                        ("per_page", Some("200".into())),
                        ("page", Some(page.to_string())),
                    ];
                    time.push_date_params_inclusive_or_exit(&mut params);
                    let path = DevPortalClient::build_path("/queue_items", &params);
                    let batch: Vec<QueueItem> = client.get(&path, CacheTtl::Short).await?;
                    let batch_len = batch.len();
                    collected_ids.extend(batch.iter().map(|i| i.id));
                    if batch_len < 200 {
                        break;
                    }
                    page += 1;
                }

                if collected_ids.is_empty() {
                    println!(
                        "{}",
                        output::empty_hint("queue items", "No items match those filters.")
                    );
                    return Ok(());
                }

                collected_ids
            };

            let update = QueueItemUpdate {
                status: set_status,
                category: set_category,
                note: set_note,
                feature_id: to_nullable(set_feature, no_feature),
                team_id: to_nullable(set_team, no_team),
                reviewed_by: set_reviewed_by,
                ..Default::default()
            };

            if dry_run {
                println!(
                    "{} Would update {} items. Sample IDs: {:?}",
                    "DRY RUN".yellow().bold(),
                    target_ids.len(),
                    &target_ids[..target_ids.len().min(10)]
                );
                println!("  Payload: {}", serde_json::to_string_pretty(&update)?);
                return Ok(());
            }

            println!("Updating {} items...", target_ids.len());

            let mut total_updated = 0u32;
            for chunk in target_ids.chunks(500) {
                let payload = serde_json::json!({
                    "ids": chunk,
                    "updates": &update,
                });
                let resp: serde_json::Value =
                    client.patch("/queue_items/bulk_update", &payload).await?;
                let updated = resp
                    .get("updated_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                total_updated += updated;
            }
            println!("{} {} updated.", "Done.".green().bold(), total_updated,);
        }

        Commands::QueueDelete { id } => {
            let path = format!("/queue_items/{}", id);
            client.delete(&path).await?;
            println!("{} Queue item #{} deleted.", "OK".green().bold(), id);
        }

        Commands::Teams { status } => {
            let params: Vec<(&str, Option<String>)> = vec![("project_id", pid), ("status", status)];
            let path = DevPortalClient::build_path("/teams", &params);
            let resp: PaginatedResponse<Team> = client.get(&path, CacheTtl::Short).await?;
            let teams = resp.data;

            if cli.json {
                println!("{}", output::render_json(&teams));
                return Ok(());
            }

            if teams.is_empty() {
                println!("{}", output::empty_hint("teams", "No teams found."));
                return Ok(());
            }

            println!("{} ({})\n", "Teams".bold(), teams.len());
            for team in &teams {
                let status = team.status.as_deref().unwrap_or("");
                println!("  {:>4}  {}  {}", team.id, team.name, status.dimmed());
            }
        }

        Commands::TriageRuns {
            status,
            limit,
            time,
        } => {
            let mut params: Vec<(&str, Option<String>)> = vec![
                ("project_id", pid),
                ("status", status),
                ("per_page", Some(limit.to_string())),
            ];
            time.push_date_params_inclusive_or_exit(&mut params);
            let path = DevPortalClient::build_path("/triage_runs", &params);
            let resp: tb_lf::api::PaginatedResponse<TriageRun> =
                client.get(&path, CacheTtl::Short).await?;
            let runs = resp.data;

            if cli.json {
                println!("{}", output::render_json(&runs));
                return Ok(());
            }

            if runs.is_empty() {
                println!("{}", output::empty_hint("triage runs", "No runs found."));
                return Ok(());
            }

            println!("{} ({})\n", "Triage Runs".bold(), runs.len());
            for r in &runs {
                let status = r.status.as_deref().unwrap_or("?");
                let status_colored = match status {
                    "completed" => status.green().to_string(),
                    "running" => status.yellow().to_string(),
                    "failed" => status.red().to_string(),
                    _ => status.to_string(),
                };
                let processed = r.processed_count.unwrap_or(0);
                let flagged = r.flagged_count.unwrap_or(0);
                let dismissed = r.dismissed_count.unwrap_or(0);
                let model = r.model.as_deref().unwrap_or("");
                let cost = r
                    .cost_cents
                    .map(|c| output::fmt_cost(c / 100.0))
                    .unwrap_or_default();
                let time = r
                    .created_at
                    .as_deref()
                    .map(output::relative_time)
                    .unwrap_or_default();

                println!(
                    "  #{} [{}]  {} processed, {} flagged, {} dismissed  {}  {}  {}",
                    r.id,
                    status_colored,
                    processed,
                    flagged,
                    dismissed,
                    model.dimmed(),
                    cost,
                    time.dimmed()
                );
            }
        }

        Commands::TriageRunsStats => {
            let path = DevPortalClient::build_path("/triage_runs/stats", &[("project_id", pid)]);
            let stats: serde_json::Value = client.get(&path, CacheTtl::Short).await?;
            if cli.json {
                println!("{}", output::render_json(&stats));
            } else {
                println!("{}\n", "Triage Stats".bold());

                if let Some(pending) = stats.get("pending_trace_count").and_then(|v| v.as_u64()) {
                    println!("  Pending traces: {}", pending);
                }
                if let Some(days) = stats.get("lookback_days").and_then(|v| v.as_u64()) {
                    println!("  Lookback: {} days", days);
                }

                if let Some(summary) = stats.get("queue_summary") {
                    let total = summary.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
                    println!("\n  Queue: {} total", total);
                    if let Some(obj) = summary.get("by_status").and_then(|v| v.as_object()) {
                        for (k, v) in obj {
                            println!("    {}: {}", k, v);
                        }
                    }
                }

                if let Some(runs) = stats.get("runs").and_then(|v| v.as_array()) {
                    let show = runs.iter().take(5);
                    println!("\n  Recent runs:");
                    for r in show {
                        let id = r.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
                        let status = r.get("status").and_then(|v| v.as_str()).unwrap_or("?");
                        let processed = r
                            .get("total_processed")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        let flagged = r.get("flagged_count").and_then(|v| v.as_u64()).unwrap_or(0);
                        let dismissed = r
                            .get("dismissed_count")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                        let cost = r
                            .get("cost_cents")
                            .and_then(|v| v.as_f64())
                            .map(|c| c / 100.0);
                        let cost_str = cost.map(output::fmt_cost).unwrap_or_default();
                        println!(
                            "    #{} [{}]  {} processed, {} flagged, {} dismissed  {}",
                            id, status, processed, flagged, dismissed, cost_str
                        );
                    }
                }
                println!();
            }
        }

        Commands::Eval { action } => match action {
            EvalAction::Runs {
                status,
                branch,
                mode,
                limit,
                time,
            } => {
                let mut params: Vec<(&str, Option<String>)> = vec![
                    ("project_id", pid),
                    ("status", status),
                    ("branch", branch),
                    ("mode", mode),
                    ("per_page", Some(limit.to_string())),
                ];
                time.push_date_params_inclusive_or_exit(&mut params);
                let path = DevPortalClient::build_path("/eval/runs", &params);
                let resp: tb_lf::api::PaginatedResponse<EvalRun> =
                    client.get(&path, CacheTtl::Short).await?;
                let runs = resp.data;

                if cli.json {
                    println!("{}", output::render_json(&runs));
                    return Ok(());
                }

                if runs.is_empty() {
                    println!(
                        "{}",
                        output::empty_hint("eval runs", "Try different filters.")
                    );
                    return Ok(());
                }

                println!("{} ({})\n", "Eval Runs".bold(), runs.len());
                for r in &runs {
                    let name = r.name.as_deref().unwrap_or("(unnamed)");
                    let branch = r.branch.as_deref().unwrap_or("");
                    let status = r.status.as_deref().unwrap_or("?");
                    let status_colored = match status {
                        "passed" | "completed" => status.green().to_string(),
                        "failed" => status.red().to_string(),
                        "running" => status.yellow().to_string(),
                        _ => status.to_string(),
                    };
                    let total = r.total_cases.unwrap_or(0);
                    let passed = r.passed_cases.unwrap_or(0);
                    let failed = r.failed_cases.unwrap_or(0);
                    let score = r.total_score.map(output::score_color).unwrap_or_default();
                    let dur = r
                        .duration_ms
                        .map(|d| format!("{:.1}s", d / 1000.0))
                        .unwrap_or_default();

                    println!(
                        "  {} {} [{}]  {}/{}/{} (pass/fail/total)  {}  {}",
                        name.bold(),
                        branch.dimmed(),
                        status_colored,
                        passed,
                        failed,
                        total,
                        score,
                        dur,
                    );
                }
            }

            EvalAction::Run { id, failed, full } => {
                let path = DevPortalClient::build_path(
                    &format!("/eval/runs/{}", id),
                    &[("project_id", pid)],
                );
                let detail: EvalRunDetail = client.get(&path, CacheTtl::Medium).await?;

                if cli.json {
                    println!("{}", output::render_json(&detail));
                    return Ok(());
                }

                let r = &detail.run;
                println!("{} #{}\n", "Eval Run".bold(), id);
                println!("  Name:   {}", r.name.as_deref().unwrap_or("—"));
                println!("  Branch: {}", r.branch.as_deref().unwrap_or("—"));
                println!("  Status: {}", r.status.as_deref().unwrap_or("—"));
                println!(
                    "  Score:  {}",
                    r.total_score.map(output::score_color).unwrap_or_default()
                );
                println!(
                    "  Cases:  {} total, {} passed, {} failed",
                    r.total_cases.unwrap_or(0),
                    r.passed_cases.unwrap_or(0),
                    r.failed_cases.unwrap_or(0)
                );

                if let Some(items) = &detail.items {
                    let items: Vec<&EvalItem> = if failed {
                        items
                            .iter()
                            .filter(|i| i.status.as_deref() == Some("failed"))
                            .collect()
                    } else {
                        items.iter().collect()
                    };

                    println!("\n  {}\n", "Items:".bold());
                    for item in &items {
                        let suite = item.suite.as_deref().unwrap_or("");
                        let case = item.case.as_deref().unwrap_or("");
                        let status = item.status.as_deref().unwrap_or("?");
                        let status_colored = match status {
                            "passed" => status.green().to_string(),
                            "failed" => status.red().to_string(),
                            _ => status.to_string(),
                        };
                        let score = item.score.map(output::score_color).unwrap_or_default();
                        let dur = item
                            .duration_seconds
                            .map(|d| format!("{:.0}s", d))
                            .unwrap_or_default();

                        println!(
                            "  {} / {} [{}]  {}  {}",
                            suite,
                            case.bold(),
                            status_colored,
                            score,
                            dur
                        );

                        if full {
                            if let Some(err) = &item.error_message {
                                println!("    {}: {}", "Error".red(), err);
                            }
                            if let Some(log) = &item.conversation_log {
                                println!("    {}", output::truncate(log, 200));
                            }
                        }

                        if let Some(trace_id) = &item.trace_langfuse_id {
                            println!("    trace: {}", trace_id.dimmed());
                        }
                    }
                }
            }

            EvalAction::Revisions {
                branch,
                mode,
                limit,
            } => {
                let path = DevPortalClient::build_path(
                    "/eval/runs/revisions",
                    &[
                        ("project_id", pid),
                        ("branch", branch),
                        ("mode", mode),
                        ("per_page", Some(limit.to_string())),
                    ],
                );
                let revs: Vec<EvalRevision> = client.get(&path, CacheTtl::Short).await?;

                if cli.json {
                    println!("{}", output::render_json(&revs));
                    return Ok(());
                }

                if revs.is_empty() {
                    println!(
                        "{}",
                        output::empty_hint("eval revisions", "No revisions found.")
                    );
                    return Ok(());
                }

                println!("{} ({})\n", "Eval Revisions".bold(), revs.len());
                for rev in &revs {
                    let sha = rev.revision.as_deref().unwrap_or("?");
                    let short_sha = if sha.len() > 7 { &sha[..7] } else { sha };
                    let msg = rev.revision_message.as_deref().unwrap_or("");
                    let date = rev
                        .latest_started_at
                        .as_deref()
                        .map(output::relative_time)
                        .unwrap_or_default();
                    let score = rev.avg_score.map(output::score_color).unwrap_or_default();
                    let passed = rev.total_passed.unwrap_or(0);
                    let failed = rev.total_failed.unwrap_or(0);
                    let runs = rev.runs_count.unwrap_or(0);

                    println!(
                        "  {} {}  {} runs  {}  {}/{} pass/fail  {}",
                        short_sha.yellow(),
                        output::truncate(msg, 40),
                        runs,
                        score,
                        passed,
                        failed,
                        date.dimmed()
                    );
                }
            }

            EvalAction::Suites { mode, branch } => {
                let path = DevPortalClient::build_path(
                    "/eval/coverage/suites",
                    &[("project_id", pid), ("mode", mode), ("branch", branch)],
                );
                let suites: Vec<EvalSuite> = client.get(&path, CacheTtl::Short).await?;

                if cli.json {
                    println!("{}", output::render_json(&suites));
                    return Ok(());
                }

                if suites.is_empty() {
                    println!("{}", output::empty_hint("eval suites", "No suites found."));
                    return Ok(());
                }

                println!("{} ({})\n", "Eval Suites".bold(), suites.len());
                for s in &suites {
                    let name = s
                        .suite_name
                        .as_deref()
                        .or(s.suite_key.as_deref())
                        .unwrap_or("(unnamed)");
                    let runs = s.run_count.unwrap_or(0);
                    let last = s
                        .last_run_at
                        .as_deref()
                        .map(output::relative_time)
                        .unwrap_or_else(|| "—".to_string());
                    println!("  {}  {} runs  last: {}", name.bold(), runs, last.dimmed());
                }
            }

            EvalAction::Cases {
                suite,
                mode,
                branch,
                limit,
            } => {
                let path = DevPortalClient::build_path(
                    "/eval/coverage/cases",
                    &[
                        ("project_id", pid),
                        ("suite", suite),
                        ("mode", mode),
                        ("branch", branch),
                        ("per_page", Some(limit.to_string())),
                    ],
                );
                let cases: Vec<EvalCase> = client.get(&path, CacheTtl::Short).await?;

                if cli.json {
                    println!("{}", output::render_json(&cases));
                    return Ok(());
                }

                if cases.is_empty() {
                    println!("{}", output::empty_hint("eval cases", "No cases found."));
                    return Ok(());
                }

                println!("{} ({})\n", "Eval Cases".bold(), cases.len());
                for c in &cases {
                    let suite = c
                        .suite_name
                        .as_deref()
                        .or(c.suite_key.as_deref())
                        .unwrap_or("");
                    let case = c
                        .case_name
                        .as_deref()
                        .or(c.case_key.as_deref())
                        .unwrap_or("");
                    let runs = c.run_count.unwrap_or(0);
                    let rate = c.pass_rate.map(output::score_color).unwrap_or_default();
                    let last = c
                        .last_run_at
                        .as_deref()
                        .map(output::relative_time)
                        .unwrap_or_else(|| "—".to_string());

                    println!(
                        "  {} / {}  {} runs  {}  last: {}",
                        suite,
                        case.bold(),
                        runs,
                        rate,
                        last.dimmed()
                    );
                }
            }

            EvalAction::Flaky {
                last_n,
                mode,
                branch,
            } => {
                let path = DevPortalClient::build_path(
                    "/eval/coverage/flaky",
                    &[
                        ("project_id", pid),
                        ("last_n", Some(last_n.to_string())),
                        ("mode", mode),
                        ("branch", branch),
                    ],
                );
                let flaky: Vec<EvalFlaky> = client.get(&path, CacheTtl::Short).await?;

                if cli.json {
                    println!("{}", output::render_json(&flaky));
                    return Ok(());
                }

                if flaky.is_empty() {
                    println!("No flaky tests detected.");
                    return Ok(());
                }

                println!("{} ({})\n", "Flaky Tests".bold().yellow(), flaky.len());
                for f in &flaky {
                    let suite = f
                        .suite_name
                        .as_deref()
                        .or(f.suite_key.as_deref())
                        .unwrap_or("");
                    let case = f
                        .case_name
                        .as_deref()
                        .or(f.case_key.as_deref())
                        .unwrap_or("");
                    let sample = f.sample_size.unwrap_or(0);
                    let passed = f.passed_count.unwrap_or(0);
                    let rate = f
                        .pass_rate
                        .map(|r| format!("{:.0}%", r * 100.0).yellow().to_string())
                        .unwrap_or_default();

                    println!(
                        "  {} / {}  {}/{} passed ({})  ",
                        suite,
                        case.bold(),
                        passed,
                        sample,
                        rate
                    );
                }
            }
        },

        Commands::Search {
            query,
            ids_only,
            time,
            pagination,
        } => {
            // Try devportal search endpoint, fall back to traces with name filter
            let mut params: Vec<(&str, Option<String>)> =
                vec![("project_id", pid.clone()), ("q", Some(query.clone()))];
            time.push_date_params_inclusive_or_exit(&mut params);
            pagination.push_params(&mut params);
            let path = DevPortalClient::build_path("/search", &params);

            let result = client.get_raw(&path, CacheTtl::Short).await;
            match result {
                Ok(body) => {
                    // Search endpoint exists
                    let resp: PaginatedResponse<SearchResult> = serde_json::from_str(&body)?;

                    if cli.json {
                        println!("{}", output::render_json(&resp.data));
                        return Ok(());
                    }

                    if ids_only {
                        for r in &resp.data {
                            println!("{}", r.trace.langfuse_id);
                        }
                        return Ok(());
                    }

                    if resp.data.is_empty() {
                        println!(
                            "{}",
                            output::empty_hint("search results", "Try a different query.")
                        );
                        return Ok(());
                    }

                    println!(
                        "{} for \"{}\" ({})\n",
                        "Search".bold(),
                        query,
                        resp.data.len()
                    );
                    for r in &resp.data {
                        let name = r
                            .trace
                            .display_name
                            .as_deref()
                            .or(r.trace.name.as_deref())
                            .unwrap_or("(unnamed)");
                        let match_type = r.match_type.as_deref().unwrap_or("");
                        let match_type_colored = match match_type {
                            "name" => match_type.green().to_string(),
                            "user_id" => match_type.green().to_string(),
                            "tags" => match_type.cyan().to_string(),
                            "user_query" | "agent_response" => match_type.yellow().to_string(),
                            _ => match_type.to_string(),
                        };
                        let time = output::relative_time(&r.trace.timestamp);

                        println!(
                            "  {} {} [{}]  {}",
                            r.trace.langfuse_id.dimmed(),
                            name.bold(),
                            match_type_colored,
                            time.dimmed()
                        );
                        if let Some(ctx) = &r.match_context {
                            println!("    {}", output::truncate(ctx, 80).dimmed());
                        }
                    }

                    if let Some(hint) =
                        output::pagination_hint(pagination.page, pagination.limit, resp.meta.total)
                    {
                        println!("\n  {}", hint.dimmed());
                    }
                }
                Err(tb_lf::error::TbLfError::Api { status: 404, .. }) => {
                    // Search endpoint not deployed — fall back to traces name filter
                    let mut params: Vec<(&str, Option<String>)> =
                        vec![("project_id", pid), ("name", Some(query.clone()))];
                    time.push_date_params_inclusive_or_exit(&mut params);
                    pagination.push_params(&mut params);
                    let path = DevPortalClient::build_path("/traces", &params);
                    let resp: PaginatedResponse<Trace> = client.get(&path, CacheTtl::Short).await?;

                    if cli.json {
                        println!("{}", output::render_json(&resp.data));
                        return Ok(());
                    }

                    if ids_only {
                        for t in &resp.data {
                            println!("{}", t.langfuse_id);
                        }
                        return Ok(());
                    }

                    if resp.data.is_empty() {
                        println!(
                            "{}",
                            output::empty_hint("search results", "Try a different query.")
                        );
                        return Ok(());
                    }

                    println!(
                        "{} for \"{}\" ({}) {}\n",
                        "Search".bold(),
                        query,
                        resp.data.len(),
                        "(name filter fallback)".dimmed()
                    );
                    for t in &resp.data {
                        let name = t
                            .display_name
                            .as_deref()
                            .or(t.name.as_deref())
                            .unwrap_or("(unnamed)");
                        let time = output::relative_time(&t.timestamp);
                        println!(
                            "  {} {}  {}",
                            t.langfuse_id.dimmed(),
                            name.bold(),
                            time.dimmed()
                        );
                    }

                    if let Some(hint) =
                        output::pagination_hint(pagination.page, pagination.limit, resp.meta.total)
                    {
                        println!("\n  {}", hint.dimmed());
                    }
                }
                Err(e) => return Err(e),
            }
        }

        Commands::Tags { time } => {
            let mut params: Vec<(&str, Option<String>)> = vec![("project_id", pid)];
            time.push_date_params_inclusive_or_exit(&mut params);
            let path = DevPortalClient::build_path("/traces/names", &params);
            let names: Vec<String> = client.get(&path, CacheTtl::Short).await?;

            if cli.json {
                println!("{}", output::render_json(&names));
                return Ok(());
            }

            if names.is_empty() {
                println!(
                    "{}",
                    output::empty_hint("trace names", "No trace names found.")
                );
                return Ok(());
            }

            println!("{} ({})\n", "Trace Names".bold(), names.len());
            for name in &names {
                println!("  {}", name);
            }
        }

        Commands::Features {
            category,
            team,
            status,
        } => {
            let path = DevPortalClient::build_path(
                "/features",
                &[
                    ("project_id", pid),
                    ("category_id", category),
                    ("team", team),
                    ("status", status),
                ],
            );
            let resp: tb_lf::api::PaginatedResponse<Feature> =
                client.get(&path, CacheTtl::Short).await?;
            let features = resp.data;

            if cli.json {
                println!("{}", output::render_json(&features));
                return Ok(());
            }

            if features.is_empty() {
                println!("{}", output::empty_hint("features", "No features found."));
                return Ok(());
            }

            println!("{} ({})\n", "Features".bold(), features.len());
            for f in &features {
                let name = f.name.as_deref().unwrap_or("(unnamed)");
                let cat = f.category.as_ref().map(|c| c.name.as_str()).unwrap_or("");
                let status = f.status.as_deref().unwrap_or("");
                let teams = f
                    .teams
                    .as_ref()
                    .map(|t| {
                        t.iter()
                            .map(|r| r.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                let items = f.queue_item_count.unwrap_or(0);

                println!(
                    "  {} [{}] {}  {}  {} queue items",
                    name.bold(),
                    cat,
                    status.dimmed(),
                    teams.dimmed(),
                    items
                );
            }
        }

        Commands::FeatureItems { id } => {
            let path = DevPortalClient::build_path(
                &format!("/features/{}/queue_items", id),
                &[("project_id", pid)],
            );
            let items: Vec<QueueItem> = client.get(&path, CacheTtl::Short).await?;

            if cli.json {
                println!("{}", output::render_json(&items));
                return Ok(());
            }

            if items.is_empty() {
                println!(
                    "{}",
                    output::empty_hint("queue items for feature", "No items found.")
                );
                return Ok(());
            }

            println!(
                "{} #{} ({})\n",
                "Feature Queue Items".bold(),
                id,
                items.len()
            );
            for item in &items {
                let trace = item.trace_langfuse_id.as_deref().unwrap_or("—");
                let status = item.status.as_deref().unwrap_or("?");
                let cat = item.ai_category.as_deref().unwrap_or("");
                let conf = item.ai_confidence.as_deref().unwrap_or("");

                println!(
                    "  {} [{}] {} {}",
                    trace.dimmed(),
                    status,
                    cat,
                    conf.dimmed()
                );
            }
        }

        Commands::Prime { mcp } => {
            if mcp {
                // Minimal ~50 token output for hook injection
                let mut parts = vec![format!("tb-lf v{}", tb_lf::VERSION)];
                if let Ok(resp) = client
                    .get::<tb_lf::api::PaginatedResponse<Project>>("/projects", CacheTtl::Long)
                    .await
                {
                    let names: Vec<&str> = resp.data.iter().map(|p| p.name.as_str()).collect();
                    parts.push(format!("projects: {}", names.join(", ")));
                }
                println!("{}", parts.join(" | "));
                return Ok(());
            }

            println!("# tb-lf context\n");

            // Projects
            println!("## Projects\n");
            match client
                .get::<tb_lf::api::PaginatedResponse<Project>>("/projects", CacheTtl::Long)
                .await
            {
                Ok(resp) => {
                    for p in &resp.data {
                        println!("- {} (id: {})", p.name, p.id);
                    }
                }
                Err(e) => println!("(could not fetch projects: {})", e),
            }

            // Quick commands
            println!("\n## Quick commands\n");
            println!("- `tb-lf traces --limit 10` — recent traces");
            println!("- `tb-lf traces --triage flagged` — flagged traces");
            println!("- `tb-lf dashboard` — KPI overview");
            println!("- `tb-lf eval runs --limit 5` — recent eval runs");
            println!("- `tb-lf queue --status pending_review` — pending triage items");
            println!("- `tb-lf queue-update <id> --team <id>` — assign team to queue item");
            println!("- `tb-lf queue-bulk-update --filter-* --set-* --dry-run` — bulk update");
            println!("- `tb-lf teams` — list teams");
            println!("- `tb-lf search <query>` — search traces");
            println!("- `tb-lf trace <id> --project <p>` — full trace detail");

            // Interpreting metrics
            println!("\n## Interpreting metrics\n");
            println!("- Scores: >=0.80 good (green), >=0.50 ok (yellow), <0.50 bad (red)");
            println!("- Satisfaction: user thumbs up/down feedback");
            println!("- Triage: flagged=needs review, dismissed=noise, untouched=not yet triaged");
            println!("- Eval pass rate: >=0.90 healthy, <0.70 needs attention");

            toolbox_core::version_check::print_update_hint("tb-lf", env!("CARGO_PKG_VERSION"));
        }

        Commands::Human => {
            println!("{}", "tb-lf — DevPortal AI Insights CLI".bold());
            println!();
            println!("{}", "Setup".bold().underline());
            println!("  1. Add to secrets.toml:");
            println!("     [devportal]");
            println!("     url = \"https://your-devportal.example.com\"");
            println!("     token = \"your-bearer-token\"");
            println!("     project = \"production\"");
            println!("  2. Or: tb-lf config set url https://...");
            println!("  3. Verify: tb-lf doctor");
            println!();
            println!("{}", "Daily Use".bold().underline());
            println!("  tb-lf dashboard                    Overview KPIs");
            println!("  tb-lf traces --from 1d            Today's traces");
            println!("  tb-lf traces --triage flagged      Flagged traces");
            println!("  tb-lf daily-metrics --days 7       Weekly trends");
            println!("  tb-lf trace-metrics --from 7d      Trace scoring aggregates");
            println!("  tb-lf trace-metrics <id>           Per-trace scoring detail");
            println!();
            println!("{}", "Investigating Traces".bold().underline());
            println!("  tb-lf traces --name <agent>        Filter by name");
            println!("  tb-lf trace <id> --project <p>     Full trace detail");
            println!("  tb-lf trace <id> --observations    With observations");
            println!("  tb-lf scores --trace <id>          Scores for a trace");
            println!("  tb-lf search <query>               Search across traces");
            println!();
            println!("{}", "Eval Runs".bold().underline());
            println!("  tb-lf eval runs                    Recent eval runs");
            println!("  tb-lf eval run <id>                Run detail + items");
            println!("  tb-lf eval run <id> --failed       Failed items only");
            println!("  tb-lf eval revisions               Score trends by commit");
            println!("  tb-lf eval flaky                   Flaky test detection");
            println!();
            println!("{}", "Triage".bold().underline());
            println!("  tb-lf queue                        Pending queue items");
            println!("  tb-lf queue --status confirmed     Confirmed items");
            println!("  tb-lf queue --team 25              Filter by team");
            println!("  tb-lf queue --team unassigned      Unassigned items");
            println!("  tb-lf queue-stats                  Queue breakdown");
            println!("  tb-lf teams                        List teams");
            println!("  tb-lf triage-runs                  Recent triage runs");
            println!();
            println!("{}", "Queue Management".bold().underline());
            println!("  tb-lf queue-update 42 --team 25    Assign team");
            println!("  tb-lf queue-update 42 --no-team    Clear team");
            println!("  tb-lf queue-update 42 --status confirmed");
            println!("  tb-lf queue-bulk-update --filter-status pending_review --set-team 25");
            println!("  tb-lf queue-bulk-update ... --dry-run  Preview first");
            println!("  tb-lf queue-delete 42              Delete an item");
            println!();
            println!("{}", "Tips".bold().underline());
            println!("  --json        Machine-readable output (pipe to jq)");
            println!("  --no-cache    Bypass cache for fresh data");
            println!("  --project <p> Override default project");
            println!("  --from 7d    Relative time filter");
            println!("  --page 2      Paginate through results");
        }

        Commands::Explain { topic } => {
            let topics = [
                (
                    "entities",
                    "DevPortal tracks AI agent behavior through several entities:\n- Traces: A single agent invocation (user query → agent response)\n- Observations: Sub-steps within a trace (LLM calls, tool calls, spans)\n- Sessions: Groups of traces from the same user conversation\n- Scores: Numeric or categorical evaluations attached to traces\n- Comments: Human annotations on traces or observations",
                ),
                (
                    "relationships",
                    "Entity relationships:\n- A Session contains multiple Traces\n- A Trace contains multiple Observations and Scores\n- Queue Items reference Traces (triage results)\n- Eval Items reference Traces (eval run results)\n- Features group Queue Items by product feature",
                ),
                (
                    "traces",
                    "Traces represent a single AI agent invocation:\n- langfuse_id: Unique identifier from Langfuse\n- name: Agent/workflow name (e.g., \"customer-support-agent\")\n- cost_usd: Total LLM cost for this invocation\n- latency_ms: End-to-end duration\n- triage_status: flagged/dismissed/untouched\n- user_query: The user's input\n- agent_response: The agent's output",
                ),
                (
                    "scores",
                    "Scores are evaluations attached to traces:\n- value: Numeric score (0.0-1.0 typical)\n- source: EVAL (automated), API (programmatic), ANNOTATION (human)\n- Thresholds: >=0.80 good, >=0.50 ok, <0.50 bad\n- Common scores: correctness, helpfulness, safety, relevance",
                ),
                (
                    "observations",
                    "Observations are sub-steps within a trace:\n- Types: GENERATION (LLM call), SPAN (logical grouping), EVENT (point-in-time)\n- Track: model, tokens, cost, latency per step\n- Useful for debugging which step in an agent pipeline failed or was slow",
                ),
                (
                    "sessions",
                    "Sessions group traces from the same user conversation:\n- session_id: Identifier linking traces together\n- trace_count: Number of turns in the conversation\n- total_cost_usd: Aggregate cost across all traces\n- user_satisfied: Whether user gave positive feedback",
                ),
                (
                    "evaluations",
                    "Eval runs test agent behavior systematically:\n- Runs execute test suites against the agent\n- Items are individual test cases with pass/fail/score\n- Revisions track score trends across git commits\n- Coverage shows which suites/cases exist and their reliability\n- Flaky detection identifies inconsistent test cases",
                ),
                (
                    "triage",
                    "Triage automates trace review:\n- Triage runs scan recent traces using AI classification\n- Queue items are the results: flagged or dismissed\n- Categories: bug, feature_request, unknown\n- Confidence: high, medium, low\n- Status: pending_review → confirmed/dismissed by human",
                ),
                (
                    "features",
                    "Features group related queue items:\n- Track which product features generate user feedback\n- Categories and teams help route items\n- Queue item count shows volume per feature",
                ),
            ];

            if cli.json {
                let map: std::collections::HashMap<&str, &str> = topics.iter().copied().collect();
                if let Some(t) = &topic {
                    if let Some(content) = map.get(t.as_str()) {
                        println!("{}", serde_json::json!({"topic": t, "content": content}));
                    } else {
                        println!(
                            "{}",
                            serde_json::json!({"error": "unknown topic", "available": topics.iter().map(|(k,_)| k).collect::<Vec<_>>()})
                        );
                    }
                } else {
                    println!("{}", serde_json::to_string_pretty(&map).unwrap());
                }
                return Ok(());
            }

            if let Some(t) = &topic {
                if let Some((_, content)) = topics.iter().find(|(k, _)| *k == t.as_str()) {
                    println!("{}\n", t.bold().underline());
                    println!("{}", content);
                } else {
                    println!("Unknown topic '{}'. Available topics:", t);
                    for (k, _) in &topics {
                        println!("  {}", k);
                    }
                }
            } else {
                println!("{}\n", "Available topics".bold());
                for (k, _) in &topics {
                    println!("  {}", k);
                }
                println!(
                    "\n  {}",
                    "Run `tb-lf explain <topic>` for details.".dimmed()
                );
            }
        }

        Commands::Doctor => {
            println!("{}\n", "Doctor".bold());

            // Config
            println!("  {:<10} {}", "Config:", "OK".green());
            println!("    URL:   {}", config.url);
            println!("    Token: {}", config.masked_token());
            if let Some(ref p) = config.project {
                println!("    Project: {}", p);
            }

            // API connectivity
            print!("  {:<10} ", "API:");
            let test_path = DevPortalClient::build_path(
                "/dashboard",
                &[("project_id", project_id.map(|id| id.to_string()))],
            );
            match client
                .get::<serde_json::Value>(&test_path, CacheTtl::Short)
                .await
            {
                Ok(_) => println!("{}", "OK".green()),
                Err(e) => println!("{} — {}", "FAIL".red(), e),
            }

            // Cache stats
            let (count, bytes) = client.cache().size();
            let size_str = if bytes > 1_048_576 {
                format!("{:.1} MB", bytes as f64 / 1_048_576.0)
            } else if bytes > 1024 {
                format!("{:.1} KB", bytes as f64 / 1024.0)
            } else {
                format!("{} B", bytes)
            };
            println!("  {:<10} {} files, {}", "Cache:", count, size_str);
            toolbox_core::version_check::print_update_hint("tb-lf", env!("CARGO_PKG_VERSION"));
        }

        Commands::Flags => {
            let path = DevPortalClient::build_path("/flags", &[("project_id", pid)]);
            let resp: serde_json::Value = client.get(&path, CacheTtl::Short).await?;
            let flags: Vec<FlagInfo> =
                serde_json::from_value(resp["data"].clone()).unwrap_or_default();

            if cli.json {
                println!("{}", output::render_json(&flags));
                return Ok(());
            }

            if flags.is_empty() {
                println!(
                    "{}",
                    output::empty_hint(
                        "flags",
                        "No flags found. Flags are extracted during trace sync."
                    )
                );
                return Ok(());
            }

            println!("{}\n", "Flags".bold());
            for f in &flags {
                let first = f
                    .first_seen
                    .as_deref()
                    .and_then(|s| s.get(..10))
                    .unwrap_or("?");
                let last = f
                    .last_seen
                    .as_deref()
                    .and_then(|s| s.get(..10))
                    .unwrap_or("?");
                println!(
                    "  {:<45} {:>6} traces  {} → {}",
                    f.flag_name.cyan(),
                    f.trace_count,
                    first.dimmed(),
                    last.dimmed()
                );
            }
            println!("\n  {} flags total.\n", flags.len());
        }

        Commands::FlagCohort {
            flag_name,
            name,
            env,
            time,
        } => {
            let mut params: Vec<(&str, Option<String>)> = vec![
                ("project_id", pid),
                ("flag_name", Some(flag_name.clone())),
                ("name", name),
                ("environment", env),
            ];
            time.push_date_params_inclusive_or_exit(&mut params);
            let path = DevPortalClient::build_path("/traces/flag_stats", &params);
            let resp: FlagStatsResponse = client.get(&path, CacheTtl::Short).await?;

            if cli.json {
                println!("{}", output::render_json(&resp));
                return Ok(());
            }

            println!("{} {}\n", "Flag Cohort:".bold(), flag_name.cyan());
            println!("  Period: {} → {}", resp.from, resp.to);
            println!();

            fn print_cohort(label: &str, s: &CohortStats) {
                println!("  {} ({} traces)", label.bold(), s.trace_count);
                if let Some(ref c) = s.cost {
                    println!(
                        "    Cost:    total {} | avg {} | p50 {} | p95 {}",
                        output::fmt_cost(c.total),
                        output::fmt_cost(c.avg),
                        output::fmt_cost(c.p_50),
                        output::fmt_cost(c.p_95)
                    );
                }
                if let Some(ref l) = s.latency_ms {
                    println!(
                        "    Latency: avg {:.0}ms | p50 {:.0}ms | p95 {:.0}ms",
                        l.avg, l.p_50, l.p_95
                    );
                }
                if let Some(errors) = s.errors {
                    println!("    Errors:  {}", errors);
                }
                if let Some(ref t) = s.tokens {
                    println!("    Tokens:  {} input | {} output", t.input, t.output);
                }
                if let Some(ref tools) = s.tool_calls
                    && let Some(map) = tools.as_object()
                    && !map.is_empty()
                {
                    let top: Vec<_> = map
                        .iter()
                        .map(|(k, v)| (k.as_str(), v.as_u64().unwrap_or(0)))
                        .collect();
                    let tool_str: Vec<_> =
                        top.iter().map(|(k, v)| format!("{}:{}", k, v)).collect();
                    println!("    Tools:   {}", tool_str.join(", "));
                }
            }

            print_cohort("ON ", &resp.on);
            println!();
            print_cohort("OFF", &resp.off);

            if resp.on.trace_count > 0 && resp.off.trace_count > 0 {
                let total = resp.on.trace_count + resp.off.trace_count;
                let pct = (resp.on.trace_count as f64 / total as f64 * 100.0).round();
                println!(
                    "\n  {} of traces have flag ON ({}/{}).",
                    format!("{:.0}%", pct).yellow(),
                    resp.on.trace_count,
                    total
                );
                if resp.on.trace_count < 10 || resp.off.trace_count < 10 {
                    println!(
                        "  {}",
                        "⚠ Small cohort — results may not be meaningful.".yellow()
                    );
                }
            }
            println!();
        }

        Commands::FlagTraces {
            flag_name,
            value,
            name,
            env,
            time,
            pagination,
        } => {
            let mut params: Vec<(&str, Option<String>)> = vec![
                ("project_id", pid),
                ("flag_name", Some(flag_name)),
                ("flag_value", Some(value)),
                ("name", name),
                ("environment", env),
            ];
            time.push_date_params_inclusive_or_exit(&mut params);
            pagination.push_params(&mut params);
            let path = DevPortalClient::build_path("/traces", &params);
            let resp: PaginatedResponse<Trace> = client.get(&path, CacheTtl::Short).await?;

            if cli.json {
                println!("{}", output::render_json(&resp.data));
                return Ok(());
            }

            if resp.data.is_empty() {
                println!(
                    "{}",
                    output::empty_hint(
                        "flag traces",
                        "No traces found for this flag/value combination."
                    )
                );
                return Ok(());
            }

            println!("{}\n", "Flag Traces".bold());
            for t in &resp.data {
                let name = t
                    .display_name
                    .as_deref()
                    .or(t.name.as_deref())
                    .unwrap_or("(unnamed)");
                let cost = t.cost_usd.map(output::fmt_cost).unwrap_or_default();
                let latency = t
                    .latency_ms
                    .map(|ms| format!("{:.0}ms", ms))
                    .unwrap_or_default();
                let age = output::relative_time(&t.timestamp);
                println!(
                    "  {} {} {} {}  {}",
                    t.langfuse_id.dimmed(),
                    name.cyan(),
                    cost,
                    latency,
                    age.dimmed()
                );
            }
            if let Some(hint) =
                output::pagination_hint(pagination.page, pagination.limit, resp.meta.total)
            {
                println!("\n  {}", hint.dimmed());
            }
        }

        Commands::Config { .. } | Commands::Skill { .. } => {} // handled before client construction
    }

    Ok(())
}

fn build_lf_project_options(projects: &[tb_lf::types::Project]) -> Vec<String> {
    projects
        .iter()
        .map(|p| format!("{} (id: {})", p.name, p.id))
        .collect()
}

fn resolve_lf_project_name(selected: &str, projects: &[tb_lf::types::Project]) -> String {
    projects
        .iter()
        .find(|p| selected == format!("{} (id: {})", p.name, p.id))
        .map(|p| p.name.clone())
        .unwrap_or_else(|| selected.to_string())
}

fn find_lf_project_starting_cursor(
    existing_project: Option<&str>,
    projects: &[tb_lf::types::Project],
) -> usize {
    existing_project
        .and_then(|name| {
            projects
                .iter()
                .position(|p| p.name.eq_ignore_ascii_case(name))
        })
        .unwrap_or(0)
}

async fn handle_config(action: Option<&ConfigAction>) -> tb_lf::error::Result<()> {
    use toolbox_core::prompt::PromptResult;

    match action {
        Some(ConfigAction::Init {
            url,
            token,
            project,
        }) => {
            let existing = Config::load().ok();

            // Resolve URL
            let default_url = existing
                .as_ref()
                .map(|c| c.url.as_str())
                .unwrap_or("https://devportal.productive.io/");
            let url = match toolbox_core::prompt::prompt_text(
                "DevPortal URL:",
                url.as_deref(),
                default_url,
            ) {
                Ok(PromptResult::Ok(u)) => u.trim_end_matches('/').to_string(),
                Ok(PromptResult::Cancelled) => {
                    println!("Cancelled.");
                    return Ok(());
                }
                Err(e) => return Err(tb_lf::error::TbLfError::Config(e)),
            };

            // Resolve token
            let token = match toolbox_core::prompt::prompt_token(
                "API token:",
                token.as_deref(),
                existing.as_ref().map(|c| c.token.as_str()),
            ) {
                Ok(PromptResult::Ok(t)) => t,
                Ok(PromptResult::Cancelled) => {
                    println!("Cancelled.");
                    return Ok(());
                }
                Err(e) => return Err(tb_lf::error::TbLfError::Config(e)),
            };

            // Validate by making a test API call
            let config = tb_lf::config::Config {
                url: url.clone(),
                token: token.clone(),
                project: None,
            };
            let client = tb_lf::api::DevPortalClient::new(&config, true)?;
            let resp: tb_lf::api::PaginatedResponse<tb_lf::types::Project> = client
                .get("/projects", tb_lf::cache::CacheTtl::Short)
                .await?;
            println!("Connected! Found {} projects.", resp.data.len());

            // Resolve project
            let project = if let Some(p) = project {
                // Non-interactive: resolve from flag
                let matched = resp
                    .data
                    .iter()
                    .find(|proj| proj.name.eq_ignore_ascii_case(p) || proj.id.to_string() == *p);
                match matched {
                    Some(proj) => {
                        println!("Default project: {} (id: {})", proj.name, proj.id);
                        Some(proj.name.clone())
                    }
                    None => {
                        eprintln!("Warning: project '{}' not found, skipping", p);
                        None
                    }
                }
            } else if !resp.data.is_empty() {
                let options = build_lf_project_options(&resp.data);
                let starting = find_lf_project_starting_cursor(
                    existing.as_ref().and_then(|c| c.project.as_deref()),
                    &resp.data,
                );

                match toolbox_core::prompt::prompt_select("Default project:", options, starting) {
                    Ok(PromptResult::Ok(selected)) => {
                        Some(resolve_lf_project_name(&selected, &resp.data))
                    }
                    Ok(PromptResult::Cancelled) => {
                        println!("Cancelled.");
                        return Ok(());
                    }
                    Err(e) => return Err(tb_lf::error::TbLfError::Config(e)),
                }
            } else {
                None
            };

            let config = tb_lf::config::Config {
                url,
                token,
                project,
            };
            toolbox_core::config::save_config(&tb_lf::config::Config::config_path()?, &config)?;
            println!(
                "Config saved to {}",
                tb_lf::config::Config::config_path()?.display()
            );

            if let Some(ref p) = config.project {
                println!("Default project: {}", p);
            }
        }
        None | Some(ConfigAction::Show) => {
            let config = Config::load()?;
            println!("{}", "DevPortal Configuration".bold());
            println!("  URL:     {}", config.url);
            println!("  Token:   {}", config.masked_token());
            println!(
                "  Project: {}",
                config.project.as_deref().unwrap_or("(none)")
            );
        }
        Some(ConfigAction::Set { key, value }) => {
            // Interactive project selection when key=project and no value
            if key == "project" && value.is_none() {
                let config = Config::load()?;
                let client = tb_lf::api::DevPortalClient::new(&config, true)?;
                let resp: tb_lf::api::PaginatedResponse<tb_lf::types::Project> = client
                    .get("/projects", tb_lf::cache::CacheTtl::Short)
                    .await?;

                if resp.data.is_empty() {
                    return Err(tb_lf::error::TbLfError::Config("No projects found".into()));
                }

                let options = build_lf_project_options(&resp.data);
                let starting =
                    find_lf_project_starting_cursor(config.project.as_deref(), &resp.data);

                let project_name = match toolbox_core::prompt::prompt_select(
                    "Default project:",
                    options,
                    starting,
                ) {
                    Ok(PromptResult::Ok(selected)) => {
                        resolve_lf_project_name(&selected, &resp.data)
                    }
                    Ok(PromptResult::Cancelled) => {
                        println!("Cancelled.");
                        return Ok(());
                    }
                    Err(e) => return Err(tb_lf::error::TbLfError::Config(e)),
                };

                let path = Config::config_path()?;
                toolbox_core::config::patch_toml(&path, "project", &project_name)?;
                println!("Set {} = {}", "project".bold(), project_name);
                return Ok(());
            }

            // Scalar set (requires value)
            let value = value.as_ref().ok_or_else(|| {
                tb_lf::error::TbLfError::Config(format!("Value is required for key '{}'", key))
            })?;

            match key.as_str() {
                "url" | "token" | "project" => {}
                _ => {
                    return Err(tb_lf::error::TbLfError::Config(format!(
                        "Unknown config key '{}'. Valid keys: url, token, project",
                        key
                    )));
                }
            }

            let path = Config::config_path()?;
            toolbox_core::config::patch_toml(&path, key, value)?;
            println!("Set {} = {}", key.bold(), value);
        }
    }
    Ok(())
}
