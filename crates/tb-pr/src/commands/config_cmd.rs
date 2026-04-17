use crate::config::Config;
use crate::error::Result;

pub fn init() -> Result<()> {
    let config = Config::default();
    config.save()?;
    let path = Config::config_path()?;
    println!("Config initialized at: {}", path.display());
    Ok(())
}

pub fn show() -> Result<()> {
    let config = Config::load()?;
    let path = Config::config_path()?;

    println!("Config file:          {}", path.display());
    println!("github.org:           {}", config.github.org);
    println!(
        "github.username_override: {}",
        if config.github.username_override.is_empty() {
            "(derived from gh)".to_string()
        } else {
            config.github.username_override.clone()
        }
    );
    println!(
        "refresh.interval_minutes: {}",
        config.refresh.interval_minutes
    );
    println!("productive.org_slug:  {}", config.productive.org_slug);
    match config.cache_dir() {
        Ok(p) => println!("cache_dir:            {}", p.display()),
        Err(e) => println!("cache_dir:            (error: {e})"),
    }

    Ok(())
}
