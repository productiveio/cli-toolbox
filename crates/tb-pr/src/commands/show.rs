use crate::error::{Error, Result};

pub fn run(_pr_ref: &str, _json: bool) -> Result<()> {
    Err(Error::Other(
        "show: not implemented yet — arrives in M4 (pretty CLI)".to_string(),
    ))
}
