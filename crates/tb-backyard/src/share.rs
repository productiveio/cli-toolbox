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

/// Guard against a server-supplied filename escaping the destination
/// directory. The viewer route is `/s/:token/*filename` (a glob segment),
/// so nothing on the wire guarantees a filename is a single path component.
/// Accept exactly one non-empty, non-`.`/`..` component with no separators
/// (either flavour) and no NUL.
///
/// `:` is deliberately NOT rejected: it is a legal byte in a filename on the
/// platforms we ship (macOS, Linux), so refusing it would reject shares that
/// download fine today. It would only matter as a Windows drive prefix, and
/// there is no Windows release target.
pub fn safe_share_filename(filename: &str) -> Result<&str, String> {
    let bad = || {
        format!(
            "share file `{}` has an unsafe filename — refusing to write it outside the destination directory",
            filename.escape_debug()
        )
    };
    if filename.is_empty() || filename == "." || filename == ".." {
        return Err(bad());
    }
    if filename.contains(['/', '\\', '\0']) {
        return Err(bad());
    }
    Ok(filename)
}

/// Where a multi-file share lands: `--output <dir>` verbatim when given,
/// otherwise `./<token>/`. Token (not title) because it is always present,
/// URL-safe, and unique per share — titles can be missing or collide.
pub fn bundle_dest_dir(output: Option<std::path::PathBuf>, token: &str) -> std::path::PathBuf {
    output.unwrap_or_else(|| std::path::PathBuf::from(token))
}

/// One planned write in a bundle download.
#[derive(Debug, PartialEq, Eq)]
pub struct BundleEntry {
    /// Filename exactly as the server reports it (used for the fetch).
    pub filename: String,
    /// `dir/filename` — where the bytes go.
    pub dest: std::path::PathBuf,
}

/// Every destination of a bundle download, resolved before a single byte
/// is fetched, so unsafe names and overwrite conflicts abort the whole
/// download instead of half of it.
#[derive(Debug, PartialEq, Eq)]
pub struct BundlePlan {
    /// Directory containing all planned bundle writes.
    pub dir: std::path::PathBuf,
    /// Every validated file and its resolved destination.
    pub entries: Vec<BundleEntry>,
}

/// Build a [`BundlePlan`]. Fails closed: an empty list, any unsafe filename
/// (see [`safe_share_filename`]) or two files resolving to the same
/// destination is an error naming the cause.
///
/// The duplicate check is defence in depth — the server validates filename
/// uniqueness per share (`ShareFile`), so a collision means the payload
/// disagrees with that invariant. Without it, a duplicate would pass the
/// caller's conflict pre-flight and then either abort mid-bundle or (under
/// `--force`) overwrite the earlier file while the summary still counts both.
pub fn plan_bundle(dir: std::path::PathBuf, filenames: &[String]) -> Result<BundlePlan, String> {
    if filenames.is_empty() {
        return Err("share reports no files to download".into());
    }
    let mut entries: Vec<BundleEntry> = Vec::with_capacity(filenames.len());
    for filename in filenames {
        let safe = safe_share_filename(filename)?;
        let dest = dir.join(safe);
        if let Some(clash) = entries.iter().find(|e| e.dest == dest) {
            return Err(format!(
                "share lists two files that would write to the same path: `{}` and `{}` both resolve to {}",
                clash.filename.escape_debug(),
                filename.escape_debug(),
                dest.display()
            ));
        }
        entries.push(BundleEntry {
            filename: filename.clone(),
            dest,
        });
    }
    Ok(BundlePlan { dir, entries })
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

    #[test]
    fn safe_share_filename_accepts_plain_and_rejects_traversal() {
        assert_eq!(safe_share_filename("report.html").unwrap(), "report.html");
        assert_eq!(safe_share_filename("data.v2.csv").unwrap(), "data.v2.csv");
        assert_eq!(safe_share_filename(".hidden").unwrap(), ".hidden");
        // `:` is legal on macOS/Linux — the only platforms we ship.
        assert_eq!(safe_share_filename("2024:q3.html").unwrap(), "2024:q3.html");
        for bad in [
            "",
            ".",
            "..",
            "a/b.html",
            "../x",
            "a\\b",
            "nul\0byte",
            "/abs.html",
        ] {
            assert!(
                safe_share_filename(bad).is_err(),
                "expected `{bad:?}` to be rejected"
            );
        }
    }

    #[test]
    fn bundle_dest_dir_defaults_to_token_dir() {
        use std::path::PathBuf;
        assert_eq!(bundle_dest_dir(None, "AbC123"), PathBuf::from("AbC123"));
        assert_eq!(
            bundle_dest_dir(Some(PathBuf::from("/tmp/out")), "AbC123"),
            PathBuf::from("/tmp/out")
        );
    }

    #[test]
    fn plan_bundle_maps_every_file_and_fails_closed() {
        use std::path::PathBuf;
        let files = vec!["index.html".to_string(), "styles.css".to_string()];
        let plan = plan_bundle(PathBuf::from("out"), &files).unwrap();
        assert_eq!(plan.dir, PathBuf::from("out"));
        assert_eq!(plan.entries.len(), 2);
        assert_eq!(plan.entries[0].filename, "index.html");
        assert_eq!(plan.entries[0].dest, PathBuf::from("out/index.html"));
        assert_eq!(plan.entries[1].dest, PathBuf::from("out/styles.css"));

        // One bad filename poisons the whole plan — nothing is partially planned.
        let poisoned = vec!["ok.html".to_string(), "../escape.html".to_string()];
        let err = plan_bundle(PathBuf::from("out"), &poisoned).unwrap_err();
        assert!(
            err.contains("../escape.html"),
            "error should name the offending file: {err}"
        );

        // Empty file list is an error, not an empty plan.
        assert!(plan_bundle(PathBuf::from("out"), &[]).is_err());

        // Two files resolving to the same destination abort the plan — without
        // this the conflict pre-flight passes and the write loop collides.
        let dupes = vec!["a.html".to_string(), "a.html".to_string()];
        let err = plan_bundle(PathBuf::from("out"), &dupes).unwrap_err();
        assert!(
            err.contains("same path") && err.contains("a.html"),
            "error should name the collision: {err}"
        );
    }
}
