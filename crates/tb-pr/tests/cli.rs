//! End-to-end CLI smoke tests — no network, no fixtures. Exists mainly to
//! prove the binary links and the top-level command tree stays wired.

// `cargo_bin` is marked deprecated in recent assert_cmd but remains the
// canonical entry point for integration tests on a standard cargo layout;
// the replacement lives in a separate crate we don't want to pull in.
#![allow(deprecated)]

use assert_cmd::Command;
use predicates::str::contains;

fn bin() -> Command {
    Command::cargo_bin("tb-pr").expect("tb-pr binary built")
}

#[test]
fn help_mentions_radar_and_core_subcommands() {
    let assert = bin().arg("--help").assert().success();
    assert
        .stdout(contains("GitHub PR radar"))
        .stdout(contains("list"))
        .stdout(contains("show"));
}

#[test]
fn version_flag_prints_name() {
    bin().arg("-V").assert().success().stdout(contains("tb-pr"));
}

#[test]
fn list_help_describes_column_filter() {
    bin()
        .args(["list", "--help"])
        .assert()
        .success()
        .stdout(contains("waiting-on-me"));
}
