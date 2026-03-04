# Release Process

This document defines how `meow` releases are versioned, validated, and published.

## Versioning Policy

`meow-soma` follows Semantic Versioning:

- `MAJOR`: breaking CLI/runtime/config behavior
- `MINOR`: backward-compatible features
- `PATCH`: backward-compatible bug fixes

Release tags must use the `vMAJOR.MINOR.PATCH` format (example: `v0.2.0`).

## Changelog Policy

Before tagging a release:

1. Move relevant entries from `## [Unreleased]` in `CHANGELOG.md` into a new version section.
2. Create a heading exactly matching the version number without `v` prefix.
   - Example: `## [0.2.0] - 2026-03-04`
3. Keep entries grouped by category (`Added`, `Changed`, `Fixed`, `Security`).

Both release scripts enforce that `CHANGELOG.md` contains a section for the target version.

## Local Release Build

Use this for local reproducible packaging:

```bash
scripts/release-local.sh v0.2.0
```

What it does:

1. Runs `fmt`, `clippy`, `check`, and `test`
2. Builds `cargo build --release --locked`
3. Creates tarball artifact under `dist/`
4. Writes SHA-256 checksum

Output naming:

- `dist/meow-v<version>-<os>-<arch>.tar.gz`
- `dist/meow-v<version>-<os>-<arch>.tar.gz.sha256`

## CI Release Build

GitHub workflow: `.github/workflows/release.yml`

Trigger: push tag matching `v*.*.*`

CI first runs quality gates (`fmt`, `clippy`, `check`, `test`) on Ubuntu, then uses
`scripts/release-ci.sh` to build artifacts on macOS/Linux/Windows and publishes assets to GitHub Release.

## Publish Flow

1. Ensure `main` is green in CI.
2. Update `CHANGELOG.md` for target version.
3. Run local packaging script and verify artifact.
4. Create and push tag:

```bash
git tag v0.2.0
git push origin v0.2.0
```

5. Confirm release workflow succeeded and artifacts are attached.
6. Run post-release smoke checks from `docs/RELEASE_CHECKLIST.md`.
