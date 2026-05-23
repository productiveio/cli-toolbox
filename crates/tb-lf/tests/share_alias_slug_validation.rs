//! Slug regex parity test — these cases MUST match
//! `devportal/spec/models/share_alias_spec.rb` exactly. Any diff here
//! signals drift between the server's `ShareAlias::SLUG_REGEX` /
//! `RESERVED_SLUGS` and the CLI port in `tb_lf::share_alias`.

use tb_lf::share_alias::{RESERVED_SLUGS, normalize_slug, validate_slug};

#[test]
fn accepts_well_formed_slugs() {
    let valid = [
        "weekly-team-report",
        "a",
        "1",
        "ab",
        "a-b",
        "a-b-c",
        "abc123",
        "abc-123",
        "123-abc",
        "a-1-b-2",
        // 64-char boundary (1 + 62 mid + 1 = 64)
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ];
    for s in valid {
        assert!(s.len() <= 64, "test bug: `{}` too long", s);
        assert!(
            validate_slug(s).is_ok(),
            "expected `{}` to be valid, got: {:?}",
            s,
            validate_slug(s).err()
        );
    }
}

#[test]
fn rejects_malformed_slugs() {
    let invalid = [
        ("", "empty"),
        ("-leading", "leading hyphen"),
        ("trailing-", "trailing hyphen"),
        ("foo--bar", "consecutive hyphens"),
        ("with spaces", "space"),
        ("under_score", "underscore"),
        ("ABC", "uppercase (un-normalized)"),
        ("a/b", "slash"),
        ("a.b", "dot"),
        ("á", "non-ascii"),
        // 65-char boundary
        (
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "too long",
        ),
    ];
    for (s, label) in invalid {
        assert!(
            validate_slug(s).is_err(),
            "expected `{}` ({}) to be invalid",
            s,
            label
        );
    }
}

#[test]
fn rejects_reserved_slugs() {
    for reserved in RESERVED_SLUGS {
        assert!(
            validate_slug(reserved).is_err(),
            "expected reserved `{}` to be rejected",
            reserved
        );
    }
}

#[test]
fn reserved_list_matches_server() {
    // Mirror of `ShareAlias::RESERVED_SLUGS` in devportal
    // `app/models/share_alias.rb` — drift detector.
    let expected = [
        "new", "edit", "delete", "index", "show", "create", "update", "destroy",
    ];
    assert_eq!(RESERVED_SLUGS, &expected);
}

#[test]
fn normalize_lowercases_and_trims() {
    // Mirrors the Ruby spec: "lowercases and trims slug before validation".
    assert_eq!(normalize_slug("  Weekly-Report  "), "weekly-report");
    assert_eq!(normalize_slug("ABC"), "abc");
    assert_eq!(normalize_slug("\tmixed-CASE\n"), "mixed-case");
    assert_eq!(normalize_slug("already-good"), "already-good");
}
