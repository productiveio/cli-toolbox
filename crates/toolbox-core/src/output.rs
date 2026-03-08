use chrono::{DateTime, Utc};
use serde::Serialize;

/// Pretty-print any serializable value as JSON.
pub fn render_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|e| format!("JSON error: {}", e))
}

/// ISO 8601 timestamp -> relative time ("just now", "5m ago", "2h ago", "3d ago").
/// Returns the raw string on parse failure.
pub fn relative_time(iso: &str) -> String {
    let Ok(dt) = iso.parse::<DateTime<Utc>>() else {
        return iso.to_string();
    };
    let secs = (Utc::now() - dt).num_seconds();
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

/// Truncate a string to `max` chars, appending "..." if truncated.
/// Unicode-safe (uses `floor_char_boundary`).
pub fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let end = max.saturating_sub(3);
    let boundary = s.floor_char_boundary(end);
    format!("{}...", &s[..boundary])
}

/// Pagination hint: "Page 1 of 75 (1500 total). Use --page 2 for next."
/// Returns `None` if everything fits on one page.
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

/// Empty result hint: "No {entity} found. {suggestion}"
pub fn empty_hint(entity: &str, suggestion: &str) -> String {
    format!("No {} found. {}", entity, suggestion)
}

/// Format a USD cost with appropriate precision.
/// < $0.01 shows 4 decimal places, >= $0.01 shows 2.
pub fn fmt_cost(cost: f64) -> String {
    if cost < 0.01 {
        format!("${:.4}", cost)
    } else {
        format!("${:.2}", cost)
    }
}

/// Format a count with thousand separators: 1234567 -> "1,234,567".
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_long() {
        assert_eq!(truncate("hello world, this is long", 10), "hello w...");
    }

    #[test]
    fn pagination_single_page() {
        assert!(pagination_hint(1, 20, 15).is_none());
    }

    #[test]
    fn pagination_multi_page() {
        let hint = pagination_hint(1, 20, 100).unwrap();
        assert!(hint.contains("Page 1 of 5"));
        assert!(hint.contains("--page 2"));
    }

    #[test]
    fn pagination_last_page() {
        let hint = pagination_hint(5, 20, 100).unwrap();
        assert!(hint.contains("Page 5 of 5"));
        assert!(!hint.contains("--page"));
    }

    #[test]
    fn empty_hint_format() {
        assert_eq!(
            empty_hint("traces", "Try wider filters."),
            "No traces found. Try wider filters."
        );
    }

    #[test]
    fn cost_formatting() {
        assert_eq!(fmt_cost(1.5), "$1.50");
        assert_eq!(fmt_cost(0.0023), "$0.0023");
    }

    #[test]
    fn count_formatting() {
        assert_eq!(fmt_count(42), "42");
        assert_eq!(fmt_count(1234), "1,234");
        assert_eq!(fmt_count(1234567), "1,234,567");
    }
}
