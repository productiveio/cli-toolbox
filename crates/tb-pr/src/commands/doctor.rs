use std::process::Command;

use colored::Colorize;

use crate::config::Config;
use crate::error::Result;

fn check(ok: bool, label: &str) -> bool {
    if ok {
        println!("  {} {}", "✓".green().bold(), label);
    } else {
        println!("  {} {}", "✗".red().bold(), label);
    }
    ok
}

fn warn(label: &str) {
    println!("  {} {}", "⚠".yellow().bold(), label);
}

pub fn run() -> Result<()> {
    let mut all_ok = true;

    println!("{}", "tb-pr doctor".bold());
    println!();

    let config = Config::load()?;

    // 1. gh binary in PATH
    let gh_found = which("gh");
    let label = if let Some(ref path) = gh_found {
        format!("gh binary in PATH ({path})")
    } else {
        "gh binary in PATH (install with: brew install gh)".to_string()
    };
    if !check(gh_found.is_some(), &label) {
        all_ok = false;
    }

    // 2. gh auth status
    if gh_found.is_some() {
        match gh_auth_status() {
            GhAuth::Ok { user, host } => {
                check(true, &format!("gh authenticated as {user} on {host}"));
            }
            GhAuth::NotLoggedIn => {
                check(false, "gh authenticated (run: gh auth login)");
                all_ok = false;
            }
            GhAuth::Unknown(msg) => {
                warn(&format!("gh auth status unclear: {msg}"));
            }
        }
    }

    // 3. Config file
    let config_path = Config::config_path()?;
    if config_path.exists() {
        check(
            true,
            &format!("Config file exists ({})", config_path.display()),
        );
    } else {
        warn(&format!(
            "Config file not found ({}  — run: tb-pr config init)",
            config_path.display()
        ));
    }

    // 4. Cache dir resolvable
    match config.cache_dir() {
        Ok(dir) => {
            check(true, &format!("Cache dir writable ({})", dir.display()));
        }
        Err(e) => {
            check(false, &format!("Cache dir not writable: {e}"));
            all_ok = false;
        }
    }

    println!();
    if all_ok {
        println!("{}", "All checks passed.".green().bold());
    } else {
        println!("{}", "Some checks failed.".red().bold());
    }

    Ok(())
}

fn which(bin: &str) -> Option<String> {
    let output = Command::new("which").arg(bin).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() { None } else { Some(path) }
}

enum GhAuth {
    Ok { user: String, host: String },
    NotLoggedIn,
    Unknown(String),
}

fn gh_auth_status() -> GhAuth {
    // `gh auth status` writes to stderr. Exit code non-zero means not logged in.
    let output = match Command::new("gh").args(["auth", "status"]).output() {
        Ok(o) => o,
        Err(e) => return GhAuth::Unknown(e.to_string()),
    };

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    if !output.status.success() {
        return GhAuth::NotLoggedIn;
    }

    let user = parse_gh_field(&combined, "account ")
        .or_else(|| parse_gh_field(&combined, "Logged in to github.com as "))
        .unwrap_or_else(|| "unknown".to_string());
    let host =
        parse_gh_field(&combined, "Logged in to ").unwrap_or_else(|| "github.com".to_string());

    GhAuth::Ok { user, host }
}

/// Extract a token after a marker up to the next whitespace or ` (`.
fn parse_gh_field(text: &str, marker: &str) -> Option<String> {
    let idx = text.find(marker)?;
    let rest = &text[idx + marker.len()..];
    let end = rest
        .find(|c: char| c.is_whitespace() || c == '(')
        .unwrap_or(rest.len());
    let value = rest[..end].trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}
