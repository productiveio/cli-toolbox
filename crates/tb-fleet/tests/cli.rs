// `cargo_bin` is marked deprecated in recent assert_cmd but remains the
// canonical entry point for integration tests on a standard cargo layout;
// the replacement lives in a separate crate we don't want to pull in.
#![allow(deprecated)]

use assert_cmd::Command;

// `list --json` must always exit 0 and emit a JSON array, even with no sessions.
#[test]
fn list_json_is_valid_array() {
    let out = Command::cargo_bin("tb-fleet")
        .unwrap()
        .args(["list", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid json");
    assert!(v.is_array());
}
