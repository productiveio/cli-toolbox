use colored::Colorize;

use crate::config::Config;
use crate::error::Result;

fn humanize_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

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

    println!("{}", "tb-session doctor".bold());
    println!();

    let config = Config::load()?;

    // 1. Claude home directory
    let claude_home = config.claude_home_path();
    let claude_home_exists = claude_home.exists();
    let label = format!("Claude home exists ({})", claude_home.display());
    if !check(claude_home_exists, &label) {
        all_ok = false;
    }

    // 2. Projects directory
    let projects_dir = config.projects_dir();
    let subdir_count = if projects_dir.exists() {
        std::fs::read_dir(&projects_dir)
            .map(|entries| entries.filter_map(|e| e.ok()).filter(|e| e.path().is_dir()).count())
            .unwrap_or(0)
    } else {
        0
    };
    let projects_exists = projects_dir.exists();
    let label = format!(
        "Projects directory exists ({} — {} subdirs)",
        projects_dir.display(),
        subdir_count
    );
    if !check(projects_exists, &label) {
        all_ok = false;
    }

    // 3. Database + FTS5 test
    let db_path = config.db_path()?;
    let db_exists = db_path.exists();
    if db_exists {
        let db_size = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);
        let label = format!(
            "Database exists ({} — {})",
            db_path.display(),
            humanize_bytes(db_size)
        );
        check(true, &label);

        // Test FTS5 availability
        let fts5_ok = test_fts5(&db_path);
        if !check(fts5_ok, "SQLite FTS5 extension available") {
            all_ok = false;
        }
    } else {
        warn(&format!("Database not yet created ({})", db_path.display()));
    }

    // 4. Config file
    let config_path = Config::config_path()?;
    if config_path.exists() {
        let label = format!("Config file exists ({})", config_path.display());
        check(true, &label);
    } else {
        warn(&format!(
            "Config file not found ({}  — using defaults)",
            config_path.display()
        ));
    }

    // 5. claude binary in PATH
    let claude_found = which_claude();
    let label = if let Some(ref path) = claude_found {
        format!("claude binary found ({})", path)
    } else {
        "claude binary found in PATH".to_string()
    };
    if !check(claude_found.is_some(), &label) {
        all_ok = false;
    }

    println!();
    if all_ok {
        println!("{}", "All checks passed.".green().bold());
    } else {
        println!("{}", "Some checks failed.".red().bold());
    }

    Ok(())
}

fn test_fts5(db_path: &std::path::Path) -> bool {
    let conn = match rusqlite::Connection::open(db_path) {
        Ok(c) => c,
        Err(_) => return false,
    };
    let create = conn.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS _doctor_fts5_test \
         USING fts5(content); \
         DROP TABLE IF EXISTS _doctor_fts5_test;",
    );
    create.is_ok()
}

fn which_claude() -> Option<String> {
    let output = std::process::Command::new("which")
        .arg("claude")
        .output()
        .ok()?;
    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            None
        } else {
            Some(path)
        }
    } else {
        None
    }
}
