//! Shared formatting helpers for the plain (non-TUI) output.

use colored::Colorize;

use crate::discovery::Session;

pub fn home_rel(path: &str) -> String {
    match dirs::home_dir().and_then(|h| h.to_str().map(String::from)) {
        Some(home) if path.starts_with(&home) => format!("~{}", &path[home.len()..]),
        _ => path.to_string(),
    }
}

/// Compact age from a ms-epoch timestamp: 12s / 4m / 3h / 2d.
pub fn ago(updated_at: Option<i64>) -> String {
    let Some(ms) = updated_at else {
        return "?".into();
    };
    let secs = ((chrono::Utc::now().timestamp_millis() - ms).max(0)) as f64 / 1000.0;
    if secs < 90.0 {
        format!("{}s", secs.round())
    } else if secs < 5400.0 {
        format!("{}m", (secs / 60.0).round())
    } else if secs < 172_800.0 {
        format!("{}h", (secs / 3600.0).round())
    } else {
        format!("{}d", (secs / 86400.0).round())
    }
}

/// Last `n` non-blank-prefixed lines, each hard-capped to the terminal width.
pub fn tail(text: &str, n: usize, width: usize) -> String {
    let capped: Vec<String> = text
        .trim_end()
        .lines()
        .map(|l| {
            if l.chars().count() > width {
                let mut s: String = l.chars().take(width.saturating_sub(1)).collect();
                s.push('…');
                s
            } else {
                l.to_string()
            }
        })
        .collect();
    let start = capped
        .iter()
        .position(|l| !l.trim().is_empty())
        .unwrap_or(0);
    let slice = &capped[start..];
    let from = slice.len().saturating_sub(n);
    slice[from..].join("\n")
}

/// Pad to `width`, or truncate with an ellipsis so a long name can't shove the
/// columns behind it out of line. Names are user-chosen now, so assume nothing.
pub fn column(text: &str, width: usize) -> String {
    let n = text.chars().count();
    if n <= width {
        return format!("{text}{}", " ".repeat(width - n));
    }
    let keep: String = text.chars().take(width.saturating_sub(1)).collect();
    format!("{keep}…")
}

pub fn plain_table(rows: &[Session]) -> String {
    let mut out = vec![
        format!(
            "\nFLEET — {} live session{}",
            rows.len(),
            if rows.len() == 1 { "" } else { "s" }
        ),
        "─".repeat(74),
    ];
    if rows.is_empty() {
        out.push("(no running claude sessions found)".into());
        return out.join("\n");
    }
    for r in rows {
        let (dot, state) = match r.status.as_str() {
            "busy" => ("●".green(), "working".green()),
            "idle" => ("○".dimmed(), "idle".dimmed()),
            other => ("·".normal(), other.normal()),
        };
        let label = column(&r.label(), 12);
        let where_ = r.cwd.as_deref().map(home_rel).unwrap_or_else(|| "?".into());
        out.push(format!(
            "{dot} {} {:<7} {:>4}  {:<5} {}",
            label.bold(),
            state,
            ago(r.updated_at),
            r.backend.label(),
            where_.dimmed(),
        ));
        let title = r.title.as_deref().unwrap_or("(no prompt captured)");
        let title: String = title
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(70)
            .collect();
        out.push(format!("  {}", title));
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_pad_and_truncate() {
        assert_eq!(column("work-23", 9), "work-23  ");
        assert_eq!(column("a-very-long-session-name", 9), "a-very-l…");
        assert_eq!(column("exactly9c", 9), "exactly9c");
    }

    #[test]
    fn age_and_tail() {
        assert_eq!(ago(None), "?");
        assert!(ago(Some(chrono::Utc::now().timestamp_millis() - 120_000)).ends_with('m'));
        assert_eq!(tail("\n\na\nb\nc\n", 2, 80), "b\nc");
    }
}
