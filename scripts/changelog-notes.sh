#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

RAW_VERSION="${1:-}"
if [[ -z "$RAW_VERSION" ]]; then
  echo "error: provide version (e.g. 1.2.3 or v1.2.3)" >&2
  exit 1
fi

VERSION="${RAW_VERSION#v}"
CHANGELOG_FILE="${2:-CHANGELOG.md}"

if [[ ! -f "$CHANGELOG_FILE" ]]; then
  echo "error: changelog file not found: ${CHANGELOG_FILE}" >&2
  exit 1
fi

awk -v version="$VERSION" '
  BEGIN {
    in_section = 0
    found = 0
  }
  $0 ~ "^## \\[" version "\\]" {
    in_section = 1
    found = 1
    next
  }
  in_section && $0 ~ "^## \\[" {
    exit
  }
  in_section {
    print
  }
  END {
    if (!found) {
      print "error: CHANGELOG.md must contain a section for " version > "/dev/stderr"
      exit 1
    }
  }
' "$CHANGELOG_FILE" | sed '1{/^[[:space:]]*$/d;}'
