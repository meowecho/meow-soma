# Changelog

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog and this project follows Semantic Versioning.

## [Unreleased]

## [0.1.0] - 2026-03-08

### Added
- Initial `meow` CLI scaffold with TUI, provider/runtime abstractions, tool policy, and MCP interop
- Session persistence, migration, and backup/restore flows
- Phase 6 coverage improvements with smoke tests, fixtures, and CI gates
- Release scripts for local and CI packaging (`scripts/release-local.sh`, `scripts/release-ci.sh`)
- Release workflow for version tags on GitHub
- Windows release artifacts (`windows-x86_64`) in release workflow
- First-run config setup helper command: `meow config setup`
- Testing and release process documentation
- Phase 8 launch and operations artifacts:
  - `docs/LAUNCH_CHECKLIST.md`
  - `docs/TRIAGE_SLA.md`
  - `docs/METRICS_BASELINE.md`
  - `docs/PATCH_RELEASE_WORKFLOW.md`
  - `docs/BACKLOG_V0_2.md`
  - `docs/reports/v0.1.0-metrics-baseline.md`
- New issue templates for incident and bug triage:
  - `.github/ISSUE_TEMPLATE/incident.yml`
  - `.github/ISSUE_TEMPLATE/bug-report.yml`
- Managed launch triage label baseline and sync workflow:
  - `.github/labels.json`
  - `.github/workflows/labels-sync.yml`
  - `.github/workflows/triage-guard.yml`

### Fixed
- Release publish step now checks out repository context and uses explicit `GH_REPO` for `gh` commands
