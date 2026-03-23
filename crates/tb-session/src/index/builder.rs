use rusqlite::Connection;
use crate::error::Result;
use super::scanner::FileInfo;
use super::parser::ParsedSession;

pub fn index_session(_conn: &Connection, _file_info: &FileInfo, _parsed: &ParsedSession) -> Result<()> {
    Ok(())
}
