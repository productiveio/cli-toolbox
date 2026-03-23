use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
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

/// Encode a filesystem path into the directory-name format used by Claude.
///
/// Claude encodes project directories by replacing `/` with `-`, e.g.
/// `/Users/test/Projects/myapp` becomes `-Users-test-Projects-myapp`.
/// This encoding is lossy — original hyphens are indistinguishable from
/// path separators — so it can only be used for forward matching, never
/// for decoding back to the original path.
pub fn encode_path(path: &Path) -> String {
    path.to_string_lossy().replace('/', "-")
}

/// Discover JSONL session files under `projects_dir`.
///
/// When `scope_to_cwd` is `Some`, only the project directory whose encoded
/// name matches the encoded `cwd` is scanned.  Otherwise every project
/// directory is visited.
pub fn scan_projects(
    projects_dir: &Path,
    scope_to_cwd: Option<&Path>,
) -> Result<Vec<FileInfo>> {
    let mut results = Vec::new();

    let entries = match fs::read_dir(projects_dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(results),
        Err(e) => return Err(e.into()),
    };

    let encoded_cwd = scope_to_cwd.map(encode_path);

    for entry in entries {
        let entry = entry?;
        let dir_path = entry.path();

        if !dir_path.is_dir() {
            continue;
        }

        let dir_name = match dir_path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name.to_string(),
            None => continue,
        };

        // When scoping, skip directories that don't match the encoded cwd.
        if let Some(ref encoded) = encoded_cwd {
            if dir_name != *encoded {
                continue;
            }
        }

        // Load sessions-index.json for this project directory.
        let index = load_sessions_index(&dir_path);

        // Resolve the project path: prefer sessions-index.json projectPath,
        // fall back to extracting cwd from the first JSONL file found.
        let project_path_from_index = index.values().find_map(|e| e.project_path.clone());

        // Iterate over JSONL files in this project directory.
        let dir_entries = match fs::read_dir(&dir_path) {
            Ok(de) => de,
            Err(_) => continue,
        };

        let mut fallback_project_path: Option<String> = None;

        // Collect JSONL file info first so we can resolve project_path lazily.
        let mut pending: Vec<(String, PathBuf, u64, Option<IndexEntry>)> = Vec::new();

        for file_entry in dir_entries {
            let file_entry = match file_entry {
                Ok(fe) => fe,
                Err(_) => continue,
            };

            let file_path = file_entry.path();

            let file_name = match file_path.file_name().and_then(|n| n.to_str()) {
                Some(name) => name.to_string(),
                None => continue,
            };

            if !file_name.ends_with(".jsonl") {
                continue;
            }

            // Session ID is the filename without extension.
            let session_id = file_name.trim_end_matches(".jsonl").to_string();

            let file_mtime = file_path
                .metadata()
                .and_then(|m| m.modified())
                .map(|t| {
                    t.duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs()
                })
                .unwrap_or(0);

            let index_entry = index.get(&session_id).cloned();

            pending.push((session_id, file_path, file_mtime, index_entry));
        }

        // Resolve fallback project_path if the index didn't have one.
        let resolved_project_path = if let Some(ref pp) = project_path_from_index {
            pp.clone()
        } else {
            // Try to extract cwd from the first JSONL file.
            if fallback_project_path.is_none() {
                for (_, path, _, _) in &pending {
                    if let Some(cwd) = extract_cwd_from_jsonl(path) {
                        fallback_project_path = Some(cwd);
                        break;
                    }
                }
            }
            fallback_project_path.unwrap_or_default()
        };

        for (session_id, file_path, file_mtime, index_entry) in pending {
            results.push(FileInfo {
                session_id,
                file_path,
                file_mtime,
                project_path: resolved_project_path.clone(),
                project_dir: dir_name.clone(),
                index_metadata: index_entry,
            });
        }
    }

    Ok(results)
}

/// Read the first few lines of a JSONL file looking for a `cwd` field.
///
/// Returns the first `cwd` value found, or `None` if the file cannot be
/// read or none of the first 5 lines contain a `cwd` key.
pub fn extract_cwd_from_jsonl(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let reader = BufReader::new(file);

    for line in reader.lines().take(5) {
        let line = line.ok()?;
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
            if let Some(cwd) = value.get("cwd").and_then(|v| v.as_str()) {
                return Some(cwd.to_string());
            }
        }
    }

    None
}

/// Load and parse `sessions-index.json` from a project directory.
///
/// Returns a map from `session_id` to `IndexEntry`.  Returns an empty map
/// if the file doesn't exist or can't be parsed.
pub fn load_sessions_index(project_dir: &Path) -> HashMap<String, IndexEntry> {
    let index_path = project_dir.join("sessions-index.json");

    let content = match fs::read_to_string(&index_path) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };

    let entries: Vec<IndexEntry> = match serde_json::from_str(&content) {
        Ok(e) => e,
        Err(_) => return HashMap::new(),
    };

    entries
        .into_iter()
        .map(|e| (e.session_id.clone(), e))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn create_test_projects_dir(base: &Path) -> PathBuf {
        let projects_dir = base.join(".claude").join("projects");
        fs::create_dir_all(&projects_dir).unwrap();
        projects_dir
    }

    fn create_project_with_sessions(
        projects_dir: &Path,
        dir_name: &str,
        sessions: &[&str],
    ) -> PathBuf {
        let project_dir = projects_dir.join(dir_name);
        fs::create_dir_all(&project_dir).unwrap();

        for session_id in sessions {
            let file_path = project_dir.join(format!("{session_id}.jsonl"));
            fs::write(&file_path, "{\"type\":\"init\"}\n").unwrap();
        }

        project_dir
    }

    #[test]
    fn test_scan_finds_jsonl_files() {
        let tmp = tempfile::tempdir().unwrap();
        let projects_dir = create_test_projects_dir(tmp.path());

        create_project_with_sessions(
            &projects_dir,
            "-Users-test-myapp",
            &["abc-123", "def-456"],
        );

        let results = scan_projects(&projects_dir, None).unwrap();
        assert_eq!(results.len(), 2);

        let mut ids: Vec<&str> = results.iter().map(|f| f.session_id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, vec!["abc-123", "def-456"]);
    }

    #[test]
    fn test_scan_extracts_session_id_from_filename() {
        let tmp = tempfile::tempdir().unwrap();
        let projects_dir = create_test_projects_dir(tmp.path());

        create_project_with_sessions(
            &projects_dir,
            "-Users-test-myapp",
            &["550e8400-e29b-41d4-a716-446655440000"],
        );

        let results = scan_projects(&projects_dir, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].session_id,
            "550e8400-e29b-41d4-a716-446655440000"
        );
    }

    #[test]
    fn test_scan_extracts_project_path_from_index() {
        let tmp = tempfile::tempdir().unwrap();
        let projects_dir = create_test_projects_dir(tmp.path());

        let project_dir = create_project_with_sessions(
            &projects_dir,
            "-Users-test-myapp",
            &["session-1"],
        );

        // Write sessions-index.json with a projectPath.
        let index_content = serde_json::json!([
            {
                "sessionId": "session-1",
                "summary": "Test session",
                "projectPath": "/Users/test/myapp"
            }
        ]);
        fs::write(
            project_dir.join("sessions-index.json"),
            index_content.to_string(),
        )
        .unwrap();

        let results = scan_projects(&projects_dir, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].project_path, "/Users/test/myapp");
        assert!(results[0].index_metadata.is_some());

        let meta = results[0].index_metadata.as_ref().unwrap();
        assert_eq!(meta.summary.as_deref(), Some("Test session"));
    }

    #[test]
    fn test_scan_scoped_to_cwd() {
        let tmp = tempfile::tempdir().unwrap();
        let projects_dir = create_test_projects_dir(tmp.path());

        // Two project directories — one for /Users/test/myapp, one for
        // /Users/test/e2e-tests (note the hyphen in the real path).
        create_project_with_sessions(
            &projects_dir,
            "-Users-test-myapp",
            &["session-a"],
        );
        create_project_with_sessions(
            &projects_dir,
            "-Users-test-e2e-tests",
            &["session-b"],
        );

        // Scope to /Users/test/myapp — should only find session-a.
        let scoped =
            scan_projects(&projects_dir, Some(Path::new("/Users/test/myapp"))).unwrap();
        assert_eq!(scoped.len(), 1);
        assert_eq!(scoped[0].session_id, "session-a");

        // Scope to /Users/test/e2e-tests — should only find session-b.
        // This works because the encoded form "-Users-test-e2e-tests" matches
        // the directory name exactly (even though the encoding is lossy for
        // paths that originally contained hyphens).
        let scoped =
            scan_projects(&projects_dir, Some(Path::new("/Users/test/e2e-tests"))).unwrap();
        assert_eq!(scoped.len(), 1);
        assert_eq!(scoped[0].session_id, "session-b");

        // Unscoped scan should find both.
        let all = scan_projects(&projects_dir, None).unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_encode_path() {
        assert_eq!(
            encode_path(Path::new("/Users/test/Projects/myapp")),
            "-Users-test-Projects-myapp"
        );
        assert_eq!(
            encode_path(Path::new("/Users/test/e2e-tests")),
            "-Users-test-e2e-tests"
        );
    }

    #[test]
    fn test_extract_cwd_from_jsonl() {
        let tmp = tempfile::tempdir().unwrap();
        let file_path = tmp.path().join("test.jsonl");

        // cwd on line 3 (within the first 5 lines).
        fs::write(
            &file_path,
            "{\"type\":\"init\"}\n{\"role\":\"user\"}\n{\"cwd\":\"/Users/test/myapp\"}\n",
        )
        .unwrap();

        assert_eq!(
            extract_cwd_from_jsonl(&file_path),
            Some("/Users/test/myapp".to_string())
        );
    }

    #[test]
    fn test_extract_cwd_from_jsonl_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let file_path = tmp.path().join("test.jsonl");

        fs::write(&file_path, "{\"type\":\"init\"}\n{\"role\":\"user\"}\n").unwrap();

        assert_eq!(extract_cwd_from_jsonl(&file_path), None);
    }

    #[test]
    fn test_load_sessions_index_missing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let index = load_sessions_index(tmp.path());
        assert!(index.is_empty());
    }

    #[test]
    fn test_load_sessions_index_valid() {
        let tmp = tempfile::tempdir().unwrap();
        let content = serde_json::json!([
            {
                "sessionId": "abc-123",
                "summary": "Refactored auth module",
                "messageCount": 42,
                "gitBranch": "feature/auth",
                "projectPath": "/Users/test/myapp"
            },
            {
                "sessionId": "def-456",
                "firstPrompt": "Fix the login bug"
            }
        ]);
        fs::write(tmp.path().join("sessions-index.json"), content.to_string()).unwrap();

        let index = load_sessions_index(tmp.path());
        assert_eq!(index.len(), 2);

        let entry = index.get("abc-123").unwrap();
        assert_eq!(entry.summary.as_deref(), Some("Refactored auth module"));
        assert_eq!(entry.message_count, Some(42));
        assert_eq!(entry.git_branch.as_deref(), Some("feature/auth"));

        let entry2 = index.get("def-456").unwrap();
        assert_eq!(entry2.first_prompt.as_deref(), Some("Fix the login bug"));
        assert!(entry2.summary.is_none());
    }

    #[test]
    fn test_scan_fallback_project_path_from_jsonl() {
        let tmp = tempfile::tempdir().unwrap();
        let projects_dir = create_test_projects_dir(tmp.path());

        let project_dir = create_project_with_sessions(
            &projects_dir,
            "-Users-test-myapp",
            &[],
        );

        // Write a JSONL file with a cwd field (no sessions-index.json).
        let file_path = project_dir.join("session-1.jsonl");
        fs::write(&file_path, "{\"cwd\":\"/Users/test/myapp\"}\n").unwrap();

        let results = scan_projects(&projects_dir, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].project_path, "/Users/test/myapp");
    }

    #[test]
    fn test_scan_nonexistent_projects_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let nonexistent = tmp.path().join("does-not-exist");

        let results = scan_projects(&nonexistent, None).unwrap();
        assert!(results.is_empty());
    }
}
