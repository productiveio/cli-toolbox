use std::path::PathBuf;

pub struct SkillConfig {
    pub tool_name: &'static str,
    pub content: &'static str,
}

#[derive(clap::Subcommand)]
pub enum SkillAction {
    /// Install SKILL.md to ~/.claude/skills/
    Install {
        /// Overwrite existing SKILL.md without prompting
        #[arg(long)]
        force: bool,
    },
    /// Print SKILL.md to stdout
    Show,
}

/// Dispatch a skill subcommand. Returns `Ok(())` on success or a
/// string error suitable for `Box<dyn Error>` return types.
pub fn run(config: &SkillConfig, action: &SkillAction) -> Result<(), String> {
    match action {
        SkillAction::Show => {
            print!("{}", config.content);
            Ok(())
        }
        SkillAction::Install { force } => install(config, *force)
            .map(|_| ())
            .map_err(|e| e.to_string()),
    }
}

/// Install SKILL.md to `~/.claude/skills/{tool_name}/SKILL.md`.
///
/// Returns the path written on success.
fn install(config: &SkillConfig, force: bool) -> Result<PathBuf, Error> {
    let dir = dirs::home_dir()
        .ok_or(Error::NoHomeDir)?
        .join(".claude")
        .join("skills")
        .join(config.tool_name);

    std::fs::create_dir_all(&dir).map_err(|e| Error::Io(dir.clone(), e))?;

    let path = dir.join("SKILL.md");

    if path.exists() && !force {
        let existing = std::fs::read_to_string(&path).map_err(|e| Error::Io(path.clone(), e))?;
        if existing == config.content {
            println!("Already up to date: {}", path.display());
            return Ok(path);
        }
        return Err(Error::AlreadyExists(path));
    }

    std::fs::write(&path, config.content).map_err(|e| Error::Io(path.clone(), e))?;
    println!("Installed: {}", path.display());
    Ok(path)
}

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("cannot determine home directory")]
    NoHomeDir,
    #[error("{} already exists (use --force to overwrite)", .0.display())]
    AlreadyExists(PathBuf),
    #[error("{}: {}", .0.display(), .1)]
    Io(PathBuf, std::io::Error),
}
