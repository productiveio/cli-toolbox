//! Session discovery. The authoritative source is Claude's own live registry at
//! `~/.claude/sessions/<pid>.json` (precise pid -> sessionId -> cwd -> status).
//! Each entry is enriched with the process tty and the backend it runs in.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

use serde::Serialize;

pub fn claude_home() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".claude")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    Iterm,
    Tmux,
    Unknown,
}

impl Backend {
    pub fn label(self) -> &'static str {
        match self {
            Backend::Iterm => "iterm",
            Backend::Tmux => "tmux",
            Backend::Unknown => "?",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Session {
    pub pid: i64,
    pub session_id: Option<String>,
    pub name: Option<String>,
    pub cwd: Option<String>,
    pub status: String,
    /// Milliseconds since the epoch of the last status update.
    pub updated_at: Option<i64>,
    pub tty: Option<String>,
    pub backend: Backend,
    /// Backend-specific control handle: iTerm session GUID or tmux pane id.
    pub handle: Option<String>,
    /// Terminal tab name: iTerm tab title or tmux window name.
    pub tab: Option<String>,
    pub title: Option<String>,
}

impl Session {
    /// Stable identity key: sessionId if known, else pid. Used to track a session
    /// across polls even as the sorted order shifts.
    pub fn key(&self) -> String {
        self.session_id
            .clone()
            .unwrap_or_else(|| self.pid.to_string())
    }

    /// Short human label: derived name, else short sessionId, else pid.
    pub fn label(&self) -> String {
        self.name
            .clone()
            .or_else(|| {
                self.session_id
                    .as_ref()
                    .map(|s| s.chars().take(8).collect())
            })
            .unwrap_or_else(|| self.pid.to_string())
    }
}

#[derive(serde::Deserialize)]
struct Reg {
    pid: Option<i64>,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    cwd: Option<String>,
    name: Option<String>,
    status: Option<String>,
    kind: Option<String>,
    #[serde(rename = "updatedAt")]
    updated_at: Option<i64>,
    #[serde(rename = "statusUpdatedAt")]
    status_updated_at: Option<i64>,
}

pub fn discover() -> Vec<Session> {
    let dir = claude_home().join("sessions");
    let panes = tmux_panes();
    let mut out = Vec::new();

    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(reg) = serde_json::from_str::<Reg>(&text) else {
            continue;
        };
        let Some(pid) = reg.pid else { continue };
        // Interactive sessions only (skip print/headless runs).
        if reg.kind.as_deref().is_some_and(|k| k != "interactive") {
            continue;
        }
        // ps returns non-zero for a dead pid, so this doubles as a liveness check.
        let Some(tty) = tty_of(pid) else { continue };
        let (backend, handle) = backend_of(pid, Some(tty.as_str()), &panes);
        // tmux tab (window name) is free from the panes query; iTerm tab titles are
        // filled lazily by enrich_iterm_tabs (one AppleScript call) only for display.
        let tab = if backend == Backend::Tmux {
            panes
                .get(&format!("/dev/{tty}"))
                .map(|(_, _, window)| window.clone())
                .filter(|w| !w.is_empty())
        } else {
            None
        };
        let title = match (reg.session_id.as_deref(), reg.cwd.as_deref()) {
            (Some(sid), Some(cwd)) => title_for(cwd, sid),
            _ => None,
        };
        out.push(Session {
            pid,
            status: reg.status.unwrap_or_else(|| "unknown".into()),
            updated_at: reg.status_updated_at.or(reg.updated_at),
            title,
            tty: Some(tty),
            backend,
            handle,
            tab,
            session_id: reg.session_id,
            name: reg.name,
            cwd: reg.cwd,
        });
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.updated_at.unwrap_or(0)));
    out
}

/// Resolve a target (sessionId prefix, derived name, or pid) to exactly one session.
pub fn resolve(target: &str) -> Result<Session, String> {
    let rows = discover();
    let hits: Vec<Session> = rows
        .into_iter()
        .filter(|r| {
            r.pid.to_string() == target
                || r.name.as_deref() == Some(target)
                || r.session_id
                    .as_deref()
                    .is_some_and(|s| s.starts_with(target))
        })
        .collect();
    match hits.len() {
        1 => Ok(hits.into_iter().next().unwrap()),
        0 => Err(format!("no live session matches \"{target}\"")),
        _ => Err(format!(
            "\"{target}\" matches {} sessions: {} — be more specific",
            hits.len(),
            hits.iter()
                .map(Session::label)
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// The Claude session this process was launched from: the nearest ancestor pid
/// that owns a registry entry (tb-fleet runs as claude -> shell -> us). `None`
/// when invoked straight from a terminal.
pub fn origin() -> Option<Session> {
    let mut chain = Vec::new();
    let mut pid = std::process::id() as i64;
    for _ in 0..6 {
        let Some(parent) = ppid_of(pid) else { break };
        if parent <= 1 {
            break;
        }
        chain.push(parent);
        pid = parent;
    }
    let rows = discover();
    chain
        .into_iter()
        .find_map(|p| rows.iter().find(|s| s.pid == p).cloned())
}

// --- process introspection ---------------------------------------------------

fn ppid_of(pid: i64) -> Option<i64> {
    let out = Command::new("ps")
        .args(["-o", "ppid=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

/// Returns the tty of a live process, `None` if the process is dead. A live
/// process with no controlling tty yields `Some("")` collapsed to `None` here,
/// which is fine: such a session can't be controlled anyway.
fn tty_of(pid: i64) -> Option<String> {
    let out = Command::new("ps")
        .args(["-o", "tty=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let tty = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if tty.is_empty() || tty == "??" || tty == "?" {
        return None;
    }
    Some(tty)
}

fn env_of(pid: i64) -> String {
    Command::new("ps")
        .args(["eww", "-p", &pid.to_string()])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default()
}

/// Detect the backend from the process environment (TMUX -> tmux, ITERM_SESSION_ID
/// -> iTerm) and resolve the handle its adapter needs.
fn backend_of(
    pid: i64,
    tty: Option<&str>,
    panes: &HashMap<String, (String, String, String)>,
) -> (Backend, Option<String>) {
    let env = env_of(pid);
    if env.starts_with("TMUX=") || env.contains(" TMUX=") {
        let handle = tty.and_then(|t| {
            panes.get(&format!("/dev/{t}")).map(|(target, pane_id, _)| {
                if pane_id.is_empty() {
                    target.clone()
                } else {
                    pane_id.clone()
                }
            })
        });
        return (Backend::Tmux, handle);
    }
    if let Some(idx) = env.find("ITERM_SESSION_ID=") {
        let rest = &env[idx + "ITERM_SESSION_ID=".len()..];
        let token = rest.split_whitespace().next().unwrap_or("");
        if let Some((_, guid)) = token.split_once(':') {
            return (Backend::Iterm, Some(guid.to_string()));
        }
    }
    (Backend::Unknown, None)
}

/// pane_tty -> (target, pane_id, window_name) for every tmux pane, empty if tmux isn't running.
fn tmux_panes() -> HashMap<String, (String, String, String)> {
    let mut map = HashMap::new();
    let Ok(out) = Command::new("tmux")
        .args([
            "list-panes",
            "-a",
            "-F",
            "#{pane_tty}\t#{session_name}:#{window_index}.#{pane_index}\t#{pane_id}\t#{window_name}",
        ])
        .output()
    else {
        return map;
    };
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let mut parts = line.split('\t');
        if let (Some(ptty), Some(target), Some(pane_id)) =
            (parts.next(), parts.next(), parts.next())
        {
            let window = parts.next().unwrap_or("").to_string();
            map.insert(
                ptty.to_string(),
                (target.to_string(), pane_id.to_string(), window),
            );
        }
    }
    map
}

/// Fill iTerm sessions' `tab` with their tab title via a single AppleScript call.
/// Kept out of `discover()` so one-shot commands (peek/send) don't pay for it.
pub fn enrich_iterm_tabs(rows: &mut [Session]) {
    if !rows
        .iter()
        .any(|r| r.backend == Backend::Iterm && r.tab.is_none())
    {
        return;
    }
    let script = r#"tell application "iTerm2"
  set out to ""
  repeat with w in windows
    repeat with t in tabs of w
      set tt to ""
      try
        set tt to title of t
      end try
      repeat with s in sessions of t
        set out to out & (id of s) & tab & tt & linefeed
      end repeat
    end repeat
  end repeat
  return out
end tell"#;
    let Ok(out) = Command::new("osascript").arg("-e").arg(script).output() else {
        return;
    };
    if !out.status.success() {
        return;
    }
    let mut names = HashMap::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        if let Some((id, name)) = line.split_once('\t') {
            let name = name.trim();
            if !name.is_empty() {
                names.insert(id.to_string(), name.to_string());
            }
        }
    }
    for r in rows.iter_mut() {
        if r.backend == Backend::Iterm
            && let Some(name) = r.handle.as_deref().and_then(|g| names.get(g))
        {
            r.tab = Some(name.clone());
        }
    }
}

// --- transcript title --------------------------------------------------------

fn encode_cwd(cwd: &str) -> String {
    cwd.chars()
        .map(|c| if c == '/' || c == '.' { '-' } else { c })
        .collect()
}

/// Title = the session's first genuine user prompt from its transcript.
fn title_for(cwd: &str, session_id: &str) -> Option<String> {
    let file = claude_home()
        .join("projects")
        .join(encode_cwd(cwd))
        .join(format!("{session_id}.jsonl"));
    let text = std::fs::read_to_string(&file).ok()?;
    for line in text.lines().take(400) {
        if !line.contains("\"type\":\"user\"") {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if v["message"]["role"] != "user" {
            continue;
        }
        let content = &v["message"]["content"];
        let t = if let Some(s) = content.as_str() {
            s.to_string()
        } else if let Some(arr) = content.as_array() {
            arr.iter()
                .find(|b| b["type"] == "text")
                .and_then(|b| b["text"].as_str())
                .unwrap_or("")
                .to_string()
        } else {
            String::new()
        };
        let t = t.trim();
        if !t.is_empty() && !t.starts_with('<') && !t.starts_with('/') {
            return Some(t.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cwd_encoding() {
        assert_eq!(encode_cwd("/Users/ivan/Code/work"), "-Users-ivan-Code-work");
        assert_eq!(encode_cwd("/a/b.c"), "-a-b-c");
    }
}
