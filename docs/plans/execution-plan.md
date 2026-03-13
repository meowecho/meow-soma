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

- Status: Active (started 2026-03-12)
- Roadmap reference: `docs/plans/roadmap.md`
- Backlog reference: `docs/plans/backlog.md`

## Phase Overview

| Phase | Goal | Status | Owner | Target |
|---|---|---|---|---|
| 1 | `<goal>` | Planned | `<owner>` | `<target date/release>` |
| 2 | `<goal>` | Planned | `<owner>` | `<target date/release>` |
| 3 | `<goal>` | Planned | `<owner>` | `<target date/release>` |

## Phase Template

Use this template for each phase in this file.

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
