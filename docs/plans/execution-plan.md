# Execution Plan

This file is the active phase execution plan for the current product cycle.
Archived completed cycle: `docs/plans/archive/execution-plan-2026-q1.md`.

## Program Rules

- Runtime product scope is single-agent.
- Multi-agent is development-time only via `.codex/`.
- Runtime config source of truth is `~/.meow-soma/config.toml`.
- Every phase must leave the repository in releasable condition.
- Every phase requires explicit acceptance checks before moving forward.

## Cycle Summary

- Status: Active (started 2026-03-15)
- Roadmap reference: `docs/plans/roadmap.md`
- Backlog reference: `docs/plans/backlog.md`

## Phase Overview

| Phase | Goal | Status | Owner | Target |
|---|---|---|---|---|
| 1 | Built-in slash command surface expansion | Done | cli/tui maintainer | current cycle |
| 2 | Instruction memory system (project/user/local) | Planned | runtime/config maintainer | current cycle |
| 3 | Permission modes and rule engine hardening | Planned | policy/runtime maintainer | current cycle |
| 4 | CLI session lifecycle parity (resume/fork/headless) | Planned | cli/runtime maintainer | current cycle |

## Current Cycle Phase Plan

### Phase 1 - Built-in Slash Command Surface Expansion

Status:
- Done

Objective:
- Improve command discoverability and in-session control for TUI users.

Scope:
- Add/normalize high-value built-ins and aliases.
- Improve `/` discovery and help output consistency.
- Keep command behavior consistent in one-screen TUI flow.

Out of scope:
- Plugin marketplace command packs.
- New external integrations.

Implementation tasks:
1. Define canonical slash command registry and aliases.
2. Implement missing built-ins selected for the current cycle.
3. Add tests for command parsing/dispatch and unknown-command handling.
4. Update docs for command usage.

Deliverables:
- Updated slash command registry in runtime/TUI command handling.
- Command tests and user docs.

Definition of done:
- Slash command discovery is complete and consistent.
- Built-ins targeted for this phase are available and test-covered.

Completion notes:
- Implemented canonical slash-command registry and alias resolution in TUI command dispatch.
- Added `/status` command, unknown-command suggestions, and shared registry usage in command palette.
- Added/updated unit tests for aliases, suggestions, and slash-prefixed palette filtering.

Verification commands:
- `cargo fmt --check`
- `cargo check`
- `cargo test`

Known risks:
- Command-name conflicts with existing behavior; mitigate with alias map and regression tests.

### Phase 2 - Instruction Memory System

Status:
- Planned

Objective:
- Provide stable instruction hierarchy for better response consistency across sessions.

Scope:
- Implement instruction loading from project, local, and user scopes.
- Implement deterministic precedence and merged-effective view.
- Provide `/init` and `/memory` flows needed for setup and inspection.

Out of scope:
- Automatic memory learning/writeback engine.
- Remote/shared instruction sync.

Implementation tasks:
1. Define instruction file locations and precedence.
2. Implement merge and effective-context injection into prompt pipeline.
3. Add user commands for initialization and inspection.
4. Add tests for precedence and reload behavior.

Deliverables:
- Instruction loader/merger integrated into runtime prompt path.
- Commands and docs for memory setup/inspection.

Definition of done:
- Effective instruction set is deterministic and visible.
- Prompt behavior reflects instruction updates without restarting sessions.

Verification commands:
- `cargo fmt --check`
- `cargo check`
- `cargo test`

Known risks:
- Unexpected instruction precedence; mitigate with explicit order and fixture tests.

### Phase 3 - Permission Modes and Rule Engine Hardening

Status:
- Planned

Objective:
- Ensure safe, predictable tool execution policy for day-to-day usage.

Scope:
- Add robust allow/ask/deny mode handling.
- Add deterministic rule matching for tool names/specifiers.
- Keep inline approval UX compatible with current TUI flow.

Out of scope:
- Full sandbox domain/path control expansion.
- Enterprise policy channels.

Implementation tasks:
1. Finalize permission mode model and rule evaluation order.
2. Implement policy matcher updates and conflict resolution semantics.
3. Add integration tests for risky tool scenarios.
4. Update policy docs and examples.

Deliverables:
- Updated policy/rule engine behavior.
- Coverage for auth/approval/denial paths.

Definition of done:
- Risky actions always follow deterministic policy outcomes.
- Rule behavior is documented and test-verified.

Verification commands:
- `cargo fmt --check`
- `cargo check`
- `cargo test`

Known risks:
- Regressions in existing tool execution; mitigate with targeted policy regression suites.

### Phase 4 - CLI Session Lifecycle Parity

Status:
- Planned

Objective:
- Close lifecycle gaps for long-running and automation-friendly usage.

Scope:
- Improve continue/resume by id or named session.
- Add safe fork/session-branch behavior.
- Strengthen headless/print workflows for scripted runs.

Out of scope:
- Cloud remote-control handoff.
- Scheduled task engine.

Implementation tasks:
1. Define lifecycle command semantics and UX messages.
2. Implement resume/fork flow in session/runtime layer.
3. Expand headless output controls and limits.
4. Add tests for resume/fork/import-export compatibility.

Deliverables:
- Updated CLI session command behavior.
- Session lifecycle and headless-mode tests.

Definition of done:
- Users can reliably continue, resume, and fork sessions.
- Headless workflows are deterministic and script-friendly.

Verification commands:
- `cargo fmt --check`
- `cargo check`
- `cargo test`

Known risks:
- Session state drift during resume/fork; mitigate with migration checks and replay tests.

## Future Phase Template

Use this template for future phases after the current cycle.

### Phase `<number>` - `<title>`

Status:
- Planned / In progress / Done

Objective:
- `<what this phase must achieve>`

Scope:
- `<in-scope item 1>`
- `<in-scope item 2>`

Out of scope:
- `<explicit non-goal 1>`
- `<explicit non-goal 2>`

Implementation tasks:
1. `<task>`
2. `<task>`
3. `<task>`

Deliverables:
- `<file/module/output>`
- `<tests/docs/update>`

Definition of done:
- `<acceptance criterion 1>`
- `<acceptance criterion 2>`

Verification commands:
- `cargo fmt --check`
- `cargo check`
- `<project-specific test command>`

Known risks:
- `<risk + mitigation>`

## Handoff Protocol for New Contributors

When assigning work to another engineer or agent:
1. Reference exact phase and task number from this file.
2. Provide impacted files and expected deliverables.
3. Provide acceptance criteria and verification commands.
4. Require a short decision log for deviations.

## Tracking Template

Use this per task in PR description:

- Phase:
- Task:
- Scope:
- Files changed:
- Acceptance criteria:
- Validation commands:
- Known risks:
- Follow-up tasks:
