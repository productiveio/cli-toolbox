#!/usr/bin/env bash
set -euo pipefail

VALID_TOOLS="tb-prod tb-sem tb-bug tb-lf devctl"

usage() {
  echo "Usage: $0 <tool> <version>"
  echo ""
  echo "Bump a tool's version, commit, and create a tag."
  echo ""
  echo "Tools: $VALID_TOOLS"
  echo "Example: $0 tb-prod 0.2.0"
  exit 1
}

if [[ $# -ne 2 ]]; then
  usage
fi

tool="$1"
version="$2"

# Validate tool name
if ! echo "$VALID_TOOLS" | grep -qw "$tool"; then
  echo "Error: unknown tool '$tool'"
  echo "Valid tools: $VALID_TOOLS"
  exit 1
fi

# Validate version format (semver-ish)
if ! [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Error: version must be in X.Y.Z format, got '$version'"
  exit 1
fi

cargo_toml="crates/$tool/Cargo.toml"

if [[ ! -f "$cargo_toml" ]]; then
  echo "Error: $cargo_toml not found"
  exit 1
fi

current=$(grep '^version = ' "$cargo_toml" | head -1 | sed 's/version = "\(.*\)"/\1/')
echo "Bumping $tool: $current → $version"

# Update version in Cargo.toml (portable across macOS and Linux)
sed -i.bak "s/^version = \"$current\"/version = \"$version\"/" "$cargo_toml"
rm -f "$cargo_toml.bak"

# Verify it compiles and update Cargo.lock
echo "Running cargo check..."
cargo check -p "$tool"

# Commit and tag
git add "$cargo_toml" Cargo.lock
git commit -m "$tool: bump version to $version"
git tag "$tool-v$version"

echo ""
echo "Done! Created commit and tag: $tool-v$version"
echo ""
echo "To release, run:"
echo "  git push && git push --tags"
