# Changelog

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog and this project follows Semantic Versioning.

## [Unreleased]

### Added
- Release scripts for local and CI packaging (`scripts/release-local.sh`, `scripts/release-ci.sh`)
- Release workflow for version tags on GitHub
- First-run config setup helper command: `meow config setup`
- Testing and release process documentation

## [0.1.0] - 2026-03-04

### Added
- Initial `meow` CLI scaffold with TUI, provider/runtime abstractions, tool policy, and MCP interop
- Session persistence, migration, and backup/restore flows
- Phase 6 coverage improvements with smoke tests, fixtures, and CI gates
