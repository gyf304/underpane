#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

if [ $# -eq 0 ]; then
  sed -nE 's/^version = "([^"]+)"/\1/p' "$ROOT/src-tauri/Cargo.toml" | head -1
  exit 0
fi

NEW="$1"

sed -i '' -E "s/^version = \"[^\"]+\"/version = \"$NEW\"/" "$ROOT/src-tauri/Cargo.toml"
sed -i '' -E "s/\"version\": \"[^\"]+\"/\"version\": \"$NEW\"/" "$ROOT/src-tauri/tauri.conf.json"
sed -i '' -E "s/\"version\": \"[^\"]+\"/\"version\": \"$NEW\"/" "$ROOT/src-ui/package.json"

(cd "$ROOT/src-tauri" && cargo update -p underpane --offline >/dev/null 2>&1 || true)

echo "bumped to $NEW"
