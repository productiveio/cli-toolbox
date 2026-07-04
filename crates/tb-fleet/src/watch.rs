//! `watch` — live supervision. Polls the registry, detects transitions, fires
//! macOS notifications on finished/stuck, and (on a TTY) renders a ratatui dashboard.

use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use serde::{Deserialize, Serialize};

use crate::backend;
use crate::discovery::{Backend, Session, claude_home, discover};
use crate::error::{Error, Result};
use crate::notify::notify;
use crate::render::{ago, home_rel};

const EVENT_CAP: usize = 20;

/// A session sitting idle on a permission/confirmation dialog is blocked on you.
fn awaiting(text: &str) -> bool {
    const MARKERS: [&str; 5] = [
        "Do you want to proceed",
        "Would you like to",
        "❯ 1. Yes",
        "❯ 1. Allow",
        "1. Yes,",
    ];
    MARKERS.iter().any(|m| text.contains(m))
}

#[derive(Default, Clone, Serialize, Deserialize)]
struct Prev {
    status: String,
    #[serde(default)]
    stuck_notified: bool,
}

type State = HashMap<String, Prev>;

fn state_path() -> PathBuf {
    claude_home().join("fleet-watch-state.json")
}
fn load_state() -> State {
    std::fs::read_to_string(state_path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}
fn save_state(s: &State) {
    if let Ok(t) = serde_json::to_string(s) {
        let _ = std::fs::write(state_path(), t);
    }
}

struct Ev {
    icon: &'static str,
    time: String,
    msg: String,
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// One supervision pass: update `state`, fire notifications, return (rows, new events).
fn tick(state: &mut State, stuck_secs: i64) -> (Vec<Session>, Vec<Ev>) {
    let rows = discover();
    let mut evs = Vec::new();
    let time = chrono::Local::now().format("%H:%M:%S").to_string();
    let mut push = |icon: &'static str, msg: String| {
        evs.push(Ev {
            icon,
            time: time.clone(),
            msg,
        })
    };

    let mut seen = std::collections::HashSet::new();
    for r in &rows {
        let key = r.key();
        seen.insert(key.clone());
        let prev = state.get(&key).cloned().unwrap_or_default();
        let label = r.label();
        let known = state.contains_key(&key);

        if prev.status == "busy" && r.status == "idle" {
            let t = r.title.as_deref().unwrap_or("");
            push(
                "✅",
                format!(
                    "{label} finished — {}",
                    t.chars().take(40).collect::<String>()
                ),
            );
            notify(
                "Session finished",
                &format!("{label}: {}", t.chars().take(60).collect::<String>()),
            );
        } else if known && prev.status != "busy" && r.status == "busy" {
            push("▶", format!("{label} working"));
        } else if !known {
            push("＋", format!("{label} appeared"));
        }

        // Stuck: idle past the threshold and blocked on a prompt — check once per idle episode.
        let idle_secs = r.updated_at.map(|u| (now_ms() - u) / 1000).unwrap_or(0);
        let mut stuck_notified = prev.stuck_notified && r.status == "idle";
        if r.status == "idle" && idle_secs > stuck_secs && !stuck_notified {
            let screen = backend::peek(r).unwrap_or_default();
            if awaiting(&screen) {
                push(
                    "⚠",
                    format!("{label} waiting for your input ({})", ago(r.updated_at)),
                );
                notify(
                    "Session waiting",
                    &format!("{label} is blocked on a prompt"),
                );
            }
            stuck_notified = true;
        }
        state.insert(
            key,
            Prev {
                status: r.status.clone(),
                stuck_notified: if r.status == "busy" {
                    false
                } else {
                    stuck_notified
                },
            },
        );
    }
    for key in state.keys().cloned().collect::<Vec<_>>() {
        if !seen.contains(&key) {
            push("✕", format!("{} closed", &key[..key.len().min(8)]));
            state.remove(&key);
        }
    }
    save_state(state);
    (rows, evs)
}

pub fn run(interval_secs: u64, stuck_secs: i64, quiet: bool) -> Result<()> {
    let tui = std::io::IsTerminal::is_terminal(&io::stdout()) && !quiet;
    if tui {
        run_tui(interval_secs, stuck_secs)
    } else {
        run_quiet(interval_secs, stuck_secs)
    }
}

fn run_quiet(interval_secs: u64, stuck_secs: i64) -> Result<()> {
    println!("fleet watch running (notify-only, every {interval_secs}s). Ctrl-c to stop.");
    let mut state = load_state();
    loop {
        let (_, evs) = tick(&mut state, stuck_secs);
        for e in evs {
            println!("{} {}  {}", e.icon, e.time, e.msg);
        }
        std::thread::sleep(Duration::from_secs(interval_secs));
    }
}

/// Restores the terminal on drop, so an early `?` return can't leave it wedged.
struct TermGuard;
impl Drop for TermGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

fn run_tui(interval_secs: u64, stuck_secs: i64) -> Result<()> {
    enable_raw_mode().map_err(Error::Io)?;
    execute!(io::stdout(), EnterAlternateScreen).map_err(Error::Io)?;
    let _guard = TermGuard;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout())).map_err(Error::Io)?;

    let mut state = load_state();
    let mut log: Vec<Ev> = Vec::new();
    let mut rows: Vec<Session> = Vec::new();
    let mut last_poll: Option<Instant> = None;
    // Track selection by session key, not index, so it stays put as rows re-sort.
    let mut selected: Option<String> = None;
    let interval = Duration::from_secs(interval_secs);

    loop {
        if last_poll.map(|t| t.elapsed() >= interval).unwrap_or(true) {
            let (r, mut evs) = tick(&mut state, stuck_secs);
            rows = r;
            evs.reverse(); // newest first when prepended
            for e in evs {
                log.insert(0, e);
            }
            log.truncate(EVENT_CAP);
            last_poll = Some(Instant::now());
        }
        if selected.is_none() && !rows.is_empty() {
            selected = Some(rows[0].key());
        }
        let sel = selected
            .as_ref()
            .and_then(|k| rows.iter().position(|r| &r.key() == k))
            .unwrap_or(0);

        terminal
            .draw(|f| ui(f, &rows, &log, sel, interval_secs, stuck_secs))
            .map_err(Error::Io)?;

        if event::poll(Duration::from_millis(250)).map_err(Error::Io)?
            && let Event::Key(k) = event::read().map_err(Error::Io)?
            && k.kind != KeyEventKind::Release
        {
            match k.code {
                KeyCode::Char('q') | KeyCode::Esc => break,
                KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => break,
                KeyCode::Char('r') => last_poll = None, // force refresh
                KeyCode::Up | KeyCode::Char('k') if !rows.is_empty() => {
                    selected = Some(rows[sel.saturating_sub(1)].key());
                }
                KeyCode::Down | KeyCode::Char('j') if !rows.is_empty() => {
                    selected = Some(rows[(sel + 1).min(rows.len() - 1)].key());
                }
                KeyCode::Enter => {
                    if let Some(r) = rows.get(sel)
                        && let Err(e) = backend::focus(r)
                    {
                        log.insert(
                            0,
                            Ev {
                                icon: "✕",
                                time: chrono::Local::now().format("%H:%M:%S").to_string(),
                                msg: format!("focus failed: {e}"),
                            },
                        );
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn backend_color(b: Backend) -> Color {
    match b {
        Backend::Iterm => Color::Cyan,
        Backend::Tmux => Color::Magenta,
        Backend::Unknown => Color::DarkGray,
    }
}

fn ui(
    f: &mut ratatui::Frame,
    rows: &[Session],
    log: &[Ev],
    sel: usize,
    interval_secs: u64,
    stuck_secs: i64,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(10),
        ])
        .split(f.area());

    let busy = rows.iter().filter(|r| r.status == "busy").count();
    let header = Line::from(vec![
        Span::styled(
            "  fleet ",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("· {} live · {busy} working ", rows.len()),
            Style::default().fg(Color::Gray),
        ),
        Span::styled(
            format!(
                "· every {interval_secs}s · stuck>{stuck_secs}s · {} ",
                chrono::Local::now().format("%H:%M:%S")
            ),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "· ↑↓/jk select · ⏎ focus · q quit · r refresh",
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    f.render_widget(Paragraph::new(header), chunks[0]);

    // Sessions
    let width = chunks[1].width.saturating_sub(4) as usize;
    let mut lines: Vec<Line> = Vec::new();
    for (idx, r) in rows.iter().enumerate() {
        let (dot, state_txt, state_style) = match r.status.as_str() {
            "busy" => ("●", "working", Style::default().fg(Color::Green)),
            "idle" => ("○", "idle   ", Style::default().fg(Color::DarkGray)),
            other => ("·", other, Style::default()),
        };
        let where_ = r.cwd.as_deref().map(home_rel).unwrap_or_else(|| "?".into());
        let title = r.title.as_deref().unwrap_or("(no prompt)");
        let title: String = title.split_whitespace().collect::<Vec<_>>().join(" ");
        let selected = idx == sel;
        let caret = if selected { "▸ " } else { "  " };
        let head = format!(
            "{caret}{dot} {:<11}{:<8}{:>4}  {:<6}",
            r.label(),
            state_txt,
            ago(r.updated_at),
            r.backend.label(),
        );
        let mut spans = vec![
            Span::styled(
                caret.to_string(),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("{dot} "), state_style),
            Span::styled(
                format!("{:<11}", r.label()),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("{:<8}", state_txt), state_style),
            Span::styled(
                format!("{:>4}  ", ago(r.updated_at)),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                format!("{:<6}", r.backend.label()),
                Style::default().fg(backend_color(r.backend)),
            ),
            Span::styled(where_.clone(), Style::default().fg(Color::Blue)),
        ];
        let used = head.chars().count() + where_.chars().count() + 3;
        let room = width.saturating_sub(used);
        if room > 6 {
            let t: String = title.chars().take(room).collect();
            spans.push(Span::styled(
                format!("  {t}"),
                Style::default().fg(Color::Gray),
            ));
        }
        // Highlight the selected row with a subtle background across every span.
        if selected {
            for sp in &mut spans {
                sp.style = sp.style.bg(Color::Indexed(236));
            }
        }
        lines.push(Line::from(spans));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "  no running claude sessions",
            Style::default().fg(Color::DarkGray),
        )));
    }
    let sessions = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" Sessions "),
    );
    f.render_widget(sessions, chunks[1]);

    // Events
    let ev_lines: Vec<Line> = if log.is_empty() {
        vec![Line::from(Span::styled(
            " waiting for something to happen…",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        log.iter()
            .map(|e| {
                Line::from(vec![
                    Span::raw(format!(" {} ", e.icon)),
                    Span::styled(format!("{} ", e.time), Style::default().fg(Color::DarkGray)),
                    Span::raw(e.msg.clone()),
                ])
            })
            .collect()
    };
    let events = Paragraph::new(ev_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" Events "),
    );
    f.render_widget(events, chunks[2]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_awaiting_prompt() {
        assert!(awaiting("│ Do you want to proceed? │"));
        assert!(awaiting("  ❯ 1. Yes  "));
        assert!(!awaiting("✻ Working for 12s"));
    }
}
