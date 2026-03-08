# Build CI Pipeline

## Problem

The cli-toolbox workspace has four independent binaries (tb-prod, tb-sem, tb-bug, tb-lf) with no CI/CD pipeline. The README describes a tag-triggered release flow but it doesn't exist yet. Building, testing, and distributing binaries is entirely manual — users must clone the repo and `cargo install` from source.

## What we're building

Three interconnected pieces:

1. **GitHub Actions pipeline** — tag-triggered CI that runs checks (fmt, clippy, test), builds release binaries (macOS ARM, Linux x86_64), and creates a GitHub Release with the binaries attached. Tags follow the existing `<crate>-v<version>` convention from the README. Modeled after the langfuse-insights workflow.

2. **Install/update script** (`scripts/install.sh`) — a shell script users can run to:
   - Check if a binary exists locally and its version
   - Check GitHub for a newer release
   - Download and install the new binary if available
   - Support `--reinstall` to force reinstall
   - Optionally install the Claude Code skill for the binary (gated behind a flag like `--with-skill`)

3. **Version check in the binary** — notify users when a newer version is available, either on `--version` or during the `prime` command, so they know to update without having to check manually.

## Scope

**In:**
- GitHub Actions workflow (check + build + release, per-crate tags)
- Version bump helper (script or cargo-release config)
- Install/update shell script with skill installation support
- Runtime version check against GitHub releases

**Out:**
- Windows builds
- crates.io publishing
- Homebrew tap
- Auto-update (pull-based check only, no self-replacing binary)
