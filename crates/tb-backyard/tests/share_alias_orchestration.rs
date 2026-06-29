//! Orchestration logic for `tb-backyard share alias set/list/rm`. The HTTP layer
//! has no mock infrastructure in this crate (no wiremock/mockito), so these
//! tests exercise the pure decision functions that drive every orchestration
//! branch: target parsing, the INV-5 opt-in gate, and the create-vs-repoint
//! discriminator. Adding wiremock just for one feature would be heavier than
//! the value it adds — these branches are where the bugs live.

use tb_backyard::share_alias::{OptInGate, opt_in_gate, parse_share_target};

// --- INV-5 opt-in gate (full transition matrix) ---

#[test]
fn create_branch_into_private_target_no_gate() {
    // was_unlisted = false (no existing alias), becomes_unlisted = false
    assert_eq!(opt_in_gate(false, false), OptInGate::None);
}

#[test]
fn into_unlisted_from_non_unlisted_prompts() {
    // Single arm in `opt_in_gate`: covers BOTH create-into-unlisted (no
    // prior alias, was_unlisted=false) and repoint-from-private-to-unlisted.
    // Same risk surface, same gate.
    assert_eq!(opt_in_gate(false, true), OptInGate::PromptUnlistedTarget);
}

#[test]
fn repoint_from_unlisted_to_private_emits_de_escalation_notice() {
    // Strictly safer; no prompt, but the user should know
    // non-logged-in viewers will lose access.
    assert_eq!(opt_in_gate(true, false), OptInGate::DeEscalationNotice);
}

#[test]
fn repoint_unlisted_to_unlisted_no_gate() {
    // Already-public URL pointing at a different already-public target.
    // Nothing materially changes.
    assert_eq!(opt_in_gate(true, true), OptInGate::None);
}

// --- Token parsing (`<share-target>` argument) ---

#[test]
fn parses_bare_token() {
    assert_eq!(parse_share_target("AbCdE-12_xy").unwrap(), "AbCdE-12_xy");
}

#[test]
fn parses_full_backyard_url() {
    assert_eq!(
        parse_share_target("https://backyard.productive.io/s/AbCdE_xy").unwrap(),
        "AbCdE_xy"
    );
}

#[test]
fn parses_url_with_trailing_slash() {
    assert_eq!(
        parse_share_target("https://backyard.productive.io/s/AbCdE_xy/").unwrap(),
        "AbCdE_xy"
    );
}

#[test]
fn parses_bundle_subpath_url() {
    // Bundles serve files at /s/<token>/<filename> — `set` should still
    // accept that and extract the leading token.
    assert_eq!(
        parse_share_target("https://backyard.productive.io/s/AbCdE_xy/report.html").unwrap(),
        "AbCdE_xy"
    );
}

#[test]
fn parses_localhost_url() {
    // The CLI hits whatever `backyard_url` is configured — accept
    // localhost / staging URLs too.
    assert_eq!(
        parse_share_target("http://localhost:3000/s/dev123").unwrap(),
        "dev123"
    );
}

#[test]
fn rejects_url_without_s_segment() {
    assert!(parse_share_target("https://example.com/foo").is_err());
}

#[test]
fn rejects_empty_or_whitespace() {
    assert!(parse_share_target("").is_err());
    assert!(parse_share_target("   ").is_err());
    assert!(parse_share_target("\t\n").is_err());
}

#[test]
fn rejects_token_with_path_separator() {
    // Bare input is anything that contains no `/` — anything else must
    // be a URL with `/s/`.
    assert!(parse_share_target("token/extra").is_err());
}

#[test]
fn trims_outer_whitespace() {
    assert_eq!(parse_share_target("  AbCdE_xy  ").unwrap(), "AbCdE_xy");
}
