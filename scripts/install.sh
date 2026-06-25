#!/usr/bin/env bash
set -euo pipefail

REPO="productiveio/cli-toolbox"
# tb-lf is DEPRECATED (superseded by tb-backyard) but kept installable so
# existing users get the release that can self-uninstall (`tb-lf uninstall`).
# Remove it from this list once it has propagated.
ALL_TOOLS="tb-sem tb-bug tb-backyard tb-lf tb-devctl tb-session tb-pr"
INSTALL_DIR="$HOME/.local/bin"

# --- Flags ---
reinstall=false
with_skill=false
uninstall=false
purge=false
tools=()

usage() {
  cat <<EOF
Usage: $0 [OPTIONS] <tool> [<tool>...]
       $0 [OPTIONS] --all

Install or update cli-toolbox binaries from GitHub releases.

Note: tb-lf is deprecated — use tb-backyard. Reinstall tb-lf only to pick up
its self-uninstall, then run: $0 --uninstall --purge tb-lf

Requires: curl

Options:
  --all          Install all tools ($ALL_TOOLS)
  --reinstall    Force download even if local version matches latest
  --with-skill   Install Claude Code skill after installing binary
  --uninstall    Remove the tool's skill + config (delegates to `<tool> uninstall`)
  --purge        With --uninstall, also remove the installed binary
  -h, --help     Show this help

Examples:
  $0 tb-backyard                   # Install/update tb-backyard
  $0 --all --with-skill            # Install all tools + Claude Code skills
  $0 --reinstall tb-sem tb-backyard  # Force reinstall specific tools
  $0 --uninstall --purge tb-backyard # Remove tb-backyard entirely
EOF
  exit 0
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --all)       tools=($ALL_TOOLS); shift ;;
    --reinstall) reinstall=true; shift ;;
    --with-skill) with_skill=true; shift ;;
    --uninstall) uninstall=true; shift ;;
    --purge)     purge=true; shift ;;
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

# --- Uninstall (delegates to each tool's own `uninstall` subcommand) ---
if [[ "$uninstall" == true ]]; then
  uninstall_args=()
  [[ "$purge" == true ]] && uninstall_args+=(--purge)
  uninstall_failed=()
  for tool in "${tools[@]}"; do
    bin="$INSTALL_DIR/$tool"
    [[ -x "$bin" ]] || bin="$(command -v "$tool" 2>/dev/null || true)"
    if [[ -z "$bin" ]]; then
      echo "[$tool] not installed — skipping"
      continue
    fi
    echo "[$tool] Uninstalling..."
    "$bin" uninstall "${uninstall_args[@]}" || { echo "[$tool] uninstall failed"; uninstall_failed+=("$tool"); }
  done
  [[ ${#uninstall_failed[@]} -gt 0 ]] && exit 1
  exit 0
fi

# --- Prerequisites ---
if ! command -v curl &>/dev/null; then
  echo "Error: curl is required but not installed"
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

# --- GitHub API helpers (using curl, no auth needed for public repo) ---
# Reads tags rather than the /releases list. The latter has eventual-
# consistency lag — a release published minutes ago can be missing for
# hours, so a freshly-tagged version would silently install the previous
# one. /tags reflects the ref the moment it's pushed.
get_latest_release() {
  local tool="$1"
  curl -fsSL "https://api.github.com/repos/$REPO/tags?per_page=100" \
    | python3 -c "
import sys, json, re
tool = '${tool}'
prefix = tool + '-v'
versions = []
for t in json.load(sys.stdin):
    name = t.get('name', '')
    if not name.startswith(prefix):
        continue
    m = re.match(r'^(\d+)\.(\d+)\.(\d+)\$', name[len(prefix):])
    if m:
        versions.append((tuple(int(p) for p in m.groups()), name[len(prefix):]))
if versions:
    versions.sort(reverse=True)
    print(versions[0][1])
" 2>/dev/null
}

download_asset() {
  local tool="$1" version="$2" platform="$3" dest="$4"
  local tag="${tool}-v${version}"
  local asset="${tool}-${platform}"
  local url="https://github.com/$REPO/releases/download/${tag}/${asset}"
  curl -fsSL -o "$dest" "$url"
}

# On Apple Silicon the kernel SIGKILLs any binary whose code signature
# doesn't validate. Our release binaries ship with a linker-generated
# adhoc signature, but downloading invalidates it: macOS tags the file
# with com.apple.quarantine / com.apple.provenance xattrs, which breaks
# the seal. The symptom is a bare "Killed: 9" on first run (and a failed
# `skill install`). Strip the quarantine flag and re-sign adhoc so the
# signature covers the file as it sits on disk. No-op on Linux.
resign_macos_binary() {
  local dest="$1"
  [[ "$PLATFORM" == macos-* ]] || return 0
  command -v codesign &>/dev/null || return 0
  xattr -d com.apple.quarantine "$dest" 2>/dev/null || true
  codesign --force --sign - "$dest" 2>/dev/null || true
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
    resign_macos_binary "$INSTALL_DIR/$tool"
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
