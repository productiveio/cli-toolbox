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

// Bare `tb-fleet` opens the TUI on a terminal, but a piped stdout (this test, a
// script, an agent) must get the one-shot list and *exit* — not the watch loop.
// Enforced with a timeout so a regression fails the suite instead of hanging it.
#[test]
fn bare_invocation_is_one_shot_when_piped() {
    let bin = assert_cmd::cargo::cargo_bin("tb-fleet");
    let mut child = std::process::Command::new(bin)
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        match child.try_wait().unwrap() {
            Some(status) => {
                assert!(status.success());
                break;
            }
            None if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                panic!("bare `tb-fleet` did not exit — it fell into the watch loop while piped");
            }
            None => std::thread::sleep(std::time::Duration::from_millis(200)),
        }
    }
    let out = child.wait_with_output().unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("FLEET"));
}
