# AGENTS.md

This file defines how humans and AI agents collaborate in this repository.
It is the source of truth for development workflow, role boundaries, and quality gates.

## Project Context

- Product: `meow-soma`
- Runtime CLI command: `meow`
- Runtime architecture target: single-agent runtime
- Multi-agent usage: development-only via Codex config in `.codex/`
- Runtime config path: `~/.meow-soma/config.toml`

## Collaboration Goals

- Keep implementation aligned with roadmap and phase plan in `docs/plans/execution-plan.md`
- Preserve runtime/dev config separation
- Ship incrementally with explicit acceptance criteria per phase
- Avoid hidden decisions by documenting assumptions in PR descriptions and commit messages

## Team Roles

- Orchestrator
- Planner
- Researcher
- Coder
- Reviewer
- Operator

Role responsibilities are configured in `.codex/config.toml` and `.codex/agents/*.toml`.

## Agent Selection Policy (Default)

Use agents defined in `.codex/` based on task type by default:
- Planning and acceptance criteria -> `planner`
- Repository/docs/API context gathering -> `researcher`
- Code implementation and refactor work -> `coder`
- Risk, regression, and test-gap review -> `reviewer`
- Build/test/release/ops execution -> `operator`
- Cross-task coordination and output merge -> `orchestrator`

Operational rules:
- Start with `orchestrator` only when a task needs decomposition or multi-step coordination.
- For direct coding tasks, route to `coder` first and then `reviewer` before merge.
- Use the minimal set of agents needed for a task; avoid unnecessary fan-out.
- If routing is ambiguous, prefer `planner` for scoping, then hand off to the execution role.

## Scope Guardrails

- Do not re-introduce runtime multi-agent cowork in MVP or v1 unless explicitly approved.
- Keep security gate mandatory for risky tool execution.
- Keep MCP support as an interop feature without coupling runtime behavior to Codex internals.
- Preserve backwards-compatible CLI command surface unless a breaking change is explicitly planned.

## Task Lifecycle

1. Intake
- Restate the request and impacted modules.
- Link the task to a phase from `docs/plans/execution-plan.md`.

2. Plan
- Produce a decision-complete implementation plan.
- Define acceptance criteria and tests before coding.

3. Implement
- Keep changes minimal and scoped.
- Prefer clear, composable modules over monolithic edits.

4. Verify
- Run `cargo fmt`, `cargo check`, and relevant tests.
- Capture known constraints if environment blocks verification.

5. Review
- Reviewer reports risks first, then summary.
- Confirm config/docs/tests updated when behavior changes.

6. Merge
- Use Conventional Commits / Commitizen format.
- Keep one logical change per commit when possible.

## Quality Gates

- Required before merge:
- `cargo fmt` passes
- `cargo check` passes
- Tests for changed behavior are added or updated
- Docs are updated when public behavior/config changes

- Required for release:
- CI green
- Release notes prepared
- Install and quickstart instructions validated

## Changelog Policy

- `CHANGELOG.md` must describe only software changes in `meow-soma` itself.
- Include user-facing/runtime behavior changes such as features, fixes, commands, provider/runtime behavior, TUI behavior, and tool/policy behavior.
- Exclude repository-maintenance-only changes from `CHANGELOG.md`:
  - docs-only edits
  - roadmap/phase/process updates
  - CI/workflow/label/template housekeeping
  - contributor guidance updates

## Code Ownership Map

- `src/cli.rs`, `src/app.rs`
- Owner focus: command surface and execution flow

- `src/config.rs`, `config/meow.example.toml`, `docs/CONFIG.md`
- Owner focus: runtime config schema and migration compatibility

- `src/providers.rs`, `src/runtime.rs`
- Owner focus: provider integrations and runtime behavior

- `src/tools.rs`, `src/policy.rs`
- Owner focus: tool safety and approval model

- `src/state.rs`
- Owner focus: persistence schema and query behavior

- `.codex/`
- Owner focus: development-time agent orchestration only

## Branch and Commit Conventions

- Branch naming:
- `feat/<topic>`
- `fix/<topic>`
- `chore/<topic>`

- Commit format:
- `feat(scope): ...`
- `fix(scope): ...`
- `docs(scope): ...`
- `chore(scope): ...`

## PR Checklist

- Problem statement and scope are explicit.
- Linked phase and milestone are identified.
- Security impact is called out.
- Config impact is called out.
- Backward compatibility impact is called out.
- Tests and manual validation steps are listed.

## Decision Log Requirement

For non-trivial decisions, add a short note in PR description or issue:
- Decision
- Alternatives considered
- Tradeoff accepted
- Follow-up task if deferred

## Handoff Requirement

Before handing off to another engineer or agent:
- Provide current state summary
- List incomplete tasks with file paths
- List blockers and assumptions
- Provide exact verification commands
