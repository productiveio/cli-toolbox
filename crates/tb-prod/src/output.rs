use chrono::{DateTime, Utc};
use serde::Serialize;

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

pub fn render_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|e| format!("JSON error: {}", e))
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let end = max.saturating_sub(3);
    let boundary = s.floor_char_boundary(end);
    format!("{}...", &s[..boundary])
}
