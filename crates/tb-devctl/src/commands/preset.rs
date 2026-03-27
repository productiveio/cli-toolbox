use std::path::Path;

use colored::Colorize;

use crate::config::Config;
use crate::error::{Error, Result};

pub fn run(config: &Config, project_root: &Path, preset_name: &str) -> Result<()> {
    let preset = config.presets.get(preset_name).ok_or_else(|| {
        let available: Vec<&str> = config.presets.keys().map(|k| k.as_str()).collect();
        Error::Config(format!(
            "Unknown preset: '{}'. Available: {}",
            preset_name,
            available.join(", ")
        ))
    })?;

    if let Some(desc) = &preset.description {
        println!("{} {}", "Preset:".blue(), desc);
    }

    let mode = preset.mode.as_deref().unwrap_or("local");

    // Set preset env vars in the current process (inherited by child commands)
    for (key, val) in &preset.env {
        println!("  {} {}={}", "env".dimmed(), key, val);
        // SAFETY: single-threaded CLI, no concurrent env access
        unsafe { std::env::set_var(key, val) };
    }

    match mode {
        "docker" => crate::commands::start::docker(config, project_root, &preset.services),
        "local" => {
            // Start each service locally in background
            for (i, service) in preset.services.iter().enumerate() {
                let is_last = i == preset.services.len() - 1;
                if is_last {
                    // Last service runs in foreground (so Ctrl+C stops everything)
                    println!();
                    crate::commands::local::start(
                        config,
                        project_root,
                        service,
                        None,
                        false, // foreground
                    )?;
                } else {
                    // Other services run in background
                    crate::commands::local::start(
                        config,
                        project_root,
                        service,
                        None,
                        true, // background
                    )?;
                }
            }
            Ok(())
        }
        other => Err(Error::Config(format!(
            "Unknown preset mode: '{}'. Use 'docker' or 'local'.",
            other
        ))),
    }
}
