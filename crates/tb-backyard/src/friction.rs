//! Pure helpers for the `tb-backyard friction` subcommands. Payload shaping
//! and response parsing live here so they're unit-testable without HTTP.
//!
//! The CLI is a thin authenticated transport over Backyard's friction
//! endpoints (`/spa_api/ai/feedback_entries`); the interactive interview is a
//! skill concern (`p-friction`), not the CLI's. The server expects the entry
//! wrapped as `{ "feedback_entry": { … } }`.

use serde_json::{Map, Value, json};

/// First line of `s`, trimmed and clamped to `max` chars (char-safe).
pub fn summarize(s: &str, max: usize) -> String {
    let first_line = s.lines().next().unwrap_or("").trim();
    first_line.chars().take(max).collect()
}

/// Build the `{ feedback_entry: { … } }` body for quick-mode submission from
/// flags. Only set fields are included, so the server applies its own defaults
/// for everything omitted.
pub fn build_quick_payload(
    description: &str,
    category: Option<&str>,
    severity: &str,
    root_cause: &str,
    repo: Option<&str>,
    time_lost_minutes: Option<i64>,
) -> Value {
    let mut entry = Map::new();
    entry.insert("friction_description".into(), json!(description));
    entry.insert("summary".into(), json!(summarize(description, 200)));
    entry.insert("severity".into(), json!(severity));
    entry.insert("root_cause".into(), json!(root_cause));
    if let Some(c) = category {
        entry.insert("category".into(), json!(c));
    }
    if let Some(r) = repo {
        entry.insert("repo".into(), json!(r));
    }
    if let Some(t) = time_lost_minutes {
        entry.insert("time_lost_minutes".into(), json!(t));
    }
    json!({ "feedback_entry": Value::Object(entry) })
}

/// Accept either a bare entry (`{ … }`) or an already-wrapped
/// `{ feedback_entry: { … } }` and return the wrapped form, so callers can pipe
/// in whichever shape they have without double-wrapping.
pub fn wrap_payload(value: Value) -> Value {
    if value.get("feedback_entry").is_some() {
        value
    } else {
        json!({ "feedback_entry": value })
    }
}

/// Pull the created entry's `id` out of a POST response, tolerating both a
/// JSON:API-style `{ data: { id } }` wrapper and a bare `{ id }`.
pub fn extract_id(resp: &Value) -> Option<i64> {
    resp.get("data")
        .and_then(|d| d.get("id"))
        .or_else(|| resp.get("id"))
        .and_then(Value::as_i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarize_takes_first_line_and_clamps() {
        assert_eq!(summarize("  one line  ", 200), "one line");
        assert_eq!(summarize("first\nsecond", 200), "first");
        assert_eq!(summarize("abcdef", 3), "abc");
        // Char-safe on multi-byte input (no panic on a byte boundary).
        assert_eq!(summarize("héllo", 2), "hé");
    }

    #[test]
    fn build_quick_payload_omits_unset_and_wraps() {
        let p = build_quick_payload("slow tool\nmore", None, "medium", "knowledge-gap", None, None);
        let e = &p["feedback_entry"];
        assert_eq!(e["friction_description"], json!("slow tool\nmore"));
        assert_eq!(e["summary"], json!("slow tool"));
        assert_eq!(e["severity"], json!("medium"));
        assert!(e.get("category").is_none());
        assert!(e.get("repo").is_none());
        assert!(e.get("time_lost_minutes").is_none());

        let p = build_quick_payload("x", Some("behavioral"), "high", "behavioral", Some("api"), Some(5));
        let e = &p["feedback_entry"];
        assert_eq!(e["category"], json!("behavioral"));
        assert_eq!(e["repo"], json!("api"));
        assert_eq!(e["time_lost_minutes"], json!(5));
    }

    #[test]
    fn wrap_payload_is_idempotent() {
        let bare = json!({"summary": "x"});
        assert_eq!(wrap_payload(bare.clone()), json!({"feedback_entry": {"summary": "x"}}));
        let wrapped = json!({"feedback_entry": {"summary": "x"}});
        assert_eq!(wrap_payload(wrapped.clone()), wrapped);
    }

    #[test]
    fn extract_id_handles_both_shapes() {
        assert_eq!(extract_id(&json!({"data": {"id": 42}})), Some(42));
        assert_eq!(extract_id(&json!({"id": 7})), Some(7));
        assert_eq!(extract_id(&json!({"nope": 1})), None);
    }
}
