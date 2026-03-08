# Research: Build CI Pipeline

## Current State

- No `.github/` directory, no CI exists
- 4 binaries (tb-prod, tb-sem, tb-bug, tb-lf) + 1 library (toolbox-core), all at v0.1.0
- All use edition 2024, no `rust-toolchain.toml`
- README documents tag convention: `<crate>-v<version>` (e.g. `tb-prod-v0.1.0`)
- No build.rs in any crate, no special build requirements
- `rusqlite` uses `bundled` feature (compiles SQLite from source, no system deps)
- `rvcr` is a git dev-dependency (needs network for tests)

## Reference: langfuse-insights Workflow

Three sequential jobs: `check` → `build` (matrix) → `release`.

- **check**: cargo check, test, clippy on ubuntu-latest with rust-cache
- **build**: matrix of aarch64-apple-darwin (macos-latest) + x86_64-unknown-linux-gnu (ubuntu-latest), uploads artifacts
- **release**: runs only on `v*` tag push (not dry_run), creates GitHub Release with `gh release create --generate-notes`, uploads binaries

Also supports `workflow_dispatch` with `dry_run` boolean for CI-only runs.

## Key Adaptations for Monorepo

### Tag Parsing

Given tag `tb-prod-v0.1.0`:
```bash
tag="${GITHUB_REF#refs/tags/}"    # tb-prod-v0.1.0
crate="${tag%-v*}"                 # tb-prod
```

### Workflow Trigger

```yaml
push:
  tags: ["tb-*-v*"]
```

This matches all four binaries' tags. The `prepare` job parses the crate name.

### Build

- `cargo build --release -p $crate --target $target` (not whole workspace)
- Artifact names: `<crate>-macos-arm64`, `<crate>-linux-x86_64`
- Check job can still run `cargo clippy --workspace` / `cargo test --workspace`

### No Cross-Compilation Needed

- macOS ARM: native on `macos-latest` (Apple Silicon)
- Linux x86_64: native on `ubuntu-latest`

## Install Script Design

### Binary Installation

- Detect platform (uname -s + uname -m) → map to artifact suffix
- Determine latest release from GitHub API: `gh api repos/OWNER/REPO/releases` filtered by tag prefix
- Compare local version (`<tool> --version`) with latest release version
- Download binary from release assets, install to a known path (e.g. `~/.local/bin/`)
- `--reinstall` flag to force download even if versions match

### Skill Installation

- The `skill install` command writes `~/.claude/skills/<tool>/SKILL.md`
- Content is embedded in the binary via `include_str!`
- After installing a new binary, the script can just run `<tool> skill install --force`
- Gate behind `--with-skill` flag

### Version Check in Binary

GitHub API endpoint: `GET /repos/{owner}/{repo}/releases` — filter by tag prefix matching the binary name.

Options for runtime check:
1. **On `--version`**: print current version, then check GitHub for latest (with timeout/cache)
2. **On `prime`**: since `prime` already makes network calls, add a version check there
3. **Cache the check**: store last-checked timestamp in `~/.cache/<tool>/version-check.json`, skip if checked within last 24h

Recommendation: implement in `toolbox-core` as a shared module since all 4 binaries need it. Use the cache module (already exists with feature gate). Check on `prime` since that's the natural "session start" command, and on explicit `--version`.

## Version Bump Workflow

Manual process (no cargo-release needed for now):
1. Edit `crates/<tool>/Cargo.toml` version
2. Commit: `<tool>: bump version to X.Y.Z`
3. Tag: `git tag <tool>-vX.Y.Z`
4. Push: `git push && git push --tags`
5. GitHub Actions triggers, builds, creates release

Could add a `scripts/bump.sh <tool> <version>` helper to automate steps 1-4.

## Open Questions

1. **Install location**: `~/.local/bin/` or `/usr/local/bin/`? Former doesn't need sudo.
2. **GitHub repo**: what's the owner/repo for the API calls? (Need to confirm once pushed)
3. **Should `cargo fmt --check` be part of CI?** (langfuse-insights doesn't include it, but it's common)
4. **Should we add `rust-toolchain.toml`?** Pins Rust version for reproducible builds.
