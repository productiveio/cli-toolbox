use clap::Parser;
use colored::Colorize;

use tb_fleet::commands;
use tb_fleet::discovery::Backend;
use tb_fleet::watch;

#[derive(Parser)]
#[command(
    name = "tb-fleet",
    version,
    about = "Manage the Claude Code sessions running on this machine"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum BackendArg {
    Iterm,
    Tmux,
}

impl From<BackendArg> for Backend {
    fn from(b: BackendArg) -> Self {
        match b {
            BackendArg::Iterm => Backend::Iterm,
            BackendArg::Tmux => Backend::Tmux,
        }
    }
}

#[derive(clap::Subcommand)]
enum Commands {
    /// List the live Claude sessions (default)
    List {
        /// Machine-readable output
        #[arg(long)]
        json: bool,
    },

    /// Read what a session is currently showing
    Peek {
        /// sessionId prefix, derived name (e.g. work-f9), or pid
        target: String,
        /// How many trailing lines to show
        #[arg(long, default_value_t = 40)]
        lines: usize,
    },

    /// Type text into a session and submit it
    Send {
        /// sessionId prefix, derived name, or pid
        target: String,
        /// The message to send
        text: String,
    },

    /// Spawn a new session in a fresh tab/pane
    Spawn {
        /// Initial prompt (optional)
        prompt: Option<String>,
        /// Working directory (defaults to the current directory)
        #[arg(long)]
        dir: Option<String>,
        /// Backend to spawn into (defaults to iterm, or tmux when inside tmux)
        #[arg(long, value_enum)]
        backend: Option<BackendArg>,
        /// tmux session name (tmux backend only)
        #[arg(long)]
        name: Option<String>,
        /// Open a new window instead of a tab (iterm backend only)
        #[arg(long)]
        window: bool,
    },

    /// Hand the current work off to a fresh session in another window
    Handoff {
        /// The brief for the new session (or use --file / stdin)
        brief: Option<String>,
        /// Read the brief from a file ("-" for stdin)
        #[arg(long)]
        file: Option<String>,
        /// Working directory for the new session (defaults to the current directory)
        #[arg(long)]
        dir: Option<String>,
        /// Backend to hand off into (defaults to iterm, or tmux when inside tmux)
        #[arg(long, value_enum)]
        backend: Option<BackendArg>,
        /// tmux session name (tmux backend only)
        #[arg(long)]
        name: Option<String>,
        /// Open a tab instead of a new window (iterm backend only)
        #[arg(long)]
        tab: bool,
        /// Return immediately instead of waiting for the new session to register
        #[arg(long)]
        no_wait: bool,
    },

    /// Live dashboard + macOS notifications on finished/stuck sessions
    Watch {
        /// Poll interval in seconds
        #[arg(long, default_value_t = 5)]
        interval: u64,
        /// Seconds a session must sit idle-on-a-prompt before it counts as stuck
        #[arg(long, default_value_t = 300)]
        stuck: i64,
        /// Notifications only, no TUI (backgroundable)
        #[arg(long)]
        quiet: bool,
    },

    /// Manage the Claude Code skill file
    Skill {
        #[command(subcommand)]
        action: toolbox_core::skill::SkillAction,
    },
}

fn main() {
    let cli = Cli::parse();

    // `skill` needs no session state — handle it first so it works from anywhere.
    if let Some(Commands::Skill { action }) = &cli.command {
        let skill = toolbox_core::skill::SkillConfig {
            tool_name: "tb-fleet",
            content: include_str!("../SKILL.md"),
        };
        if let Err(e) = toolbox_core::skill::run(&skill, action) {
            eprintln!("{} {}", "Error:".red().bold(), e);
            std::process::exit(1);
        }
        return;
    }

    let result = match cli.command.unwrap_or(Commands::List { json: false }) {
        Commands::List { json } => commands::list(json),
        Commands::Peek { target, lines } => commands::peek(&target, lines),
        Commands::Send { target, text } => commands::send(&target, &text),
        Commands::Spawn {
            prompt,
            dir,
            backend,
            name,
            window,
        } => commands::spawn(prompt, dir, backend.map(Into::into), name, window),
        Commands::Handoff {
            brief,
            file,
            dir,
            backend,
            name,
            tab,
            no_wait,
        } => commands::handoff(
            brief,
            file,
            dir,
            backend.map(Into::into),
            name,
            tab,
            !no_wait,
        ),
        Commands::Watch {
            interval,
            stuck,
            quiet,
        } => watch::run(interval, stuck, quiet),
        Commands::Skill { .. } => unreachable!("handled above"),
    };

    if let Err(e) = result {
        eprintln!("{} {}", "Error:".red().bold(), e);
        std::process::exit(1);
    }
}
