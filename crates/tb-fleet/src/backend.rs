//! Backend adapters. Each session is controlled through the backend it actually
//! runs in — iTerm via AppleScript keyed on the session GUID, tmux via the pane id.

use std::process::Command;

use crate::discovery::{Backend, Session};
use crate::error::{Error, Result};

/// AppleScript helper: locate an iTerm session by its `id` (the GUID that also
/// appears in ITERM_SESSION_ID).
const ITERM_FIND: &str = r#"on findSession(theId)
  tell application "iTerm2"
    repeat with w in windows
      repeat with t in tabs of w
        repeat with s in sessions of t
          if (id of s) is theId then return s
        end repeat
      end repeat
    end repeat
  end tell
  return missing value
end findSession"#;

/// Run a static AppleScript, passing dynamic values as argv (injection-safe).
fn osa(script: &str, args: &[&str]) -> Result<String> {
    let file = std::env::temp_dir().join(format!("tb-fleet-{}.applescript", std::process::id()));
    std::fs::write(&file, script)?;
    let mut cmd = Command::new("osascript");
    cmd.arg(&file);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output()?;
    if !out.status.success() {
        return Err(Error::Other(format!(
            "osascript: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn require_handle(s: &Session) -> Result<&str> {
    s.handle.as_deref().ok_or_else(|| {
        Error::Other(format!(
            "session {} has no controllable backend (backend={})",
            s.label(),
            s.backend.label()
        ))
    })
}

pub fn peek(s: &Session) -> Result<String> {
    let handle = require_handle(s)?;
    match s.backend {
        Backend::Iterm => osa(
            &format!(
                r#"{ITERM_FIND}
on run argv
  set s to findSession(item 1 of argv)
  if s is missing value then error "session not found"
  tell application "iTerm2" to return text of s
end run"#
            ),
            &[handle],
        ),
        Backend::Tmux => {
            let out = Command::new("tmux")
                .args(["capture-pane", "-t", handle, "-p"])
                .output()?;
            Ok(String::from_utf8_lossy(&out.stdout).to_string())
        }
        Backend::Unknown => require_handle(s).map(|_| String::new()),
    }
}

pub fn send(s: &Session, text: &str) -> Result<()> {
    let handle = require_handle(s)?;
    match s.backend {
        Backend::Iterm => {
            // Two writes: a single `write text` sends text+newline as one chunk,
            // which the bracketed-paste-aware TUI absorbs as a literal newline
            // instead of submitting. Type without newline, then a bare Enter.
            osa(
                &format!(
                    r#"{ITERM_FIND}
on run argv
  set s to findSession(item 1 of argv)
  if s is missing value then error "session not found"
  tell application "iTerm2" to tell s
    write text (item 2 of argv) without newline
    delay 0.2
    write text ""
  end tell
end run"#
                ),
                &[handle, text],
            )?;
            Ok(())
        }
        Backend::Tmux => {
            Command::new("tmux")
                .args(["send-keys", "-t", handle, "-l", text])
                .status()?;
            Command::new("tmux")
                .args(["send-keys", "-t", handle, "Enter"])
                .status()?;
            Ok(())
        }
        Backend::Unknown => require_handle(s).map(|_| ()),
    }
}

/// Bring a session's terminal to the front (its iTerm tab, or its tmux window/pane).
pub fn focus(s: &Session) -> Result<()> {
    let handle = require_handle(s)?;
    match s.backend {
        Backend::Iterm => {
            osa(
                r#"on run argv
  set theId to item 1 of argv
  tell application "iTerm2"
    activate
    repeat with w in windows
      repeat with t in tabs of w
        repeat with s in sessions of t
          if (id of s) is theId then
            select w
            select t
            select s
            return
          end if
        end repeat
      end repeat
    end repeat
  end tell
  error "session not found"
end run"#,
                &[handle],
            )?;
            Ok(())
        }
        Backend::Tmux => {
            // Resolve the pane's window, select both; switch the client too if we're in tmux.
            let win = Command::new("tmux")
                .args([
                    "display-message",
                    "-p",
                    "-t",
                    handle,
                    "#{session_name}:#{window_index}",
                ])
                .output()?;
            let win = String::from_utf8_lossy(&win.stdout).trim().to_string();
            if !win.is_empty() {
                Command::new("tmux")
                    .args(["select-window", "-t", &win])
                    .status()?;
                if std::env::var_os("TMUX").is_some()
                    && let Some(session) = win.split(':').next()
                {
                    Command::new("tmux")
                        .args(["switch-client", "-t", session])
                        .status()?;
                }
            }
            Command::new("tmux")
                .args(["select-pane", "-t", handle])
                .status()?;
            Ok(())
        }
        Backend::Unknown => require_handle(s).map(|_| ()),
    }
}

/// Shell-quote a value for a single-quoted context.
fn shq(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// How the first prompt reaches the spawned session.
#[derive(Clone, Copy)]
pub enum Prompt<'a> {
    /// Start Claude with no prompt.
    None,
    /// A short one-liner, quoted straight onto the command line.
    Inline(&'a str),
    /// A file whose contents become the prompt. The command line only carries the
    /// path — the shell expands it at launch, so a multi-line brief of any shape
    /// survives AppleScript/send-keys untouched.
    File(&'a str),
}

fn run_command(prompt: Prompt, launcher: &str) -> String {
    match prompt {
        Prompt::None => launcher.to_string(),
        Prompt::Inline("") => launcher.to_string(),
        Prompt::Inline(p) => format!("{launcher} {}", shq(p)),
        Prompt::File(path) => format!("{launcher} \"$(cat {})\"", shq(path)),
    }
}

fn launch_command(dir: &str, prompt: Prompt, launcher: &str) -> String {
    format!("cd {} && {}", shq(dir), run_command(prompt, launcher))
}

/// Spawn a fresh session and return a human description of where it landed.
pub fn spawn(
    backend: Backend,
    dir: &str,
    prompt: Prompt,
    name: Option<&str>,
    window: bool,
    launcher: &str,
) -> Result<String> {
    let line = launch_command(dir, prompt, launcher);
    match backend {
        Backend::Iterm => {
            // A fresh tab's shell is still sourcing the login profile (which on
            // this stack loads secrets), so a single `write text` — text plus its
            // implicit newline in one chunk — can land before the prompt is ready
            // and never submit. Type without a newline, let the shell settle, then
            // send a bare Enter, the same reliable two-write dance `send` uses.
            osa(
                r#"on run argv
  set theCmd to item 1 of argv
  set makeWindow to (item 2 of argv is "1")
  tell application "iTerm2"
    activate
    if makeWindow or (count of windows) is 0 then
      create window with default profile
    else
      tell current window to create tab with default profile
    end if
    tell (current session of current tab of current window)
      delay 0.5
      write text theCmd without newline
      delay 0.3
      write text ""
    end tell
  end tell
end run"#,
                &[&line, if window { "1" } else { "0" }],
            )?;
            Ok(format!(
                "spawned iTerm {}",
                if window { "window" } else { "tab" }
            ))
        }
        Backend::Tmux => {
            let session = name.unwrap_or("fleet");
            let has = Command::new("tmux")
                .args(["has-session", "-t", session])
                .status()?;
            if !has.success() {
                Command::new("tmux")
                    .args(["new-session", "-d", "-s", session])
                    .status()?;
            }
            let out = Command::new("tmux")
                .args([
                    "new-window",
                    "-t",
                    session,
                    "-P",
                    "-F",
                    "#{session_name}:#{window_index}",
                    "-c",
                    dir,
                ])
                .output()?;
            let win = String::from_utf8_lossy(&out.stdout).trim().to_string();
            // Window already opened in `dir`; just run the launcher in it.
            let run = run_command(prompt, launcher);
            Command::new("tmux")
                .args(["send-keys", "-t", &win, "-l", &run])
                .status()?;
            Command::new("tmux")
                .args(["send-keys", "-t", &win, "Enter"])
                .status()?;
            Ok(format!(
                "spawned tmux window {win}  (attach: tmux attach -t {session})"
            ))
        }
        Backend::Unknown => Err(Error::Other("unknown backend".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_and_command() {
        assert_eq!(shq("a b"), "'a b'");
        assert_eq!(shq("it's"), r"'it'\''s'");
        assert_eq!(
            launch_command("/tmp", Prompt::None, "claude"),
            "cd '/tmp' && claude"
        );
        assert_eq!(
            launch_command("/tmp", Prompt::Inline(""), "claude"),
            "cd '/tmp' && claude"
        );
        assert_eq!(
            launch_command("/tmp", Prompt::Inline("hi there"), "claude"),
            "cd '/tmp' && claude 'hi there'"
        );
        // A custom launcher (e.g. the user's `cc` wrapper) replaces `claude`.
        assert_eq!(
            launch_command("/tmp", Prompt::Inline("hi"), "cc"),
            "cd '/tmp' && cc 'hi'"
        );
    }

    #[test]
    fn prompt_file_expands_in_the_spawned_shell() {
        // The brief's text must never reach the command line — only its path does.
        assert_eq!(
            launch_command("/tmp", Prompt::File("/tmp/a b.md"), "claude"),
            r#"cd '/tmp' && claude "$(cat '/tmp/a b.md')""#
        );
    }
}
