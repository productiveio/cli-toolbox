use std::path::{Path, PathBuf};

/// Remove a tool's installed footprint. Always removes the skill dir
/// (`~/.claude/skills/<tool>/`) and the tool's config dir (the parent of its
/// `config.toml`). With `purge`, also removes the installed binary
/// (`~/.local/bin/<tool>`) — otherwise the binary is left in place (it's
/// typically the one currently running). Idempotent: absent paths are reported,
/// not errored.
///
/// Reusable by any `tb-*` tool; wire a `Uninstall { --purge }` command to it.
pub fn run(tool_name: &str, purge: bool) -> Result<(), String> {
    uninstall(tool_name, purge).map_err(|e| e.to_string())
}

fn uninstall(tool_name: &str, purge: bool) -> Result<(), Error> {
    let home = dirs::home_dir().ok_or(Error::NoHomeDir)?;
    let mut report = Report::default();

    let skill_dir = home.join(".claude").join("skills").join(tool_name);
    remove_path(&skill_dir, &mut report)?;

    let config_file = crate::config::config_path(tool_name).map_err(Error::Path)?;
    if let Some(config_dir) = config_file.parent() {
        remove_path(config_dir, &mut report)?;
    }

    let bin = home.join(".local").join("bin").join(tool_name);
    if purge {
        remove_path(&bin, &mut report)?;
    }

    report.print();
    if !purge && bin.exists() {
        println!(
            "Binary left in place: {} — re-run `{} uninstall --purge` to remove it.",
            bin.display(),
            tool_name
        );
    }
    Ok(())
}

#[derive(Default)]
struct Report {
    removed: Vec<PathBuf>,
    missing: Vec<PathBuf>,
}

impl Report {
    fn print(&self) {
        for p in &self.removed {
            println!("Removed: {}", p.display());
        }
        for p in &self.missing {
            println!("Not present: {}", p.display());
        }
        if self.removed.is_empty() {
            println!("Nothing to remove.");
        }
    }
}

/// Remove a file or directory if it exists, recording the outcome.
fn remove_path(path: &Path, report: &mut Report) -> Result<(), Error> {
    if !path.exists() {
        report.missing.push(path.to_path_buf());
        return Ok(());
    }
    let result = if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
    result.map_err(|e| Error::Io(path.to_path_buf(), e))?;
    report.removed.push(path.to_path_buf());
    Ok(())
}

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("cannot determine home directory")]
    NoHomeDir,
    #[error("cannot determine config directory: {0}")]
    Path(std::io::Error),
    #[error("{}: {}", .0.display(), .1)]
    Io(PathBuf, std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_path_handles_dir_file_and_absent() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("skills/tb-x");
        std::fs::create_dir_all(dir.join("nested")).unwrap();
        std::fs::write(dir.join("nested/SKILL.md"), "x").unwrap();
        let file = tmp.path().join("config.toml");
        std::fs::write(&file, "x").unwrap();
        let absent = tmp.path().join("not-there");

        let mut report = Report::default();
        remove_path(&dir, &mut report).unwrap(); // recursive dir
        remove_path(&file, &mut report).unwrap(); // single file
        remove_path(&absent, &mut report).unwrap(); // missing → recorded, not errored

        assert!(!dir.exists() && !file.exists());
        assert_eq!(report.removed.len(), 2);
        assert_eq!(report.missing, vec![absent]);
    }
}
