# Build CI Pipeline

**Status:** Ready
**Last updated:** 2026-03-08

## Summary

Add a complete CI/CD pipeline to cli-toolbox: a GitHub Actions workflow that builds and releases binaries on tag push, a shell script for installing/updating binaries locally, and a runtime version check so users know when updates are available.

## Requirements

### 1. GitHub Actions Workflow

- Trigger on tag push matching `tb-*-v*` pattern (e.g. `tb-prod-v0.1.0`)
- Also support manual `workflow_dispatch` with a `dry_run` flag (runs checks + build, skips release)
- **check job** (ubuntu-latest): `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`
- **build job** (matrix, depends on check): build the tagged crate for two targets:
  - `aarch64-apple-darwin` on `macos-latest` → artifact `<crate>-macos-arm64`
  - `x86_64-unknown-linux-gnu` on `ubuntu-latest` → artifact `<crate>-linux-x86_64`
  - Use `cargo build --release -p <crate> --target <target>`
  - Upload artifacts via `actions/upload-artifact@v4`
- **release job** (depends on build, only on tag push + not dry_run): download artifacts, create GitHub Release with `gh release create <tag> --generate-notes`, upload binaries
- Use `Swatinem/rust-cache@v2` in check and build jobs
- Parse crate name from tag: `tag="${GITHUB_REF#refs/tags/}"`, `crate="${tag%-v*}"`

### 2. rust-toolchain.toml

- Add to repo root
- `channel = "stable"` with components `clippy` and `rustfmt`
- No pinned version — always uses latest stable

### 3. Install/Update Script (`scripts/install.sh`)

- Install a single tool: `scripts/install.sh tb-prod`
- Install all tools: `scripts/install.sh --all`
- Flags:
  - `--reinstall` — force download even if local version matches latest
  - `--with-skill` — run `<tool> skill install --force` after installing the binary
- Behavior:
  1. Detect platform via `uname -s` + `uname -m` → map to artifact suffix (`macos-arm64` or `linux-x86_64`)
  2. For each tool to install:
     a. Query GitHub API for latest release matching `<tool>-v*` tag
     b. If tool exists locally, compare `<tool> --version` output with latest release version
     c. If versions match and `--reinstall` not set, skip with "already up to date" message
     d. Download binary from release assets to `~/.local/bin/<tool>`
     e. `chmod +x` the binary
     f. If `--with-skill` set, run `<tool> skill install --force`
  3. Print summary of what was installed/skipped
- Use `curl` for downloads (no `gh` CLI dependency for the install script — users may not have it)
- GitHub API: `https://api.github.com/repos/productiveio/cli-toolbox/releases` — filter by tag_name prefix
- Error handling: fail clearly if platform unsupported, network unavailable, or binary not found in release

### 4. Version Bump Script (`scripts/bump.sh`)

- Usage: `scripts/bump.sh <tool> <version>`
- Steps:
  1. Validate tool name is one of: tb-prod, tb-sem, tb-bug, tb-lf
  2. Update `version = "..."` in `crates/<tool>/Cargo.toml`
  3. Run `cargo check -p <tool>` to verify (also updates Cargo.lock)
  4. Commit: `<tool>: bump version to <version>`
  5. Tag: `git tag <tool>-v<version>`
  6. Print instruction to push: `git push && git push --tags`
- Does NOT push automatically — user confirms

### 5. Runtime Version Check (toolbox-core)

- New module in `toolbox-core`: `version_check`
- Shared by all 4 binaries
- Queries GitHub API: `GET https://api.github.com/repos/productiveio/cli-toolbox/releases` filtered by tag prefix
- **Cache**: store last check result in `~/.cache/<tool>/version-check.json` with timestamp; skip API call if checked within last 24 hours
- **Output**: if a newer version exists, print a single line to stderr: `Update available: <tool> <current> → <latest> (run scripts/install.sh <tool>)`
- **Where it runs**:
  - On `prime` command (natural session start, already makes network calls)
  - On `--version` flag (user explicitly checking version)
- **Failure handling**: never block or error on version check failure — silently skip if network unavailable, API errors, or timeout (2s max)
- Feature-gated: add to `toolbox-core` `version-check` feature, opt-in per binary

### 6. Local Checks (cargo fmt)

- Add `cargo fmt --check` to the CI check job
- Update any local check scripts/skill instructions to include fmt alongside clippy and test

## Non-goals

- **Windows builds** — no Windows target in the build matrix
- **crates.io publishing** — these are internal tools, not public crates
- **Homebrew tap** — install script is sufficient for now
- **Auto-update / self-replacing binary** — pull-based check only; user runs install script manually
- **Intel Mac builds** — `x86_64-apple-darwin` not included; Apple Silicon only
- **Cross-compilation** — each target builds natively on its matching runner
- **Workspace-level version sync** — each crate is versioned and released independently
- **Changelog generation** — `gh release create --generate-notes` from commit messages is sufficient

## Technical approach

### Workflow file: `.github/workflows/release.yml`

Single workflow with three sequential jobs. The `prepare` step (in check job or as a separate job) extracts the crate name from the tag and passes it as an output to downstream jobs.

```yaml
# Pseudostructure
on:
  push:
    tags: ["tb-*-v*"]
  workflow_dispatch:
    inputs:
      dry_run: { type: boolean, default: true }

jobs:
  check:    # fmt, clippy, test (full workspace)
  build:    # matrix: 2 targets, builds only the tagged crate
  release:  # creates GitHub Release, uploads binaries
```

### Install script: `scripts/install.sh`

Bash script. Uses `curl` for GitHub API and downloads. The `--all` flag iterates over a hardcoded list of tools (`tb-prod tb-sem tb-bug tb-lf`).

### Version check module

Lives in `toolbox-core/src/version_check.rs`. Uses `reqwest` (already a workspace dependency) with a 2s timeout. Cache file format:

```json
{
  "latest_version": "0.2.0",
  "checked_at": "2026-03-08T12:00:00Z"
}
```

Each binary calls `version_check::check("tb-prod", env!("CARGO_PKG_VERSION"))` at the appropriate points. The function handles caching, API calls, and stderr output internally.

### Key decisions

| Decision | Choice | Why |
|----------|--------|-----|
| Install location | `~/.local/bin/` | No sudo needed; standard user-local bin path |
| Version check cache | 24h TTL | Avoids hitting GitHub API every session |
| Version check output | stderr | Doesn't interfere with piped/parsed stdout |
| Install script deps | curl only | More portable than requiring `gh` CLI |
| rust-toolchain.toml | stable (unpinned) | Ensures clippy+rustfmt available without version maintenance burden |
| Bump script | Does not auto-push | Safety — user reviews and pushes manually |

## Open questions

None — all resolved.
