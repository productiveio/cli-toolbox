#!/usr/bin/env bash
set -euo pipefail

REPO="productiveio/cli-toolbox"
ALL_TOOLS="tb-prod tb-sem tb-bug tb-lf"
INSTALL_DIR="$HOME/.local/bin"

# --- Flags ---
reinstall=false
with_skill=false
tools=()

usage() {
  cat <<EOF
Usage: $0 [OPTIONS] <tool> [<tool>...]
       $0 [OPTIONS] --all

Install or update cli-toolbox binaries from GitHub releases.

Requires: gh (GitHub CLI) — for authenticated access to the private repo.

Options:
  --all          Install all tools ($ALL_TOOLS)
  --reinstall    Force download even if local version matches latest
  --with-skill   Install Claude Code skill after installing binary
  -h, --help     Show this help

Examples:
  $0 tb-prod                       # Install/update tb-prod
  $0 --all --with-skill            # Install all tools + Claude Code skills
  $0 --reinstall tb-prod tb-lf     # Force reinstall specific tools
EOF
  exit 0
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --all)       tools=($ALL_TOOLS); shift ;;
    --reinstall) reinstall=true; shift ;;
    --with-skill) with_skill=true; shift ;;
    -h|--help)   usage ;;
    -*)          echo "Unknown option: $1"; usage ;;
    *)           tools+=("$1"); shift ;;
  esac
done

if [[ ${#tools[@]} -eq 0 ]]; then
  echo "Error: specify at least one tool or use --all"
  echo ""
  usage
fi

# --- Prerequisites ---
if ! command -v gh &>/dev/null; then
  echo "Error: gh (GitHub CLI) is required but not installed"
  echo "Install it: https://cli.github.com/"
  exit 1
fi

# --- Platform detection ---
detect_platform() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Darwin) os="macos" ;;
    Linux)  os="linux" ;;
    *)      echo "Error: unsupported OS: $os"; exit 1 ;;
  esac

  case "$arch" in
    arm64|aarch64) arch="arm64" ;;
    x86_64)        arch="x86_64" ;;
    *)             echo "Error: unsupported architecture: $arch"; exit 1 ;;
  esac

  echo "${os}-${arch}"
}

PLATFORM="$(detect_platform)"
echo "Platform: $PLATFORM"
echo "Install dir: $INSTALL_DIR"
echo ""

# Ensure install dir exists and is on PATH
mkdir -p "$INSTALL_DIR"
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
  echo "Warning: $INSTALL_DIR is not on your PATH"
  echo "Add this to your shell profile:"
  echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
  echo ""
fi

# --- GitHub API helpers (using gh for auth) ---
get_latest_release() {
  local tool="$1"
  gh api "repos/$REPO/releases" --jq \
    "[.[] | select(.tag_name | startswith(\"${tool}-v\")) | select(.draft == false) | select(.prerelease == false)] | .[0].tag_name // empty" \
    2>/dev/null | sed "s/^${tool}-v//"
}

download_asset() {
  local tool="$1" version="$2" platform="$3" dest="$4"
  local tag="${tool}-v${version}"
  local asset="${tool}-${platform}"
  gh release download "$tag" --repo "$REPO" --pattern "$asset" --output "$dest" --clobber
}

get_local_version() {
  local tool="$1"
  if command -v "$tool" &>/dev/null; then
    "$tool" --version 2>/dev/null | awk '{print $2}'
  else
    echo ""
  fi
}

# --- Install loop ---
installed=()
skipped=()
failed=()

for tool in "${tools[@]}"; do
  # Validate tool name
  if ! echo "$ALL_TOOLS" | grep -qw "$tool"; then
    echo "[$tool] Error: unknown tool (valid: $ALL_TOOLS)"
    failed+=("$tool")
    continue
  fi

  echo "[$tool] Checking for latest release..."

  latest="$(get_latest_release "$tool")"
  if [[ -z "$latest" ]]; then
    echo "[$tool] No release found — skipping"
    failed+=("$tool")
    continue
  fi

  local_version="$(get_local_version "$tool")"
  echo "[$tool] Local: ${local_version:-not installed} | Latest: $latest"

  if [[ "$local_version" == "$latest" ]] && [[ "$reinstall" == false ]]; then
    echo "[$tool] Already up to date"
    skipped+=("$tool")
    continue
  fi

  echo "[$tool] Downloading ${tool}-${PLATFORM} from release ${tool}-v${latest}..."
  if download_asset "$tool" "$latest" "$PLATFORM" "$INSTALL_DIR/$tool"; then
    chmod +x "$INSTALL_DIR/$tool"
    echo "[$tool] Installed $latest to $INSTALL_DIR/$tool"
    installed+=("$tool")

    if [[ "$with_skill" == true ]]; then
      echo "[$tool] Installing Claude Code skill..."
      "$INSTALL_DIR/$tool" skill install --force 2>&1 || echo "[$tool] Warning: skill install failed"
    fi
  else
    echo "[$tool] Error: download failed"
    failed+=("$tool")
  fi

  echo ""
done

# --- Summary ---
echo "=== Summary ==="
[[ ${#installed[@]} -gt 0 ]] && echo "Installed: ${installed[*]}"
[[ ${#skipped[@]} -gt 0 ]]   && echo "Up to date: ${skipped[*]}"
[[ ${#failed[@]} -gt 0 ]]    && echo "Failed: ${failed[*]}"

[[ ${#failed[@]} -gt 0 ]] && exit 1
exit 0
