use chrono::{DateTime, TimeZone, Utc};
use chrono_tz::Tz;
use serde::Serialize;

/// Format a UTC epoch timestamp to local time string.
pub fn epoch_to_local(seconds: i64, tz: &Tz) -> String {
    let dt: DateTime<Utc> = Utc
        .timestamp_opt(seconds, 0)
        .single()
        .unwrap_or_else(Utc::now);
    dt.with_timezone(tz).format("%Y-%m-%d %H:%M").to_string()
}

/// Format an ISO 8601 timestamp to local time string.
pub fn iso_to_local(iso: &str, tz: &Tz) -> String {
    iso.parse::<DateTime<Utc>>()
        .map(|dt| dt.with_timezone(tz).format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|_| iso.to_string())
}

/// Format the duration between two ISO timestamps as a human-readable string.
pub fn duration_str(start_iso: &str, end_iso: &str) -> String {
    let start = start_iso.parse::<DateTime<Utc>>().ok();
    let end = end_iso.parse::<DateTime<Utc>>().ok();
    match (start, end) {
        (Some(s), Some(e)) => format_duration_secs((e - s).num_seconds()),
        _ => "?".to_string(),
    }
}

/// Format seconds into a human-readable duration string.
pub fn format_duration_secs(total_secs: i64) -> String {
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    if mins > 0 {
        format!("{}m {}s", mins, secs)
    } else {
        format!("{}s", secs)
    }
}

/// Render a result as pretty-printed JSON.
pub fn render_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|e| format!("JSON error: {}", e))
}

/// Strip ANSI escape codes from text.
pub fn strip_ansi(text: &str) -> String {
    let re = regex::Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]").unwrap();
    re.replace_all(text, "").to_string()
}
