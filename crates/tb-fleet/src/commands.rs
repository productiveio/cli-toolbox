//! One function per CLI verb. `watch` lives in its own module.

use colored::Colorize;

use crate::backend;
use crate::discovery::{self, Backend};
use crate::error::{Error, Result};
use crate::render::{home_rel, plain_table, tail};

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

pub fn spawn(
    prompt: Option<String>,
    dir: Option<String>,
    backend_arg: Option<Backend>,
    name: Option<String>,
    window: bool,
) -> Result<()> {
    let dir = dir.unwrap_or_else(|| {
        std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| ".".into())
    });
    if !std::path::Path::new(&dir).exists() {
        return Err(Error::Other(format!("dir does not exist: {dir}")));
    }
    let backend_kind = backend_arg.unwrap_or_else(|| {
        if std::env::var_os("TMUX").is_some() {
            Backend::Tmux
        } else {
            Backend::Iterm
        }
    });
    let prompt = prompt.unwrap_or_default();
    let desc = backend::spawn(backend_kind, &dir, &prompt, name.as_deref(), window)?;
    if prompt.is_empty() {
        println!("{desc} in {}", home_rel(&dir));
    } else {
        println!("{desc} in {} — \"{prompt}\"", home_rel(&dir));
    }
    Ok(())
}
