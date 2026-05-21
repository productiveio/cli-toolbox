//! Pure logic for the `tb-lf share alias` CLI subcommands. Validation,
//! parsing, and the INV-5 opt-in decision live here so they're unit-testable
//! without touching HTTP. Orchestration (wiring these into clap +
//! `DevPortalClient`) sits in `main.rs` next to `share_upload`.
//!
//! The slug rules MUST stay in lockstep with the server's
//! `ShareAlias::SLUG_REGEX` and `RESERVED_SLUGS`
//! (devportal `app/models/share_alias.rb`). The parity unit test in
//! `tests/share_alias_slug_validation.rs` is the drift detector.

/// Reserved words — must match `ShareAlias::RESERVED_SLUGS` on the server.
pub const RESERVED_SLUGS: &[&str] = &[
    "new", "edit", "delete", "index", "show", "create", "update", "destroy",
];

/// Max slug length — server enforces `length: { maximum: 64 }`.
pub const MAX_SLUG_LEN: usize = 64;

/// Lowercase + trim. The server runs the same normalization via
/// `before_validation :normalize_slug`; the CLI does it client-side so the
/// user can see the normalized form before any HTTP call.
pub fn normalize_slug(input: &str) -> String {
    input.trim().to_lowercase()
}

/// Validate a normalized slug against the server's `SLUG_REGEX` + reserved
/// list. Mirrors `\A(?!.*--)[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?\z` without
/// requiring the regex crate (no lookahead support).
pub fn validate_slug(slug: &str) -> Result<(), String> {
    if slug.is_empty() {
        return Err("slug cannot be empty".into());
    }
    if slug.len() > MAX_SLUG_LEN {
        return Err(format!("slug cannot exceed {} characters", MAX_SLUG_LEN));
    }
    if RESERVED_SLUGS.contains(&slug) {
        return Err(format!("slug `{}` is reserved", slug));
    }

    let bytes = slug.as_bytes();
    let is_alnum_lower = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    let is_slug_char = |b: u8| is_alnum_lower(b) || b == b'-';

    if !bytes.iter().all(|&b| is_slug_char(b)) {
        return Err("slug can only contain lowercase letters, digits, and hyphens".into());
    }
    if !is_alnum_lower(bytes[0]) {
        return Err("slug cannot start with a hyphen".into());
    }
    if !is_alnum_lower(bytes[bytes.len() - 1]) {
        return Err("slug cannot end with a hyphen".into());
    }
    if slug.contains("--") {
        return Err("slug cannot contain consecutive hyphens".into());
    }
    Ok(())
}

/// Extract a share token from either a bare token (`AbCdE...`) or a URL
/// ending in `/s/<token>` (with or without trailing slash).
pub fn parse_share_target(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("share target is empty".into());
    }

    let token = if let Some(idx) = trimmed.rfind("/s/") {
        let rest = &trimmed[idx + 3..];
        rest.trim_end_matches('/')
            .split('/')
            .next()
            .unwrap_or("")
            .to_string()
    } else if trimmed.contains('/') {
        return Err(format!(
            "share target `{}` looks like a URL but doesn't contain `/s/<token>`",
            trimmed
        ));
    } else {
        trimmed.to_string()
    };

    if token.is_empty() {
        return Err("share target token is empty".into());
    }
    let token_ok = token
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if !token_ok {
        return Err(format!(
            "share target token contains invalid characters: `{}`",
            token
        ));
    }
    Ok(token)
}

/// What the CLI must do about a visibility transition on `set`. The server
/// does NOT enforce the unlisted opt-in — the CLI gate (TTY prompt or
/// `--force`) is the entire surface (INV-5).
#[derive(Debug, PartialEq, Eq)]
pub enum OptInGate {
    /// No transition to a more-public state; proceed.
    None,
    /// Target is/becomes `unlisted` and it wasn't before (create-branch or
    /// repoint from private). Must prompt on TTY or require `--force`.
    PromptUnlistedTarget,
    /// Repoint away from `unlisted` to `private`. Emit a single stderr
    /// notice; no prompt.
    DeEscalationNotice,
}

/// `was_unlisted = false` for create-branch (no existing alias).
pub fn opt_in_gate(was_unlisted: bool, becomes_unlisted: bool) -> OptInGate {
    match (was_unlisted, becomes_unlisted) {
        (false, true) => OptInGate::PromptUnlistedTarget,
        (true, false) => OptInGate::DeEscalationNotice,
        _ => OptInGate::None,
    }
}

/// Verbatim copy from chunk #49 / chunk #62. Used at the TTY prompt and in
/// the `--force` failure message so server and CLI surfaces stay in sync.
pub const UNLISTED_OPT_IN_COPY: &str = "This alias points at an `unlisted` share. \
     The resulting URL `/u/<user_id>/<slug>` is intentionally guessable — \
     anyone who guesses both segments can view the share without logging in. \
     Proceed?";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_lowercases_and_trims() {
        assert_eq!(normalize_slug("  Weekly-Report  "), "weekly-report");
        assert_eq!(normalize_slug("ABC"), "abc");
        assert_eq!(normalize_slug("already-good"), "already-good");
    }

    #[test]
    fn opt_in_matrix() {
        assert_eq!(opt_in_gate(false, false), OptInGate::None);
        assert_eq!(opt_in_gate(true, true), OptInGate::None);
        assert_eq!(opt_in_gate(false, true), OptInGate::PromptUnlistedTarget);
        assert_eq!(opt_in_gate(true, false), OptInGate::DeEscalationNotice);
    }

    #[test]
    fn parse_target_bare_token() {
        assert_eq!(parse_share_target("AbCdE-12_xy").unwrap(), "AbCdE-12_xy");
    }

    #[test]
    fn parse_target_full_url() {
        assert_eq!(
            parse_share_target("https://devportal.productive.io/s/AbCdE_xy").unwrap(),
            "AbCdE_xy"
        );
    }

    #[test]
    fn parse_target_trailing_slash() {
        assert_eq!(
            parse_share_target("https://devportal.productive.io/s/AbCdE_xy/").unwrap(),
            "AbCdE_xy"
        );
    }

    #[test]
    fn parse_target_bundle_subpath_keeps_first_segment() {
        assert_eq!(
            parse_share_target("https://devportal.productive.io/s/AbCdE_xy/report.html").unwrap(),
            "AbCdE_xy"
        );
    }

    #[test]
    fn parse_target_rejects_bare_url_without_s_segment() {
        assert!(parse_share_target("https://example.com/foo").is_err());
    }

    #[test]
    fn parse_target_rejects_empty() {
        assert!(parse_share_target("").is_err());
        assert!(parse_share_target("   ").is_err());
    }
}
