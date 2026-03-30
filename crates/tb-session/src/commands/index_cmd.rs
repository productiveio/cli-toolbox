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

pub fn run(all_projects: bool) -> Result<()> {
    let config = Config::load()?;
    let conn = crate::index::open_db(true)?;

    let projects_dir = config.projects_dir();
    let cwd = std::env::current_dir().ok();
    let scope: Option<&std::path::Path> = if all_projects { None } else { cwd.as_deref() };

    println!("Indexing sessions...");
    crate::index::ensure_fresh(&conn, &projects_dir, scope)?;

    let stats = crate::index::get_stats(&conn)?;
    println!("Indexed {} sessions", stats.session_count);
    println!(
        "Total: {} sessions across {} projects",
        stats.session_count, stats.project_count
    );
    println!("Database: {}", humanize_bytes(stats.db_size_bytes));

    Ok(())
}
