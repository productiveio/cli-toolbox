use chrono::{DateTime, Utc};
use colored::Colorize;
use serde::Serialize;

pub fn render_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|e| format!("JSON error: {}", e))
}

pub fn score_color(value: f64) -> String {
    let text = format!("{:.2}", value);
    if value >= 0.8 {
        text.green().to_string()
    } else if value >= 0.5 {
        text.yellow().to_string()
    } else {
        text.red().to_string()
    }
}

pub fn pagination_hint(page: u32, per_page: u32, total: u32) -> Option<String> {
    if total <= per_page {
        return None;
    }
    let total_pages = total.div_ceil(per_page);
    let mut hint = format!("Page {} of {} ({} total).", page, total_pages, total);
    if page < total_pages {
        hint.push_str(&format!(" Use --page {} for next.", page + 1));
    }
    Some(hint)
}

pub fn empty_hint(entity: &str, suggestion: &str) -> String {
    format!("No {} found. {}", entity, suggestion)
}

pub fn relative_time(iso: &str) -> String {
    let Ok(dt) = iso.parse::<DateTime<Utc>>() else {
        return iso.to_string();
    };
    let diff = Utc::now() - dt;
    let secs = diff.num_seconds();
    if secs < 60 {
        "just now".to_string()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let end = s.floor_char_boundary(max.saturating_sub(1));
        format!("{}…", &s[..end])
    }
}

pub fn fmt_cost(cost: f64) -> String {
    if cost < 0.01 {
        format!("${:.4}", cost)
    } else {
        format!("${:.2}", cost)
    }
}
