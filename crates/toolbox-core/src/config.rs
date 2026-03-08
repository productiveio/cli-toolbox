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

/// Standard config directory path: `~/.config/{tool_name}/config.toml`.
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
    fn config_path_contains_tool_name() {
        let path = config_path("tb-test").unwrap();
        assert!(path.to_str().unwrap().contains("tb-test"));
        assert!(path.to_str().unwrap().ends_with("config.toml"));
    }
}
