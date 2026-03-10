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

## Installing

Requires [GitHub CLI](https://cli.github.com/) (`gh`) authenticated with access to this repo.

```bash
# Install all tools
bash <(gh api repos/productiveio/cli-toolbox/contents/scripts/install.sh --jq '.content' | base64 -d) --all

# Install all tools + Claude Code skills
bash <(gh api repos/productiveio/cli-toolbox/contents/scripts/install.sh --jq '.content' | base64 -d) --all --with-skill

# Install a specific tool
bash <(gh api repos/productiveio/cli-toolbox/contents/scripts/install.sh --jq '.content' | base64 -d) tb-prod
```

Or if you have the repo cloned:

```bash
./scripts/install.sh --all --with-skill
```

Binaries are installed to `~/.local/bin`. Make sure it's on your `PATH`:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

The installer compares local vs latest release and only downloads newer versions. Use `--reinstall` to force re-download.

### Claude Code permissions

Each tool ships with a Claude Code skill that runs `tb-<tool> prime` on load to inject live context. By default this triggers a permission prompt. To allow it automatically, add these to `~/.claude/settings.json` under `permissions.allow`:

```json
"Bash(tb-bug:*)",
"Bash(tb-lf:*)",
"Bash(tb-prod:*)",
"Bash(tb-sem:*)"
```

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
