# Idea: Rename devctl → tb-devctl

Rename the `devctl` tool to `tb-devctl` everywhere — as if it was always named that way.
The goal is consistency with the other cli-toolbox binaries (`tb-prod`, `tb-sem`, `tb-bug`, `tb-lf`).

**In scope:**
- Binary name: `tb-devctl` (user types `tb-devctl start`)
- Crate package name: `tb-devctl`
- Crate directory: `crates/tb-devctl/`
- Config file: `tb-devctl.toml` (currently `devctl.toml`)
- State directory: `.tb-devctl/` (currently `.devctl/`)
- All user-facing strings, help text, generated comments, error messages
- Tooling: bump.sh, install.sh, release.yml, publish skill

**Out of scope:**
- Nothing — treat this as a full clean rename from the start
