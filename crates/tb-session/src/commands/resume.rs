use crate::error::{Error, Result};

pub fn run(session_id: &str) -> Result<()> {
    let claude_path = which_claude()?;

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = std::process::Command::new(&claude_path)
            .arg("--resume")
            .arg(session_id)
            .exec();
        // exec() only returns on error
        return Err(Error::Other(format!(
            "Failed to exec claude: {}",
            err
        )));
    }

    #[cfg(not(unix))]
    {
        let status = std::process::Command::new(&claude_path)
            .arg("--resume")
            .arg(session_id)
            .status()
            .map_err(|e| Error::Other(format!("Failed to run claude: {}", e)))?;

        if !status.success() {
            return Err(Error::Other(format!(
                "claude exited with status: {}",
                status
            )));
        }
        Ok(())
    }
}

fn which_claude() -> Result<String> {
    let output = std::process::Command::new("which")
        .arg("claude")
        .output()
        .map_err(|e| Error::Other(format!("Failed to run 'which claude': {}", e)))?;

    if !output.status.success() {
        return Err(Error::Other(
            "claude CLI not found in PATH. Install it from https://docs.anthropic.com/s/claude-code"
                .to_string(),
        ));
    }

    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        return Err(Error::Other("claude CLI not found in PATH".to_string()));
    }

    Ok(path)
}
