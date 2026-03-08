use std::fmt;
use std::io::IsTerminal;

use inquire::{InquireError, MultiSelect, Password, PasswordDisplayMode, Select, Text};

/// Result of an interactive prompt that may be cancelled.
pub enum PromptResult<T> {
    /// User provided a value.
    Ok(T),
    /// User cancelled (Esc or Ctrl+C).
    Cancelled,
}

/// Prompt for a masked token interactively.
///
/// - If `flag` is `Some`, returns it directly (no prompt).
/// - If stdin is a TTY, shows an interactive password prompt.
///   - If the user submits empty input and `existing` is `Some`, returns the existing token.
///   - If the user cancels, returns `Cancelled`.
/// - If stdin is not a TTY, returns an error.
pub fn prompt_token(
    label: &str,
    flag: Option<&str>,
    existing: Option<&str>,
) -> Result<PromptResult<String>, String> {
    if let Some(t) = flag {
        return Ok(PromptResult::Ok(t.to_string()));
    }

    if !std::io::stdin().is_terminal() {
        return Err("Token is required. Use --token or run interactively in a terminal.".into());
    }

    let mut prompt = Password::new(label)
        .with_display_mode(PasswordDisplayMode::Masked)
        .without_confirmation();
    if existing.is_some() {
        prompt = prompt.with_help_message("Press Enter to keep existing token");
    }

    match prompt.prompt() {
        Ok(t) if t.is_empty() => match existing {
            Some(tok) => Ok(PromptResult::Ok(tok.to_string())),
            None => Err("Token is required".into()),
        },
        Ok(t) => Ok(PromptResult::Ok(t)),
        Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
            Ok(PromptResult::Cancelled)
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Interactive multi-select from a list of options.
///
/// - `label`: prompt text
/// - `options`: items to choose from (must implement `Display`)
/// - `defaults`: indices of pre-selected items
/// - Returns the selected items, or `Cancelled`.
pub fn prompt_multi_select<T: fmt::Display>(
    label: &str,
    options: Vec<T>,
    defaults: &[usize],
) -> Result<PromptResult<Vec<T>>, String> {
    if !std::io::stdin().is_terminal() {
        return Err("Interactive selection requires a terminal.".into());
    }

    match MultiSelect::new(label, options)
        .with_default(defaults)
        .with_page_size(15)
        .with_help_message("Space to toggle, Enter to confirm, type to filter")
        .prompt()
    {
        Ok(selected) => Ok(PromptResult::Ok(selected)),
        Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
            Ok(PromptResult::Cancelled)
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Interactive text prompt with a default value.
///
/// - If `flag` is `Some`, returns it directly (no prompt).
/// - If stdin is a TTY, shows a text prompt with the default pre-filled.
/// - If stdin is not a TTY, returns the default.
pub fn prompt_text(
    label: &str,
    flag: Option<&str>,
    default: &str,
) -> Result<PromptResult<String>, String> {
    if let Some(v) = flag {
        return Ok(PromptResult::Ok(v.to_string()));
    }

    if !std::io::stdin().is_terminal() {
        return Ok(PromptResult::Ok(default.to_string()));
    }

    match Text::new(label).with_default(default).prompt() {
        Ok(v) => Ok(PromptResult::Ok(v)),
        Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
            Ok(PromptResult::Cancelled)
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Interactive single-select from a list of options.
///
/// - `label`: prompt text
/// - `options`: items to choose from (must implement `Display`)
/// - `starting`: index of the initially highlighted item
/// - Returns the selected item, or `Cancelled`.
pub fn prompt_select<T: fmt::Display>(
    label: &str,
    options: Vec<T>,
    starting: usize,
) -> Result<PromptResult<T>, String> {
    if !std::io::stdin().is_terminal() {
        return Err("Interactive selection requires a terminal.".into());
    }

    match Select::new(label, options)
        .with_starting_cursor(starting)
        .with_help_message("Arrow keys to move, Enter to select")
        .prompt()
    {
        Ok(selected) => Ok(PromptResult::Ok(selected)),
        Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {
            Ok(PromptResult::Cancelled)
        }
        Err(e) => Err(e.to_string()),
    }
}
