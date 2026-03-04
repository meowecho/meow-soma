#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

RAW_VERSION="${VERSION:-${1:-${GITHUB_REF_NAME:-}}}"
if [[ -z "$RAW_VERSION" ]]; then
  echo "error: provide VERSION env or first arg (e.g. v1.2.3)"
  exit 1
fi

if [[ ! "$RAW_VERSION" =~ ^v?[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "error: version must look like v1.2.3 or 1.2.3"
  exit 1
fi

VERSION="${RAW_VERSION#v}"
TAG="v${VERSION}"

if ! grep -q "## \[${VERSION}\]" CHANGELOG.md; then
  echo "error: CHANGELOG.md must contain a section for ${VERSION}"
  exit 1
fi

cargo build --release --locked

OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"
DIST_DIR="$ROOT_DIR/dist"
PACKAGE_NAME="meow-${TAG}-${OS}-${ARCH}"

mkdir -p "$DIST_DIR"
cp "$ROOT_DIR/target/release/meow" "$DIST_DIR/meow"
tar -C "$DIST_DIR" -czf "$DIST_DIR/${PACKAGE_NAME}.tar.gz" meow
rm -f "$DIST_DIR/meow"

if command -v shasum >/dev/null 2>&1; then
  shasum -a 256 "$DIST_DIR/${PACKAGE_NAME}.tar.gz" > "$DIST_DIR/${PACKAGE_NAME}.tar.gz.sha256"
else
  sha256sum "$DIST_DIR/${PACKAGE_NAME}.tar.gz" > "$DIST_DIR/${PACKAGE_NAME}.tar.gz.sha256"
fi

if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
  echo "artifact=$DIST_DIR/${PACKAGE_NAME}.tar.gz" >> "$GITHUB_OUTPUT"
  echo "checksum=$DIST_DIR/${PACKAGE_NAME}.tar.gz.sha256" >> "$GITHUB_OUTPUT"
fi

echo "[release-ci] built: $DIST_DIR/${PACKAGE_NAME}.tar.gz"
echo "[release-ci] checksum: $DIST_DIR/${PACKAGE_NAME}.tar.gz.sha256"
