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

```bash
# Install all tools
curl -fsSL https://raw.githubusercontent.com/productiveio/cli-toolbox/main/scripts/install.sh | bash -s -- --all

# Install all tools + Claude Code skills
curl -fsSL https://raw.githubusercontent.com/productiveio/cli-toolbox/main/scripts/install.sh | bash -s -- --all --with-skill

# Install a specific tool
curl -fsSL https://raw.githubusercontent.com/productiveio/cli-toolbox/main/scripts/install.sh | bash -s -- tb-prod
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

- **tb-prod** — [Productive.io](https://app.productive.io) → click your avatar → Profile settings → API access
- **tb-sem** — [Semaphore CI](https://semaphoreci.com) → click your avatar → Profile Settings → API Token
- **tb-bug** — [Bugsnag](https://app.bugsnag.com) → Settings → My account → Personal auth tokens
- **tb-lf** — [DevPortal](https://devportal.productive.io) → AI Tools → click your avatar → API Tokens

Config files are stored in `~/.config/tb-<tool>/config.toml`. Run `tb-<tool> doctor` to verify connectivity.

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
