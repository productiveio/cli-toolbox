use chrono::{DateTime, Utc};
use serde::Serialize;

/// Format an ISO 8601 timestamp as a relative time string ("2h ago", "3d ago").
pub fn relative_time(iso: &str) -> String {
    let dt = match iso.parse::<DateTime<Utc>>() {
        Ok(dt) => dt,
        Err(_) => return iso.to_string(),
    };
    let delta = Utc::now() - dt;
    let mins = delta.num_minutes();
    if mins < 1 {
        "just now".to_string()
    } else if mins < 60 {
        format!("{}m ago", mins)
    } else if mins < 1440 {
        format!("{}h ago", mins / 60)
    } else {
        format!("{}d ago", mins / 1440)
    }
}

/// Render a value as pretty-printed JSON.
pub fn render_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|e| format!("JSON error: {}", e))
}

/// Truncate a string to `max` characters, appending "..." if truncated.
/// Safe for multi-byte UTF-8.
pub fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let end = max.saturating_sub(3);
    let boundary = s.floor_char_boundary(end);
    format!("{}...", &s[..boundary])
}

/// Format a number with thousand separators.
pub fn fmt_count(n: u64) -> String {
    if n < 1_000 {
        return n.to_string();
    }
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}
