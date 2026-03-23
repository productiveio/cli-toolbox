use std::path::{Path, PathBuf};
use crate::error::Result;

#[derive(Debug)]
pub struct FileInfo {
    pub session_id: String,
    pub file_path: PathBuf,
    pub file_mtime: u64,
    pub project_path: String,
    pub project_dir: String,
    pub index_metadata: Option<IndexEntry>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexEntry {
    pub session_id: String,
    pub summary: Option<String>,
    pub first_prompt: Option<String>,
    pub message_count: Option<usize>,
    pub git_branch: Option<String>,
    pub created: Option<String>,
    pub modified: Option<String>,
    pub is_sidechain: Option<bool>,
    pub project_path: Option<String>,
}

pub fn scan_projects(_projects_dir: &Path, _scope_to_cwd: Option<&Path>) -> Result<Vec<FileInfo>> {
    Ok(vec![])
}
