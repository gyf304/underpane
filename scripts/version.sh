#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
pushd "$ROOT" >/dev/null

if [ $# -eq 0 ]; then
  sed -nE 's/^version = "([^"]+)"/\1/p' src-tauri/Cargo.toml | head -1
  exit 0
fi

NEW="$1"
TAG="v$NEW"

SEMVER_RE='^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$'
if ! [[ "$NEW" =~ $SEMVER_RE ]]; then
  echo "error: '$NEW' is not a valid semver" >&2
  exit 1
fi

if git rev-parse -q --verify "refs/tags/$TAG" >/dev/null; then
  echo "error: tag $TAG already exists" >&2
  exit 1
fi

if ! git diff-index --quiet HEAD --; then
  echo "error: working tree is not clean" >&2
  exit 1
fi

sed -i '' -E "s/^version = \"[^\"]+\"/version = \"$NEW\"/" src-tauri/Cargo.toml
sed -i '' -E "s/\"version\": \"[^\"]+\"/\"version\": \"$NEW\"/" src-tauri/tauri.conf.json
sed -i '' -E "s/\"version\": \"[^\"]+\"/\"version\": \"$NEW\"/" src-ui/package.json

pushd src-tauri >/dev/null
cargo update -p underpane --offline >/dev/null 2>&1 || true
popd >/dev/null

git add -A
git commit -m "bump version to $NEW"
git tag -a "$TAG" -m "$TAG"
git push
git push origin "$TAG"

echo "bumped to $NEW, tagged $TAG, pushed"
