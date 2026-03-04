#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "Usage: scripts/release-local.sh <vMAJOR.MINOR.PATCH>"
  exit 1
}

if [[ $# -ne 1 ]]; then
  usage
fi

RAW_VERSION="$1"
if [[ ! "$RAW_VERSION" =~ ^v?[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "error: version must look like v1.2.3 or 1.2.3"
  exit 1
fi

VERSION="${RAW_VERSION#v}"
TAG="v${VERSION}"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if ! grep -q "## \[${VERSION}\]" CHANGELOG.md; then
  echo "error: CHANGELOG.md must contain a section for ${VERSION}"
  exit 1
fi

echo "[release-local] running quality gates"
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo check
cargo test

echo "[release-local] building release binary"
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

echo "[release-local] built: $DIST_DIR/${PACKAGE_NAME}.tar.gz"
echo "[release-local] checksum: $DIST_DIR/${PACKAGE_NAME}.tar.gz.sha256"
echo "[release-local] next: git tag ${TAG} && git push origin ${TAG}"
