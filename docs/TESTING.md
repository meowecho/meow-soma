# Testing Guide

Phase 6 establishes a single local/CI test contract for `meow-soma`.

## Required Checks

Run these commands from the repository root:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo check
cargo test
```

## Test Coverage Areas

- Command-level surface tests: `tests/command_surface.rs`
- CLI parser command tests: `src/cli.rs`
- Fixture-based config validation tests: `tests/fixtures/config/` via `src/config.rs`
- Fixture-based policy behavior tests: `tests/fixtures/policy/` via `src/policy.rs`
- Existing module coverage across runtime, tools, state, providers, MCP, and TUI modules

## Running Focused Suites

```bash
cargo test command_surface
cargo test cli::tests
cargo test config::tests
cargo test policy::tests
```

## Fixture Update Rules

- Keep fixtures deterministic and self-contained.
- Add new fixture files under `tests/fixtures/<area>/`.
- Prefer adding fixture cases over duplicating assertion-heavy test code.
- When a fixture changes expected behavior, update both fixture data and the matching test assertion.

## Failure Triage Template

Use this when a test fails locally or in CI:

```text
Failure: <test name>
Category: parser | config | policy | runtime | tools | state | provider | mcp | tui
Command: <exact cargo command>
Observed: <actual output or panic message>
Expected: <expected behavior>
First bad change: <commit/PR if known>
Follow-up: <fix now | open issue | needs investigation>
```

GitHub issue template: `.github/ISSUE_TEMPLATE/test-failure-triage.yml`

## Retry Strategy For Flaky Tests

1. Re-run the failing test only: `cargo test <name> -- --exact --nocapture`
2. Re-run the suite to confirm reproducibility: `cargo test <suite>`
3. If still flaky, capture logs and open an issue before merging.
4. Keep CI strict; do not mute failures with `#[ignore]` unless a follow-up issue is linked.
