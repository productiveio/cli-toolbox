use crate::config::Config;
use crate::error::Result;

/// Write default config to disk and print its path.
pub fn init() -> Result<()> {
    let config = Config::default();
    config.save()?;
    let path = Config::config_path()?;
    println!("Config initialized at: {}", path.display());
    Ok(())
}

/// Load and print all config values and resolved paths.
pub fn show() -> Result<()> {
    let config = Config::load()?;
    let path = Config::config_path()?;

    println!("Config file:    {}", path.display());
    println!("claude_home:    {}", config.claude_home);
    println!(
        "claude_home resolved: {}",
        config.claude_home_path().display()
    );
    println!("projects_dir:   {}", config.projects_dir().display());
    match config.db_path() {
        Ok(p) => println!("db_path:        {}", p.display()),
        Err(e) => println!("db_path:        (error: {})", e),
    }
    println!("ttl_minutes:    {}", config.ttl_minutes);
    println!("ttl:            {}s", config.ttl().as_secs());
    println!("default_limit:  {}", config.default_limit);

    Ok(())
}
