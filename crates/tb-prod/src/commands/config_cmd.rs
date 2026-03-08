use crate::config::Config;
use crate::error::Result;

pub fn init(token: &str, org_id: &str, person_id: Option<&str>) -> Result<()> {
    let config = Config {
        token: token.to_string(),
        org_id: org_id.to_string(),
        person_id: person_id.map(|s| s.to_string()),
        api_base_url: None,
    };
    config.save()?;
    println!("Config saved to {:?}", Config::config_path()?);
    Ok(())
}

pub fn show(config: &Config) {
    println!("token:      {}", config.masked_token());
    println!("org_id:     {}", config.org_id);
    println!(
        "person_id:  {}",
        config.person_id.as_deref().unwrap_or("(not set)")
    );
    println!("base_url:   {}", config.base_url());
}
