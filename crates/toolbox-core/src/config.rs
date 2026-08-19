use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Load a named section from `secrets.toml` in the current directory.
/// Returns `Ok(None)` if the file doesn't exist or the section is missing.
pub fn load_secrets_section<T: DeserializeOwned>(section: &str) -> std::io::Result<Option<T>> {
    let path = PathBuf::from("secrets.toml");
    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&path)?;
    let table: toml::Table = toml::from_str(&content)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    match table.get(section) {
        Some(value) => {
            let config: T = value.clone().try_into().map_err(|e: toml::de::Error| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, e)
            })?;
            Ok(Some(config))
        }
        None => Ok(None),
    }
}

/// Load a standalone TOML config file.
/// Returns `Ok(None)` if the file doesn't exist.
pub fn load_standalone<T: DeserializeOwned>(path: &Path) -> std::io::Result<Option<T>> {
    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(path)?;
    let config: T = toml::from_str(&content)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    Ok(Some(config))
}

/// Standard config file path: `{config_dir}/{tool_name}/config.toml`, where
/// `config_dir` is the platform config directory (`~/.config` on Linux,
/// `~/Library/Application Support` on macOS — NOT `~/.config`).
pub fn config_path(tool_name: &str) -> std::io::Result<PathBuf> {
    let dir = dirs::config_dir().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "cannot determine config directory",
        )
    })?;
    Ok(dir.join(tool_name).join("config.toml"))
}

/// Save a serializable config to the given path, creating parent directories.
pub fn save_config<T: Serialize>(path: &Path, config: &T) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = toml::to_string_pretty(config)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, content)
}

/// Read a TOML config file, set a single top-level key, and write it back.
/// Creates the file and parent directories if they don't exist.
pub fn patch_toml(path: &Path, key: &str, value: &str) -> std::io::Result<()> {
    patch_toml_path(path, &[key], value)
}

/// Set a (possibly nested) string key — `["ui", "rows"]` is `[ui] rows = "…"` —
/// creating any missing intermediate tables, and write the file back.
///
/// Edits the document in place, so comments, key order and formatting survive.
/// A file that exists but doesn't parse is an error, never a fresh empty
/// document: silently replacing a config someone hand-edited badly loses the
/// rest of their settings.
pub fn patch_toml_path(path: &Path, keys: &[&str], value: &str) -> std::io::Result<()> {
    if keys.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "patch_toml_path needs at least one key",
        ));
    }
    let mut doc: toml_edit::DocumentMut = if path.exists() {
        std::fs::read_to_string(path)?
            .parse()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?
    } else {
        toml_edit::DocumentMut::new()
    };

    let (last, parents) = keys.split_last().expect("non-empty, checked above");
    let mut table = doc.as_table_mut();
    for k in parents {
        let entry = table
            .entry(k)
            .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()));
        table = entry.as_table_mut().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("`{k}` is not a table"),
            )
        })?;
    }
    table[*last] = toml_edit::value(value);

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, doc.to_string())
}

/// Mask a token for display: `****...XXXX` (last 4 chars).
pub fn masked_token(token: &str) -> String {
    if token.len() > 8 {
        format!("****...{}", &token[token.len() - 4..])
    } else {
        "****".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masked_token_long() {
        assert_eq!(masked_token("abcdefghijklmnop"), "****...mnop");
    }

    #[test]
    fn masked_token_short() {
        assert_eq!(masked_token("short"), "****");
    }

    #[test]
    fn patch_toml_keeps_comments_and_unrelated_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "# my config\ncommand = \"cc\"\n\n[ui]\nmouse = true\n",
        )
        .unwrap();

        patch_toml_path(&path, &["ui", "rows"], "2").unwrap();

        let out = std::fs::read_to_string(&path).unwrap();
        assert!(out.contains("# my config"), "{out}");
        assert!(out.contains("command = \"cc\""), "{out}");
        assert!(out.contains("mouse = true"), "{out}");
        assert!(out.contains("rows = \"2\""), "{out}");

        // An existing value is replaced, not duplicated.
        patch_toml_path(&path, &["ui", "rows"], "auto").unwrap();
        let out = std::fs::read_to_string(&path).unwrap();
        assert_eq!(out.matches("rows =").count(), 1, "{out}");
        assert!(out.contains("rows = \"auto\""), "{out}");
    }

    #[test]
    fn patch_toml_creates_missing_file_and_tables() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/config.toml");
        patch_toml_path(&path, &["ui", "rows"], "1").unwrap();
        let out = std::fs::read_to_string(&path).unwrap();
        assert!(out.contains("[ui]"), "{out}");
        assert!(out.contains("rows = \"1\""), "{out}");
    }

    // A hand-edited syntax error must not be "fixed" by overwriting the file.
    #[test]
    fn patch_toml_refuses_to_clobber_an_unparseable_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let broken = "command = \"cc\n[ui\n";
        std::fs::write(&path, broken).unwrap();
        assert!(patch_toml_path(&path, &["ui", "rows"], "2").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), broken);
    }

    #[test]
    fn config_path_contains_tool_name() {
        let path = config_path("tb-test").unwrap();
        assert!(path.to_str().unwrap().contains("tb-test"));
        assert!(path.to_str().unwrap().ends_with("config.toml"));
    }
}
