//! One function per CLI verb. `watch` lives in its own module.

use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use colored::Colorize;
use serde::Deserialize;

use crate::backend::{self, Prompt};
use crate::discovery::{self, Backend, Session, claude_home};
use crate::error::{Error, Result};
use crate::render::{home_rel, plain_table, tail};

#[derive(Deserialize)]
struct FleetConfig {
    /// The command that launches Claude Code (defaults to `claude`).
    command: Option<String>,
}

/// Resolve the command used to launch Claude in a spawned session. Hosts differ:
/// some invoke `claude`, others a wrapper/alias like `cc`. Precedence:
/// `TB_FLEET_CMD` env var → `command` in the tool's config.toml → `claude`.
fn resolve_launcher() -> String {
    if let Ok(cmd) = std::env::var("TB_FLEET_CMD") {
        let cmd = cmd.trim();
        if !cmd.is_empty() {
            return cmd.to_string();
        }
    }
    if let Ok(path) = toolbox_core::config::config_path("tb-fleet")
        && let Ok(Some(cfg)) = toolbox_core::config::load_standalone::<FleetConfig>(&path)
        && let Some(cmd) = cfg.command
        && !cmd.trim().is_empty()
    {
        return cmd.trim().to_string();
    }
    "claude".to_string()
}

fn term_width() -> usize {
    crossterm::terminal::size()
        .map(|(c, _)| c as usize)
        .unwrap_or(120)
}

pub fn list(json: bool) -> Result<()> {
    let rows = discovery::discover();
    if json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
    } else {
        println!("{}\n", plain_table(&rows));
    }
    Ok(())
}

pub fn peek(target: &str, lines: usize) -> Result<()> {
    let s = discovery::resolve(target).map_err(Error::Other)?;
    let text = backend::peek(&s)?;
    let where_ = s.cwd.as_deref().map(home_rel).unwrap_or_default();
    println!(
        "\n{} {} ({}, {}) — {}",
        "▼".cyan(),
        s.label().bold(),
        s.backend.label(),
        s.status,
        where_.dimmed()
    );
    println!("{}", "─".repeat(74));
    println!("{}", tail(&text, lines, term_width().saturating_sub(2)));
    println!("{}\n", "─".repeat(74));
    Ok(())
}

pub fn send(target: &str, text: &str) -> Result<()> {
    let s = discovery::resolve(target).map_err(Error::Other)?;
    backend::send(&s, text)?;
    println!(
        "sent to {} ({}): {}",
        s.label().bold(),
        s.backend.label(),
        text
    );
    Ok(())
}

/// Rename a session by driving Claude's own `/rename` — the registry (and so
/// every fleet view) picks the new name up on its next status write.
pub fn rename(target: &str, name: &str) -> Result<()> {
    let s = discovery::resolve(target).map_err(Error::Other)?;
    let name = clean_name(name)?;
    let was = s.label();
    backend::send(&s, &format!("/rename {name}"))?;

    // Confirm from the registry rather than trusting the keystrokes landed.
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(500));
        if let Some(now) = discovery::discover().iter().find(|r| r.key() == s.key())
            && now.name.as_deref() == Some(name.as_str())
        {
            println!("{} → {}", was.dimmed(), name.bold());
            return Ok(());
        }
    }
    println!(
        "sent `/rename {name}` to {} — not reflected yet, check `tb-fleet list`",
        was.bold()
    );
    Ok(())
}

/// A session name has to survive being typed into a TUI prompt as one line.
fn clean_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Other("empty name".into()));
    }
    if name.contains(['\n', '\r']) {
        return Err(Error::Other("name must be a single line".into()));
    }
    Ok(name.to_string())
}

/// Everything `spawn` and `handoff` share about *where* the new session lands.
#[derive(Default)]
pub struct SpawnOpts {
    pub dir: Option<String>,
    pub backend: Option<Backend>,
    /// Claude display name for the new session (`claude -n`).
    pub name: Option<String>,
    pub tmux_session: Option<String>,
    pub window: bool,
}

impl SpawnOpts {
    /// Fill in the defaults that need the environment: cwd, and the backend we're
    /// sitting in. Also validates, so a bad dir/name fails before anything opens.
    fn resolve(&self) -> Result<(String, Backend, Option<String>)> {
        let dir = self.dir.clone().unwrap_or_else(|| {
            std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| ".".into())
        });
        if !Path::new(&dir).exists() {
            return Err(Error::Other(format!("dir does not exist: {dir}")));
        }
        let backend = self.backend.unwrap_or_else(|| {
            if std::env::var_os("TMUX").is_some() {
                Backend::Tmux
            } else {
                Backend::Iterm
            }
        });
        let name = self.name.as_deref().map(clean_name).transpose()?;
        Ok((dir, backend, name))
    }
}

pub fn spawn(prompt: Option<String>, opts: SpawnOpts) -> Result<()> {
    let (dir, backend_kind, name) = opts.resolve()?;
    let prompt = prompt.unwrap_or_default();
    let launcher = resolve_launcher();
    let desc = backend::spawn(
        backend_kind,
        &dir,
        Prompt::Inline(&prompt),
        name.as_deref(),
        opts.tmux_session.as_deref(),
        opts.window,
        &launcher,
    )?;
    if prompt.is_empty() {
        println!("{desc} in {}", home_rel(&dir));
    } else {
        println!("{desc} in {} — \"{prompt}\"", home_rel(&dir));
    }
    Ok(())
}

// --- handoff -----------------------------------------------------------------

/// Where handoff briefs are kept. They outlive the spawn on purpose: the record
/// of what was handed off, and a file the new session can re-read at any point.
fn handoff_dir() -> PathBuf {
    claude_home().join("fleet-handoffs")
}

/// A filename-safe stem from the brief's first line.
fn slug(text: &str) -> String {
    let first = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let mut out = String::new();
    for c in first.trim_start_matches(['#', ' ']).chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
        if out.len() >= 40 {
            break;
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "handoff".into()
    } else {
        out
    }
}

/// The text the receiving session wakes up to: a line of provenance, then the brief.
fn compose(brief: &str, dir: &str, from: Option<&Session>, stamp: &str) -> String {
    let origin = match from {
        Some(s) => format!(
            "another Claude session ({}, in {})",
            s.label(),
            s.cwd.as_deref().map(home_rel).unwrap_or_else(|| "?".into())
        ),
        None => "another Claude session".to_string(),
    };
    let reply = from
        .map(|s| {
            format!(
                "\nWhen you're done (or blocked), report back with: `tb-fleet send {} \"<your update>\"`.\n",
                s.label()
            )
        })
        .unwrap_or_default();
    format!(
        "You're picking up work handed off from {origin} at {stamp}. \
You're running in {}. Work autonomously within the brief below — where it sets a \
scope or a constraint, that wins over your defaults.\n{reply}\n--- Brief ---\n\n{}\n",
        home_rel(dir),
        brief.trim()
    )
}

/// Read the brief from an explicit argument, a file (`-` = stdin), or piped stdin.
fn read_brief(brief: Option<String>, file: Option<String>) -> Result<String> {
    let text = match (brief, file) {
        (Some(b), _) => b,
        (None, Some(f)) if f == "-" => read_stdin()?,
        (None, Some(f)) => std::fs::read_to_string(&f)
            .map_err(|e| Error::Other(format!("cannot read brief from {f}: {e}")))?,
        (None, None) => read_stdin()?,
    };
    if text.trim().is_empty() {
        return Err(Error::Other(
            "empty brief — pass it as an argument, via --file, or on stdin".into(),
        ));
    }
    Ok(text)
}

fn read_stdin() -> Result<String> {
    let mut buf = String::new();
    std::io::stdin().read_to_string(&mut buf)?;
    Ok(buf)
}

/// Same directory, whether or not symlinks and trailing slashes agree.
fn same_dir(a: &str, b: &str) -> bool {
    let norm = |p: &str| {
        std::fs::canonicalize(p)
            .unwrap_or_else(|_| PathBuf::from(p))
            .display()
            .to_string()
    };
    norm(a) == norm(b)
}

/// Poll the registry for the session that just appeared in `dir`. Claude takes a
/// few seconds to register, so this is worth waiting on — it gives the caller a
/// name to `peek`/`send` with instead of "go look for it".
fn await_new_session(before: &HashSet<String>, dir: &str, timeout: Duration) -> Option<Session> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(700));
        if let Some(s) = discovery::discover().into_iter().find(|s| {
            !before.contains(&s.key()) && s.cwd.as_deref().is_some_and(|c| same_dir(c, dir))
        }) {
            return Some(s);
        }
    }
    None
}

pub fn handoff(
    brief: Option<String>,
    file: Option<String>,
    opts: SpawnOpts,
    wait: bool,
) -> Result<()> {
    let brief = read_brief(brief, file)?;
    let (dir, backend_kind, name) = opts.resolve()?;

    let from = discovery::origin();
    let now = chrono::Local::now();
    let text = compose(
        &brief,
        &dir,
        from.as_ref(),
        &now.format("%Y-%m-%d %H:%M").to_string(),
    );

    let path = handoff_dir().join(format!(
        "{}-{}.md",
        now.format("%Y%m%d-%H%M%S"),
        slug(&brief)
    ));
    std::fs::create_dir_all(handoff_dir())?;
    std::fs::write(&path, &text)?;

    let before: HashSet<String> = discovery::discover().iter().map(Session::key).collect();
    let launcher = resolve_launcher();
    let desc = backend::spawn(
        backend_kind,
        &dir,
        Prompt::File(&path.display().to_string()),
        name.as_deref(),
        opts.tmux_session.as_deref(),
        opts.window,
        &launcher,
    )?;
    println!(
        "{} {desc} in {} — brief: {}",
        "→".cyan(),
        home_rel(&dir),
        home_rel(&path.display().to_string()).dimmed()
    );

    if wait {
        // Returns as soon as the session appears; the ceiling only bites when the
        // spawn failed. A cold Claude boot behind a profile that loads secrets is
        // comfortably past 30s, so don't set this tight.
        match await_new_session(&before, &dir, Duration::from_secs(75)) {
            Some(s) => println!(
                "  picked up as {} — steer it with `tb-fleet send {} \"…\"`",
                s.label().bold(),
                s.label()
            ),
            None => println!("  (not registered yet — `tb-fleet list` in a moment)"),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_come_from_the_first_meaningful_line() {
        assert_eq!(
            slug("# Fix the flaky login test\n\nmore"),
            "fix-the-flaky-login-test"
        );
        assert_eq!(
            slug("\n\nCDC ingestion: scale it"),
            "cdc-ingestion-scale-it"
        );
        assert_eq!(slug("!!! ???"), "handoff");
        assert!(slug(&"word ".repeat(50)).len() <= 40);
    }

    #[test]
    fn brief_carries_provenance_and_the_reply_path() {
        let text = compose("Do the thing.", "/tmp", None, "2026-08-17 10:00");
        assert!(text.contains("handed off from another Claude session"));
        assert!(text.contains("Do the thing."));
        // No known origin -> nothing to report back to.
        assert!(!text.contains("tb-fleet send"));
    }

    #[test]
    fn known_origin_gets_a_report_back_instruction() {
        let from = Session {
            pid: 1,
            session_id: Some("abcdef123".into()),
            name: Some("work-f9".into()),
            cwd: Some("/tmp".into()),
            status: "busy".into(),
            updated_at: None,
            tty: None,
            backend: Backend::Iterm,
            handle: None,
            tab: None,
            title: None,
        };
        let text = compose("Do it.", "/tmp", Some(&from), "2026-08-17 10:00");
        assert!(text.contains("(work-f9, in /tmp)"));
        assert!(text.contains("tb-fleet send work-f9"));
    }

    #[test]
    fn brief_sources_are_ordered_and_validated() {
        assert_eq!(
            read_brief(Some("inline".into()), Some("/nope".into())).unwrap(),
            "inline"
        );
        assert!(read_brief(Some("   ".into()), None).is_err());
        assert!(read_brief(None, Some("/definitely/not/here.md".into())).is_err());
    }
}
