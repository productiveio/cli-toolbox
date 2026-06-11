//! Pure helpers for the `tb-lf share` subcommand surface (sister to
//! `tb_lf::share_alias`). Visibility-change decision + URL formatter
//! live here so the CLI's UX symmetry with the SPA's `EditShareSheet`
//! is unit-testable without HTTP.

/// SPA's `EditShareSheet` AlertDialog copy. Mirrored at the CLI on
/// `private → unlisted` so the SPA and CLI tell users the same thing.
pub const SHARE_ESCALATION_COPY: &str =
    "Anyone with this URL will be able to view it without logging in. Continue?";

/// `https://backyard.productive.io/s/<token>` — trims a trailing slash
/// from the base so we never emit `//s/...`.
pub fn share_url(base: &str, token: &str) -> String {
    format!("{}/s/{}", base.trim_end_matches('/'), token)
}

/// Direction of a `share update --visibility` transition. The SPA mirrors
/// this as an asymmetric AlertDialog on escalation and a toast on
/// de-escalation; same shape applies to the CLI.
#[derive(Debug, PartialEq, Eq)]
pub enum ShareVisibilityChange {
    /// No `--visibility` flag was given OR the value equals the current.
    None,
    /// `private → unlisted` — exposure escalation. Gate.
    Escalation,
    /// `unlisted → private` — exposure de-escalation. Notice only.
    DeEscalation,
}

pub fn visibility_change(current: &str, new: Option<&str>) -> ShareVisibilityChange {
    match (current, new) {
        (_, None) => ShareVisibilityChange::None,
        (cur, Some(n)) if cur == n => ShareVisibilityChange::None,
        ("private", Some("unlisted")) => ShareVisibilityChange::Escalation,
        ("unlisted", Some("private")) => ShareVisibilityChange::DeEscalation,
        // Anything else (including the would-be-invalid "private" → "private"
        // already caught above, or future visibility values) falls through as
        // None — the local --visibility validator in main.rs has already
        // rejected non-{private,unlisted} values before we get here.
        _ => ShareVisibilityChange::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_url_strips_trailing_slash() {
        assert_eq!(
            share_url("https://backyard.productive.io", "abc"),
            "https://backyard.productive.io/s/abc"
        );
        assert_eq!(
            share_url("https://backyard.productive.io/", "abc"),
            "https://backyard.productive.io/s/abc"
        );
        assert_eq!(
            share_url("http://localhost:3080", "xyz"),
            "http://localhost:3080/s/xyz"
        );
    }

    #[test]
    fn visibility_change_matrix() {
        // No flag → None
        assert_eq!(
            visibility_change("private", None),
            ShareVisibilityChange::None
        );
        assert_eq!(
            visibility_change("unlisted", None),
            ShareVisibilityChange::None
        );

        // Same → None
        assert_eq!(
            visibility_change("private", Some("private")),
            ShareVisibilityChange::None
        );
        assert_eq!(
            visibility_change("unlisted", Some("unlisted")),
            ShareVisibilityChange::None
        );

        // Escalation
        assert_eq!(
            visibility_change("private", Some("unlisted")),
            ShareVisibilityChange::Escalation
        );

        // De-escalation
        assert_eq!(
            visibility_change("unlisted", Some("private")),
            ShareVisibilityChange::DeEscalation
        );
    }
}
