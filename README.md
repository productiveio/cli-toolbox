# cli-toolbox

A Cargo workspace monorepo containing CLI tools built for internal use at [Productive.io](https://productive.io). These tools are primarily consumed by AI agents (Claude Code) but work equally well for humans.

## Crates

| Crate | Binary | Description |
|-------|--------|-------------|
| `tb-lf` | `tb-lf` | Langfuse insights — query and analyze LLM observability data |
| `tb-sem` | `tb-sem` | Semaphore CI insights — pipeline and job analysis |
| `tb-prod` | `tb-prod` | Productive.io CLI — tasks, projects, people, time tracking |
| `tb-bug` | `tb-bug` | Bugsnag insights — error and stability analysis |
| `toolbox-core` | (library) | Shared infrastructure: config, HTTP, output formatting, time parsing |

## Naming convention

All binaries use the `tb-{domain}` prefix to signal they belong to this toolbox and to avoid name collisions with official CLIs.

## Building

```bash
# Check all crates
cargo check --workspace

# Build a specific binary
cargo build --release -p tb-prod

# Run a specific binary
cargo run -p tb-sem -- --version
```

## Releasing

Each crate is versioned and released independently. Tag convention: `<crate>-v<version>`.

```bash
git tag tb-prod-v0.1.0
git push origin tb-prod-v0.1.0
```

Tags trigger CI to build platform binaries (macOS ARM, Linux x86_64) and create a GitHub release.
