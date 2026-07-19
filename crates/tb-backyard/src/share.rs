//! Pure helpers for the `tb-backyard share` subcommand surface (sister to
//! `tb_backyard::share_alias`). Visibility-change decision + URL formatter
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

/// Parse a forward duration (`30m`, `24h`, `7d`, `2w`) for `share upload
/// --expires-in` into a `chrono::Duration` the caller adds to `now`.
/// Deliberately not toolbox-core's `--from` parser: that one is past-oriented
/// and date-granular, while expiry needs a future window with sub-day precision.
pub fn parse_expires_in(input: &str) -> Result<chrono::Duration, String> {
    let invalid = || {
        format!(
            "invalid --expires-in `{}` — expected a positive number followed by m, h, d, or w (e.g. 30m, 24h, 7d, 2w)",
            input
        )
    };
    let s = input.trim();
    let unit = s.chars().last().ok_or_else(invalid)?;
    let unit_secs: i64 = match unit {
        'm' => 60,
        'h' => 3600,
        'd' => 86_400,
        'w' => 604_800,
        _ => return Err(invalid()),
    };
    let n: i64 = s[..s.len() - unit.len_utf8()]
        .parse()
        .map_err(|_| invalid())?;
    if n <= 0 {
        return Err(invalid());
    }
    let secs = n
        .checked_mul(unit_secs)
        .ok_or_else(|| format!("--expires-in `{}` is too large", input))?;
    Ok(chrono::Duration::seconds(secs))
}

/// Resolve where `share download` writes the fetched file. A directory
/// `--output` keeps the share's own filename inside it; a non-directory path
/// is used verbatim; no `--output` writes `filename` into the cwd.
pub fn download_dest(
    output: Option<std::path::PathBuf>,
    output_is_dir: bool,
    filename: &str,
) -> std::path::PathBuf {
    match output {
        Some(dir) if output_is_dir => dir.join(filename),
        Some(path) => path,
        None => std::path::PathBuf::from(filename),
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

    #[test]
    fn parse_expires_in_units_and_errors() {
        use chrono::Duration;
        assert_eq!(parse_expires_in("30m").unwrap(), Duration::minutes(30));
        assert_eq!(parse_expires_in("24h").unwrap(), Duration::hours(24));
        assert_eq!(parse_expires_in("7d").unwrap(), Duration::days(7));
        assert_eq!(parse_expires_in(" 2w ").unwrap(), Duration::weeks(2));

        for bad in ["", "d", "7", "7x", "-3d", "0h", "1.5d", "7dd"] {
            assert!(parse_expires_in(bad).is_err(), "expected `{bad}` to error");
        }
    }

    #[test]
    fn download_dest_resolution() {
        use std::path::PathBuf;
        // Directory output keeps the share's filename.
        assert_eq!(
            download_dest(Some(PathBuf::from("/tmp/out")), true, "report.html"),
            PathBuf::from("/tmp/out/report.html")
        );
        // File output is used verbatim.
        assert_eq!(
            download_dest(Some(PathBuf::from("renamed.html")), false, "report.html"),
            PathBuf::from("renamed.html")
        );
        // No output → cwd + filename.
        assert_eq!(
            download_dest(None, false, "report.html"),
            PathBuf::from("report.html")
        );
    }
}
