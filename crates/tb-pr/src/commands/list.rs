use crate::error::{Error, Result};

pub fn run(_column: Option<String>, _stale_days: Option<u32>, _json: bool) -> Result<()> {
    Err(Error::Other(
        "list: not implemented yet — arrives in M2 (data layer)".to_string(),
    ))
}
