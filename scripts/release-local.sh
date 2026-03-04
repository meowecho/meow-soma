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

RAW_OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
case "$RAW_OS" in
  linux*) OS="linux" ;;
  darwin*) OS="darwin" ;;
  msys*|mingw*|cygwin*|windows_nt*) OS="windows" ;;
  *) OS="$RAW_OS" ;;
esac

RAW_ARCH="$(uname -m | tr '[:upper:]' '[:lower:]')"
case "$RAW_ARCH" in
  x86_64|amd64) ARCH="x86_64" ;;
  arm64|aarch64) ARCH="arm64" ;;
  *) ARCH="$RAW_ARCH" ;;
esac

DIST_DIR="$ROOT_DIR/dist"
PACKAGE_NAME="meow-${TAG}-${OS}-${ARCH}"
BIN_NAME="meow"
if [[ "$OS" == "windows" ]]; then
  BIN_NAME="meow.exe"
fi

mkdir -p "$DIST_DIR"
cp "$ROOT_DIR/target/release/${BIN_NAME}" "$DIST_DIR/${BIN_NAME}"
tar -C "$DIST_DIR" -czf "$DIST_DIR/${PACKAGE_NAME}.tar.gz" "${BIN_NAME}"
rm -f "$DIST_DIR/${BIN_NAME}"

if command -v shasum >/dev/null 2>&1; then
  shasum -a 256 "$DIST_DIR/${PACKAGE_NAME}.tar.gz" > "$DIST_DIR/${PACKAGE_NAME}.tar.gz.sha256"
elif command -v sha256sum >/dev/null 2>&1; then
  sha256sum "$DIST_DIR/${PACKAGE_NAME}.tar.gz" > "$DIST_DIR/${PACKAGE_NAME}.tar.gz.sha256"
else
  openssl dgst -sha256 "$DIST_DIR/${PACKAGE_NAME}.tar.gz" | sed 's/^.*= //' > "$DIST_DIR/${PACKAGE_NAME}.tar.gz.sha256"
fi

echo "[release-local] built: $DIST_DIR/${PACKAGE_NAME}.tar.gz"
echo "[release-local] checksum: $DIST_DIR/${PACKAGE_NAME}.tar.gz.sha256"
echo "[release-local] next: git tag ${TAG} && git push origin ${TAG}"
