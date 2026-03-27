---
name: cli-toolbox_publish
description: Bump, tag, and release cli-toolbox binaries. Use when the user says "publish", "release", "bump version", or invokes /cli-toolbox_publish.
argument-hint: "<tool|--all> <version> [--install] [--with-skill]"
allowed-tools: Bash, Read, Glob, Grep
---

# Publish

Bump versions, push tags, trigger CI releases, and optionally install locally — handling all the friction points of the cli-toolbox release process.

## Context

This is for the `productiveio/cli-toolbox` workspace. It has 4 independent binaries: `tb-prod`, `tb-sem`, `tb-bug`, `tb-lf`. Each is versioned and released independently using git tags in `<crate>-v<version>` format.

**Critical:** GitHub Actions does NOT trigger individual workflows when multiple tags are pushed in a single `git push --tags`. Tags MUST be pushed one at a time with a small delay between them.

## Step 1: Parse arguments

Parse `$ARGUMENTS` to determine:
- **Which tools** to publish: a specific tool name (e.g., `tb-prod`), multiple names, or `--all` for all 4
- **Version**: the target version (e.g., `0.2.0`). If omitted, ask the user.
- **`--install`**: after releases complete, install binaries locally via `scripts/install.sh`
- **`--with-skill`**: when installing, also install Claude Code skills (passed through to install.sh)

Valid tool names: `tb-prod`, `tb-sem`, `tb-bug`, `tb-lf`, `tb-devctl`

If `--all` is used and no version is specified, read each crate's current version from `crates/<tool>/Cargo.toml` and suggest a patch bump for each. Ask the user to confirm.

## Step 2: Quality gates

Before any version bumps, run quality gates:
1. `cargo fmt --check`
2. `cargo clippy --workspace -- -D warnings`
3. `cargo test --workspace`

If any fail, stop and fix before proceeding.

## Step 3: Bump versions

For each tool to publish, run `scripts/bump.sh <tool> <version>`.

This will:
- Update `crates/<tool>/Cargo.toml`
- Run `cargo check -p <tool>`
- Create a commit: `<tool>: bump version to <version>`
- Create a tag: `<tool>-v<version>`

## Step 4: Push commits

Push all bump commits at once:
```
git push
```

## Step 5: Push tags ONE AT A TIME

**This is the critical step.** Push each tag individually, not with `git push --tags`:

```bash
git push origin refs/tags/<tool>-v<version>
```

Do this for each tool, one at a time. This ensures each tag triggers its own GitHub Actions workflow.

## Step 6: Monitor pipelines

After pushing all tags, monitor the release pipelines:
1. Run `gh run list --limit <n>` to see all triggered runs
2. Wait for all runs to complete
3. Report results — which succeeded, which failed

If a pipeline fails, show the failure details and ask the user how to proceed.

## Step 7: Verify releases

For each tool, verify the GitHub Release was created with binaries:
```
gh release view <tool>-v<version>
```

Report: tool name, version, assets (macos-arm64, linux-x86_64).

## Step 8: Install locally (if --install)

If `--install` was requested, run:
```
scripts/install.sh [--with-skill] <tool1> <tool2> ...
```

Or with `--all`:
```
scripts/install.sh --all [--with-skill]
```

Report installed versions.

## Summary

At the end, print a summary table:

```
=== Published ===
Tool        Version   Release   Installed
tb-prod     0.2.0     ✓         ✓
tb-sem      0.2.0     ✓         —
tb-bug      0.2.0     ✓         ✓
tb-lf       0.2.0     ✓         —
```
