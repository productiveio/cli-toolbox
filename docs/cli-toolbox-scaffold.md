# cli-toolbox — Monorepo Scaffold Guide

This document contains everything needed to scaffold the `cli-toolbox` Cargo workspace monorepo. It consolidates four Rust CLI tools into a single repo with shared infrastructure, independent versioning, and independent releases.

## Overview

| Crate | Binary | Version | Status | Source |
|-------|--------|---------|--------|--------|
| `lfi` | `lfi` | 1.1.3 | Migrate from `productiveio/langfuse-insights` | `~/code/rust/langfuse-insights` |
| `semi` | `semi` | 0.1.0 | Migrate from local | `~/code/rust/semaphore-insights` |
| `papi` | `papi` | 1.0.0 | Migrate from local | `~/code/rust/papi` |
| `bsi` | `bsi` | 0.1.0 | New — build from design doc | `~/code/rust/bugsnag-insights/docs/` |
| `toolbox-core` | (library) | 0.1.0 | New — extract shared code | — |

GitHub org: `productiveio`
Repo name: `cli-toolbox`

## Design Decision: papi as a Single Binary

The Productive.io surface is large — 84 API resources, 15 agent skills, 10 business domains (project management, CRM, resource planning, time tracking, invoicing, etc.). The question: should `papi` be one CLI covering everything, or split into domain-scoped CLIs (`papi-tasks`, `papi-crm`, `papi-bookings`, etc.)?

**Decision: one `papi` binary, with internal domain modules. Don't split.**

### Why not split

**Shared auth eliminates the main benefit.** Every domain-scoped CLI would need its own config with the same token, org_id, and person_id. One binary means configure once.

**The primary consumer is Claude Code, not humans.** The main argument for splitting is discoverability — a CLI with 50 subcommands is overwhelming to browse via `--help`. But Claude Code discovers commands via `papi prime` and the skill file, not by scanning help text. That eliminates the biggest argument for splitting.

**No context window tax.** The skills in ai-agent are split because each skill's instructions + tools eat LLM tokens — you only load what's needed per conversation. A CLI doesn't have that constraint. `papi bookings` costs nothing when you're running `papi tasks`.

### Internal structure

One binary, domain-scoped command modules:

```
papi/
  src/
    commands/
      tasks.rs        # tasks, todos, task_lists, workflows
      projects.rs     # projects, boards, memberships
      crm.rs          # deals, companies, pipelines
      bookings.rs     # bookings, capacity, availability
      time.rs         # time_entries, timers
      invoices.rs     # invoices, payments, expenses
      people.rs       # people, teams, salaries
      pages.rs        # pages, comments, discussions
      services.rs     # services, service_types, pricing
```

Each module is a clap subcommand group:

```
papi tasks [list|create|update|search]
papi deals [list|show|update|search]
papi bookings [list|create|conflicts]
papi time [list|create|delete]
```

### Growth strategy

Don't build all domains upfront. Grow based on what Claude Code actually needs. Current papi covers: tasks, people, projects, teams, availability. The next gaps (from `docs/productive-cli-needs.md`): task creation, task list filtering, all-status search. Add domains as agent skills need them, not because the API supports them.

If splitting ever makes sense later, the modular `commands/` structure makes it trivial — each module is already self-contained. But `git` has 150+ subcommands in one binary and nobody splits it up.

## Directory Structure

```
cli-toolbox/
├── Cargo.toml                    # Workspace root
├── Cargo.lock
├── .github/
│   └── workflows/
│       ├── ci.yml                # PR checks: check + test + clippy (all crates)
│       └── release.yml           # Tag-triggered: build + release (per-crate)
├── crates/
│   ├── toolbox-core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── config.rs         # Profile management (TOML, add/edit/delete/verify)
│   │       ├── output.rs         # --json wrapping, table formatting, --long toggle
│   │       ├── cache.rs          # SQLite lifecycle, sync status, cache clear
│   │       ├── time.rs           # Parse --since 7d, yesterday, ISO 8601, relative
│   │       ├── http.rs           # Base HTTP client: token auth, retry, rate limiting
│   │       └── prime.rs          # Standard prime/prime --mcp output structure
│   ├── lfi/
│   │   ├── Cargo.toml
│   │   └── src/                  # Migrated from langfuse-insights
│   ├── semi/
│   │   ├── Cargo.toml
│   │   └── src/                  # Migrated from semaphore-insights
│   ├── papi/
│   │   ├── Cargo.toml
│   │   └── src/                  # Migrated from papi
│   └── bsi/
│       ├── Cargo.toml
│       └── src/                  # New, built from design doc
└── docs/
    ├── bsi/                      # Migrated from ~/code/rust/bugsnag-insights/docs/
    │   ├── cli-design.md
    │   ├── bugsnag-insights-research.md
    │   └── reference/            # API endpoint docs (errors, events, trends, etc.)
    └── architecture.md           # Workspace conventions, shared patterns
```

## Workspace Root Cargo.toml

```toml
[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.dependencies]
# CLI
clap = { version = "4.5", features = ["derive"] }
clap_complete = "4.5"

# Async + HTTP
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
reqwest = { version = "0.13", features = ["json"] }
reqwest-middleware = "0.5"

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"

# Time
chrono = { version = "0.4", features = ["serde"] }

# Output
colored = "3.0"
indicatif = "0.18"

# Database
rusqlite = { version = "0.38", features = ["bundled"] }

# Error handling
thiserror = "2.0"

# Internal
toolbox-core = { path = "crates/toolbox-core" }

[workspace.dev-dependencies]
assert_cmd = "2"
predicates = "3"
tempfile = "3"
http = "1.3"
rvcr = { git = "https://github.com/robert-sjoblom/rvcr", branch = "main" }
```

## Per-Crate Cargo.toml Pattern

Example for `lfi`:

```toml
[package]
name = "lfi"
version = "1.1.3"
edition = "2024"
description = "Langfuse CLI with local SQLite caching"

[[bin]]
name = "lfi"
path = "src/main.rs"

[lib]
doctest = false

[dependencies]
toolbox-core.workspace = true
clap.workspace = true
clap_complete.workspace = true
tokio.workspace = true
reqwest.workspace = true
reqwest-middleware.workspace = true
serde.workspace = true
serde_json.workspace = true
toml.workspace = true
chrono.workspace = true
colored.workspace = true
indicatif.workspace = true
rusqlite.workspace = true
thiserror.workspace = true

# lfi-specific deps
base64 = "0.22"
urlencoding = "2.1"
csv = "1.4"
textplots = "0.8"
dialoguer = "0.12"
log = "0.4"
env_logger = "0.11"
rand = "0.10"
uuid = { version = "1.0", features = ["v4"] }

[dev-dependencies]
assert_cmd.workspace = true
predicates.workspace = true
tempfile.workspace = true
http.workspace = true
rvcr.workspace = true
```

Example for `papi` (no SQLite):

```toml
[package]
name = "papi"
version = "1.0.0"
edition = "2024"
description = "Productive.io API CLI"

[[bin]]
name = "papi"
path = "src/main.rs"

[lib]
doctest = false

[dependencies]
toolbox-core.workspace = true
clap.workspace = true
clap_complete.workspace = true
tokio.workspace = true
reqwest.workspace = true
reqwest-middleware.workspace = true
serde.workspace = true
serde_json.workspace = true
toml.workspace = true
chrono.workspace = true
colored.workspace = true
indicatif.workspace = true
thiserror.workspace = true

# papi-specific deps
confy = "0.6"
chrono-humanize = "0.2"
html2text = "0.14"

[dev-dependencies]
http.workspace = true
rvcr.workspace = true
```

## CI Workflow (.github/workflows/ci.yml)

Runs on every PR and push to main. Checks all crates.

```yaml
name: CI

on:
  pull_request:
  push:
    branches: [main]

env:
  CARGO_TERM_COLOR: always

jobs:
  check:
    name: Check, Test, Clippy
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: Swatinem/rust-cache@v2

      - name: Check
        run: cargo check --workspace

      - name: Test
        run: cargo test --workspace

      - name: Clippy
        run: cargo clippy --workspace -- -D warnings
```

## Release Workflow (.github/workflows/release.yml)

Tag convention: `<crate>-v<version>` (e.g. `lfi-v1.2.0`, `papi-v1.1.0`, `bsi-v0.1.0`)

Each crate is released independently. The tag prefix determines which binary to build.

```yaml
name: Release

on:
  push:
    tags: ["lfi-v*", "semi-v*", "papi-v*", "bsi-v*"]
  workflow_dispatch:
    inputs:
      dry_run:
        description: "Dry run (build but don't upload)"
        type: boolean
        default: true

permissions:
  contents: write

env:
  CARGO_TERM_COLOR: always

jobs:
  check:
    name: Check, Test, Clippy
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: Swatinem/rust-cache@v2
      - run: cargo check --workspace
      - run: cargo test --workspace
      - run: cargo clippy --workspace -- -D warnings

  prepare:
    name: Determine crate and version
    runs-on: ubuntu-latest
    outputs:
      crate: ${{ steps.parse.outputs.crate }}
      version: ${{ steps.parse.outputs.version }}
      tag: ${{ steps.parse.outputs.tag }}
    steps:
      - name: Parse tag
        id: parse
        run: |
          tag="${GITHUB_REF#refs/tags/}"
          crate=$(echo "$tag" | sed 's/-v.*//')
          version=$(echo "$tag" | sed 's/.*-v//')
          echo "tag=$tag" >> "$GITHUB_OUTPUT"
          echo "crate=$crate" >> "$GITHUB_OUTPUT"
          echo "version=$version" >> "$GITHUB_OUTPUT"
          echo "Building $crate v$version from tag $tag"

  build:
    needs: [check, prepare]
    strategy:
      matrix:
        include:
          - target: aarch64-apple-darwin
            os: macos-latest
            suffix: macos-arm64
          - target: x86_64-unknown-linux-gnu
            os: ubuntu-latest
            suffix: linux-x86_64
    runs-on: ${{ matrix.os }}
    env:
      CRATE: ${{ needs.prepare.outputs.crate }}
    steps:
      - uses: actions/checkout@v4

      - uses: Swatinem/rust-cache@v2
        with:
          key: ${{ matrix.target }}

      - name: Build release binary
        run: cargo build --release --package ${{ env.CRATE }} --target ${{ matrix.target }}

      - name: Rename binary
        run: |
          cp target/${{ matrix.target }}/release/${{ env.CRATE }} \
             ${{ env.CRATE }}-${{ matrix.suffix }}

      - name: Upload artifact
        uses: actions/upload-artifact@v4
        with:
          name: ${{ env.CRATE }}-${{ matrix.suffix }}
          path: ${{ env.CRATE }}-${{ matrix.suffix }}

  release:
    needs: [build, prepare]
    runs-on: ubuntu-latest
    if: startsWith(github.ref, 'refs/tags/') && !(inputs.dry_run || false)
    env:
      TAG: ${{ needs.prepare.outputs.tag }}
    steps:
      - name: Download all artifacts
        uses: actions/download-artifact@v4
        with:
          path: artifacts

      - name: Create release and upload binaries
        env:
          GH_TOKEN: ${{ github.token }}
        run: |
          gh release view "$TAG" --repo "$GITHUB_REPOSITORY" || \
            gh release create "$TAG" --generate-notes --title "$TAG" --repo "$GITHUB_REPOSITORY"
          for file in artifacts/*/*; do
            gh release upload "$TAG" "$file" --repo "$GITHUB_REPOSITORY"
          done
```

## toolbox-core: Shared Infrastructure

Candidates for extraction from existing crates. Don't force — start thin, extract as patterns solidify.

### Immediate extractions (shared across 3+ crates)

```rust
// config.rs — Profile management
// All tools use TOML config with token + org/project IDs.
// Pattern: add/edit/delete/verify/set-default/show
pub struct Profile {
    pub name: String,
    pub token: String,
    pub base_url: String,
    pub extra: HashMap<String, String>,  // org_id, project_id, person_id, etc.
}
pub trait ConfigManager {
    fn load() -> Result<Config>;
    fn active_profile(&self) -> Result<&Profile>;
    fn add_profile(&mut self, profile: Profile) -> Result<()>;
    // ...
}

// output.rs — Formatting
// --json wrapping with metadata, human-readable tables, --long toggle
pub struct JsonEnvelope<T> {
    pub query: serde_json::Value,
    pub count: usize,
    pub total_count: Option<usize>,
    pub results: Vec<T>,
}
pub fn print_table(headers: &[&str], rows: Vec<Vec<String>>, long: bool);

// time.rs — Time argument parsing
// All tools accept: 7d, 24h, 2w, yesterday, today, tomorrow, ISO 8601
pub fn parse_time_arg(input: &str) -> Result<DateTime<Utc>>;
pub fn parse_day_arg(input: &str) -> Result<NaiveDate>;  // yesterday, today, YYYY-MM-DD
pub fn format_relative(dt: DateTime<Utc>) -> String;      // "2h ago", "3d ago"

// http.rs — Base HTTP client
// Token auth header, retry on 429/5xx, rate limit awareness
pub struct BaseClient {
    client: reqwest::Client,
    token: String,
    base_url: String,
}

// cache.rs — SQLite lifecycle (used by lfi, bsi; not semi, papi)
// DB init, migration, sync status tracking, --status flag
pub struct CacheDb { /* rusqlite Connection wrapper */ }
pub fn sync_status(db: &CacheDb) -> Result<SyncStatus>;
```

### Defer for now

- `prime.rs` — Each tool's prime output is too domain-specific to generalize yet
- Shell completions — Each tool has different completable values

## Migration Plan

### Phase 1: Scaffold workspace

1. Create `productiveio/cli-toolbox` repo on GitHub
2. Set up workspace root `Cargo.toml` with shared dependencies
3. Create empty `crates/toolbox-core` with stub `lib.rs`
4. Add CI + release workflows
5. Verify `cargo check --workspace` passes

### Phase 2: Migrate lfi (first mover)

1. Copy `langfuse-insights/src/` → `crates/lfi/src/`
2. Adapt `Cargo.toml` to use workspace dependencies
3. Verify `cargo test -p lfi` passes
4. Extract first shared pieces into `toolbox-core` (time parsing, output formatting)
5. Tag `lfi-v1.2.0`, verify release workflow produces binaries
6. Archive `productiveio/langfuse-insights` repo

### Phase 3: Migrate semi + papi

1. Copy sources into `crates/semi/` and `crates/papi/`
2. Adapt to workspace deps + `toolbox-core` where natural
3. Tag initial releases

### Phase 4: Build bsi

1. Implement from design doc at `docs/bsi/cli-design.md`
2. Use `toolbox-core` from the start
3. Reference API docs in `docs/bsi/reference/`

## Existing Design Docs to Migrate

Copy these into the monorepo's `docs/` directory:

```
~/code/rust/bugsnag-insights/docs/cli-design.md         → docs/bsi/cli-design.md
~/code/rust/bugsnag-insights/docs/bugsnag-insights-research.md → docs/bsi/research.md
~/code/rust/bugsnag-insights/docs/reference/*            → docs/bsi/reference/
~/code/rust/semaphore-insights/docs/semaphore-cli-features.md → docs/semi/ (if exists)
```

## Current Dependency Overlap

Dependencies shared by 3+ crates (good workspace.dependencies candidates):

| Dependency | lfi | semi | papi | bsi (planned) |
|-----------|-----|------|------|---------------|
| clap 4.5 (derive) | x | x | x | x |
| tokio 1 (multi-thread) | x | x | x | x |
| reqwest 0.12-0.13 (json) | x | x | x | x |
| serde 1 (derive) | x | x | x | x |
| serde_json 1 | x | x | x | x |
| toml 0.8-1.0 | x | x | x | x |
| chrono 0.4 (serde) | x | x | x | x |
| colored 3.0 | x | — | x | x |
| indicatif 0.17-0.18 | x | — | x | x |
| thiserror 2.0 | x | x | — | x |
| reqwest-middleware 0.5 | x | — | x | x |
| rusqlite 0.38 (bundled) | x | — | — | x |
| clap_complete 4.5 | x | — | x | x |

Note: `reqwest` version differs (semi uses 0.12, lfi/papi use 0.13). Align to 0.13 during migration. Similarly `toml` (semi/papi use 0.8, lfi uses 1.0) — align to latest.

## Release Cheatsheet

```bash
# Release a specific crate
git tag lfi-v1.2.0
git push origin lfi-v1.2.0

# Dry run (manual trigger in GitHub Actions UI)
# → builds binaries but doesn't upload to release

# Release multiple crates (if needed)
git tag lfi-v1.2.0
git tag papi-v1.1.0
git push origin lfi-v1.2.0 papi-v1.1.0
```

Each tag triggers an independent workflow run that builds only that crate's binary.
