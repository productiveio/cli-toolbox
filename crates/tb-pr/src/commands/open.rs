use crate::commands::util::{open_url, parse_pr_ref};
use crate::config::Config;
use crate::error::Result;

pub fn run(pr_ref: &str) -> Result<()> {
    let config = Config::load()?;
    let parsed = parse_pr_ref(pr_ref, &config.github.org)?;
    let url = parsed.web_url();
    println!("Opening {url}");
    open_url(&url)
}
