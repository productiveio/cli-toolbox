# cli-toolbox

A Cargo workspace monorepo containing CLI tools built for internal use at [Productive.io](https://productive.io). These tools are primarily consumed by AI agents (Claude Code) but work equally well for humans.

## Crates

| Crate | Binary | Description |
|-------|--------|-------------|
| `tb-backyard` | `tb-backyard` | Productive Backyard CLI — AI insights, evals, document shares, and friction logging |
| `tb-sem` | `tb-sem` | Semaphore CI insights — pipeline and job analysis |
| `tb-bug` | `tb-bug` | Bugsnag insights — error and stability analysis |
| `tb-devctl` | `tb-devctl` | Local dev environment orchestrator for Productive services |
| `tb-session` | `tb-session` | Claude Code session search — full-text index and resume past conversations |
| `tb-pr` | `tb-pr` | GitHub PR radar — kanban TUI + CLI for tracking PRs needing your attention |
| `toolbox-core` | (library) | Shared infrastructure: config, HTTP, output formatting, time parsing |

> **Deprecated:** `tb-lf` (Langfuse/DevPortal insights) is superseded by `tb-backyard` — same commands, current Backyard auth. It still installs and runs but prints a deprecation notice; run `tb-lf uninstall` once you've switched. It will be dropped from the install/release flow after the `v0.9.0` self-uninstall has propagated. Source stays in `crates/tb-lf`.

> **Retired:** `tb-prod` (Productive.io CLI) is no longer published or installed via this toolbox. We use the official Productive MCP connected to the Claude org account (`mcp__claude_ai_Productive__*`) instead. The crate source is kept in `crates/tb-prod` for reference and is not part of the workspace install/release flow.

## Naming convention

All binaries use the `tb-{domain}` prefix to signal they belong to this toolbox and to avoid name collisions with official CLIs.

## Installing

```bash
# Install all tools
curl -fsSL https://raw.githubusercontent.com/productiveio/cli-toolbox/main/scripts/install.sh | bash -s -- --all

# Install all tools + Claude Code skills
curl -fsSL https://raw.githubusercontent.com/productiveio/cli-toolbox/main/scripts/install.sh | bash -s -- --all --with-skill

# Install a specific tool
curl -fsSL https://raw.githubusercontent.com/productiveio/cli-toolbox/main/scripts/install.sh | bash -s -- tb-backyard
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

### Configuration

After installing, each tool needs an API token from its respective service. Run `tb-<tool> config init` for each — it's interactive, validates the token, and auto-detects org/project settings.

Grab your token from:

- **tb-sem** — [Semaphore CI](https://semaphoreci.com) → click your avatar → Profile Settings → API Token
- **tb-bug** — [Bugsnag](https://app.bugsnag.com) → Settings → My account → Personal auth tokens
- **tb-backyard** — uses your Productive token automatically via `PRODUCTIVE_AUTH_TOKEN` (already set if you use the Productive MCP / `load-secrets`), against `backyard.productive.io`. No manual token step; override with `BACKYARD_TOKEN` or `tb-backyard config init`.

Config files live in your platform config dir — `~/.config/tb-<tool>/config.toml` on Linux, `~/Library/Application Support/tb-<tool>/config.toml` on macOS. Run `tb-<tool> doctor` to verify connectivity.

### Claude Code permissions

Each tool ships with a Claude Code skill that runs `tb-<tool> prime` on load to inject live context. By default this triggers a permission prompt. To allow it automatically, add these to `~/.claude/settings.json` under `permissions.allow`:

```json
"Bash(tb-backyard:*)",
"Bash(tb-bug:*)",
"Bash(tb-lf:*)",
"Bash(tb-pr:*)",
"Bash(tb-sem:*)",
"Bash(tb-session:*)"
```

## Building

```bash
# Check all crates
cargo check --workspace

# Build a specific binary
cargo build --release -p tb-sem

# Run a specific binary
cargo run -p tb-sem -- --version
```

## Releasing

Each crate is versioned and released independently. Tag convention: `<crate>-v<version>`.

```bash
git tag tb-sem-v0.1.0
git push origin tb-sem-v0.1.0
```

Tags trigger CI to build platform binaries (macOS ARM, Linux x86_64) and create a GitHub release.
