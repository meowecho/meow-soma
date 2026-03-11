# v0.2 Prioritized Backlog

This backlog is prioritized for the next minor cycle after `v0.1.x`.

## P0

1. Provider resilience parity across modes
- Scope: unify retry and timeout behavior between one-shot and streaming flows.
- Acceptance: consistent retryability and error mapping for hosted/local providers.
- Owner: runtime/provider maintainer.
- Status: Done (2026-03-10)

2. Launch telemetry implementation
- Scope: add structured runtime counters for startup latency, response latency, and error categories.
- Acceptance: metrics exportable for weekly reporting without manual log scraping.
- Owner: runtime/state maintainer.
- Status: Done (2026-03-10)

## P1

1. MCP client compatibility expansion
- Scope: broaden test matrix for malformed payload and recovery behavior.
- Acceptance: stable protocol errors and continued server responsiveness across cases.
- Owner: mcp maintainer.
- Status: Done (2026-03-11)

2. TUI transcript performance for long sessions
- Scope: optimize rendering and scroll behavior under large transcript volumes.
- Acceptance: no missing latest message, no overflow panics, stable scroll behavior.
- Owner: tui/runtime maintainer.
- Status: Done (2026-03-11)

## P2

1. Release docs automation polish
- Scope: reduce manual steps in release notes and post-release report generation.
- Acceptance: repeatable runbook with minimal manual edits.
- Owner: operator.

2. Additional provider troubleshooting guide
- Scope: add targeted diagnostics for auth/rate-limit/timeout failures.
- Acceptance: actionable steps documented for top failure classes.
- Owner: docs/provider maintainer.
- Status: Done (2026-03-11)

## Intake Rule

New work should be appended with:

- Priority (`P0`, `P1`, `P2`)
- Problem statement
- Acceptance criteria
- Owner
- Target release (`0.2.0` or `0.2.x`)
