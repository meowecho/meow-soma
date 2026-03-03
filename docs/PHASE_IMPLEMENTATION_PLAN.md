# Meow-Soma Phase Implementation Plan

This document is execution-focused and handoff-ready.
It defines what must be built, how to verify completion, and what output is required per phase.

## Program Rules

- Runtime product scope is single-agent.
- Multi-agent is development-time only via `.codex/`.
- Runtime config source of truth is `~/.meow-soma/config.toml`.
- Every phase must leave the repository in releasable condition.
- Every phase requires explicit acceptance checks before moving forward.

## Phase 0 - Baseline Scaffold (Completed)

Status:
- Done

Objective:
- Establish runnable CLI scaffold and repository structure.

Completed outputs:
- Rust package with `meow` binary.
- Core command surface scaffold (default `meow` TUI, `ask`, `run`, `tool`, `mcp`, `session`, `config`).
- Runtime config schema and template.
- State, tool, policy, and runtime module boundaries.
- Development multi-agent config under `.codex/`.

Acceptance evidence:
- `cargo check` passes.
- Initial docs and config files committed.

## Phase 1 - Real Provider Integrations

Objective:
- Replace provider stubs with production integrations for OpenAI, Anthropic, and Ollama.

Current status:
- Baseline implemented:
  - Live HTTP adapters for OpenAI/Anthropic/Ollama
  - API key lookup from env for OpenAI/Anthropic
  - Timeout + retry behavior from config
  - Error normalization (`auth`, `rate_limit`, `timeout`, `invalid_request`, `server`, `transport`, `parse`)
- Remaining hardening:
  - Add provider-specific integration tests for OpenAI/Anthropic with mocked authenticated responses

Scope:
- Implement HTTP clients for each provider.
- Add API key resolution from environment variables.
- Support timeout and retry policy from config.
- Normalize provider responses into one internal output contract.

Implementation tasks:
1. Add provider-specific request/response types.
2. Add shared transport helper and retry wrapper.
3. Add structured error mapping (`auth`, `rate_limit`, `timeout`, `server`, `invalid_request`).
4. Update runtime path to use live provider adapters.
5. Add provider selection fallback behavior.

Deliverables:
- Updated `src/providers.rs` and any new provider modules.
- Unit tests for request building and error mapping.
- Integration tests with mocked HTTP responses.
- Updated docs with auth setup instructions.

Definition of done:
- `meow ask` works with all three providers in configured modes.
- Invalid credentials produce actionable errors.
- Timeouts and retries behave as configured.

Verification commands:
- `cargo fmt --check`
- `cargo check`
- `cargo test providers`

## Phase 2 - Runtime Loop and UX Reliability

Objective:
- Make single-agent behavior consistent and predictable for real workflows.

Current status:
- Baseline implemented:
  - Runtime execution context model with profile + operation + bounded context messages
  - Bounded session-context loading for chat interactions
  - Profile-based prompt templates (`default`, `coding`, `research` behavior modes)
  - Interrupt handling for long requests (Ctrl+C cancellation path)
  - Improved `run` and `session` output formatting
- Remaining hardening:
  - Add richer profile selection UX (CLI-level profile override) if needed

Scope:
- Improve prompt pipeline and state handling for TUI chat interactions, `ask`, and `run`.
- Add deterministic run metadata (run ids, timestamps, status).
- Add concise user-facing errors and troubleshooting hints.

Implementation tasks:
1. Add structured runtime execution context object.
2. Add bounded context loading from prior session messages.
3. Add prompt templates by profile (`default`, `coding`, `research`).
4. Add cancellation and interruption handling for long-running requests.
5. Improve command output formatting for session and run inspection.

Deliverables:
- Runtime loop updates in `src/runtime.rs` and `src/app.rs`.
- Session/run formatting updates.
- Tests for context window behavior and cancellation paths.

Definition of done:
- Repeated TUI chat interactions preserve context reliably.
- `run` outputs consistent status and summary fields.
- Runtime errors are mapped to clear user messages.

Verification commands:
- `cargo fmt --check`
- `cargo check`
- `cargo test runtime`

## Phase 2.5 - Claude Code/Codex-Style Terminal UI (TUI)

Objective:
- Deliver an agentic terminal interface that feels like Claude Code/Codex workflows while keeping single-agent runtime architecture.

Current status:
- Done:
  - `meow` full-screen terminal mode scaffold
  - Transcript/input/status panes
  - Chat submission flow wired to runtime with session persistence
  - Exit controls (`Esc`, `Ctrl+C`, `/quit`)
  - Streaming token rendering in TUI chat feed
  - Inline approval prompts for risky `/tool` actions
  - Rich keybindings (`Up/Down`, `Ctrl+R` search, `Ctrl+P` palette)
  - Updated TUI usage docs in `README.md`

Scope:
- Add full-screen TUI mode for interactive usage.
- Add streaming assistant output in terminal.
- Add inline approval prompts for risky tool execution.
- Add session/status panels that keep users inside one continuous workflow.

Implementation tasks:
1. Add default `meow` TUI entrypoint and event loop using terminal UI primitives.
2. Implement panes: transcript, input, status, and tool timeline.
3. Implement token streaming rendering path in TUI/run flows.
4. Add inline approval interactions tied to permission gate decisions.
5. Add slash commands in TUI (`/help`, `/provider`, `/model`, `/session`, `/clear`).
6. Add keyboard controls for history, scrolling, and command palette behavior.
7. Add graceful resize/repaint handling and crash-safe terminal restore.

Deliverables:
- TUI runtime module(s) and command wiring in CLI/app layers.
- Streaming output path for TUI mode and non-TUI request flows.
- Inline approval UX connected to policy engine.
- User docs for TUI controls and troubleshooting.

Definition of done:
- `meow` runs as a stable full-screen interface.
- User can chat, run tasks, approve/reject risky actions, and inspect timeline in one screen.
- Streaming output is visible token-by-token without waiting for full completion.
- Terminal state is restored correctly after exit or panic.

Verification commands:
- `cargo fmt --check`
- `cargo check`
- `cargo test tui`
- Manual smoke: `meow` + provider call + one approval flow

## Phase 3 - Tool Safety and Execution Hardening

Objective:
- Upgrade permission gate and tool sandboxing for safe real usage.

Current status:
- Done:
  - Policy severity model (`allow`, `approve_required`, `deny`)
  - Shell command normalization before policy checks
  - `fs.write` workspace/approved-root boundary enforcement
  - Approval reason-code persistence in audit records
  - Policy/tool/state-focused test coverage

Scope:
- Improve allowlist/denylist semantics.
- Add command classifier for high-risk shell patterns.
- Constrain file tools to workspace or approved paths.
- Expand audit logging for tool calls and approvals.

Implementation tasks:
1. Add policy severity model (`allow`, `approve_required`, `deny`).
2. Add workspace boundary enforcement for `fs.write`.
3. Add shell command normalization before policy checks.
4. Add approval reason codes for analytics and debugging.
5. Add policy-focused test matrix.

Deliverables:
- Policy engine updates in `src/policy.rs`.
- Tool execution guard updates in `src/tools.rs` and `src/app.rs`.
- Extended audit schema in `src/state.rs`.
- Updated runtime config docs for security settings.

Definition of done:
- Risky commands never run without explicit approval.
- Denied commands produce clear rationale.
- Audit records capture action, decision, reason, and timestamp.

Verification commands:
- `cargo fmt --check`
- `cargo check`
- `cargo test policy`
- `cargo test tools`

## Phase 4 - MCP Interop Compliance

Objective:
- Evolve MCP server mode from scaffold to stable client-facing capability.

Current status:
- Done:
  - MCP request/response schema with protocol version (`meow.mcp.v1`)
  - Structured protocol error codes and mapping for tool/policy failures
  - Request-id correlation and structured MCP server logs
  - Tool discovery via `tools/list` method
  - Compatibility tests for normal and malformed request flows

Scope:
- Formalize request/response schema and lifecycle behavior.
- Improve stdio server reliability and error handling.
- Expose tool metadata and supported operations in a discoverable format.

Implementation tasks:
1. Define internal MCP DTOs and versioned schema mapping.
2. Add explicit protocol error codes.
3. Add request id correlation and structured logs.
4. Add discovery endpoint behavior for available tools.
5. Add compatibility tests against expected MCP client interactions.

Deliverables:
- Hardened MCP handling in `src/app.rs` and/or dedicated modules.
- MCP protocol docs and examples.
- Integration tests for normal and failure flows.

Definition of done:
- External MCP client can discover tools and invoke at least one successful call.
- Invalid payloads return stable structured errors.
- Server remains responsive after malformed requests.

Verification commands:
- `cargo fmt --check`
- `cargo check`
- `cargo test mcp`

## Phase 5 - Persistence, Migration, and Data Integrity

Objective:
- Make local state durable, queryable, and migration-safe.

Scope:
- Add schema versioning and migration framework.
- Improve indexing and query performance for sessions/tool history.
- Add export/import utilities for migration and backup.

Implementation tasks:
1. Add schema version table and migration runner.
2. Add indexes for frequently queried columns.
3. Add JSON and Markdown export validation tests.
4. Add corruption handling and recovery messaging.
5. Document backup and restore workflow.

Deliverables:
- Migration-aware `src/state.rs`.
- Data management command enhancements in `src/app.rs`.
- Data lifecycle docs.

Definition of done:
- Upgrades run migrations automatically and safely.
- Existing sessions remain readable after migration.
- Export/import behavior is validated in tests.

Verification commands:
- `cargo fmt --check`
- `cargo check`
- `cargo test state`

## Phase 6 - Test Coverage and CI Gates

Objective:
- Make regressions hard to introduce and easy to detect.

Scope:
- Expand unit/integration coverage across all critical modules.
- Add CI workflow with strict checks.
- Add smoke tests for CLI command paths.

Implementation tasks:
1. Add command-level tests for default `meow` TUI, `ask`, `run`, `tool`, `mcp`, `session`, `config`.
2. Add fixture-based tests for config validation and policy behavior.
3. Add CI workflow with fmt, clippy, check, and test gates.
4. Add failure triage templates and retry strategy for flaky tests.

Deliverables:
- Test suites under `tests/` and module tests in `src/`.
- CI definitions in `.github/workflows/`.
- Testing guide in docs.

Definition of done:
- CI is required and green on main.
- Coverage is sufficient for critical paths.
- A contributor can run tests locally with documented steps.

Verification commands:
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`

## Phase 7 - Packaging and Release Readiness

Objective:
- Make installation, upgrade, and first-run experience stable for external users.

Scope:
- Add release build profiles and packaging artifacts.
- Improve CLI help and quickstart docs.
- Add versioning and changelog workflow.

Implementation tasks:
1. Add release scripts for local and CI release builds.
2. Add semantic versioning and changelog update process.
3. Add installation docs for macOS and Linux.
4. Add first-run setup helper for runtime config initialization.
5. Add release checklist template.

Deliverables:
- Release scripts and docs.
- Updated README with install and quickstart sections.
- Taggable release process.

Definition of done:
- New user can install `meow`, configure one provider, and run `ask` in under 10 minutes.
- Release artifact can be produced repeatably.

Verification commands:
- `cargo build --release`
- `meow config init`
- `meow ask "health check"`

## Phase 8 - v1 Launch and Post-Launch Hardening

Objective:
- Ship v1 and stabilize based on real usage feedback.

Scope:
- Publish first stable release.
- Track critical defects and performance regressions.
- Prioritize patch releases based on severity.

Implementation tasks:
1. Run launch checklist and publish release notes.
2. Add issue triage labels and SLA targets.
3. Track startup time, response latency, and failure rates.
4. Execute patch release process for high-priority defects.

Deliverables:
- v1.0.0 release.
- Post-launch incident and patch workflow.
- Prioritized backlog for v1.1.

Definition of done:
- v1 released with installation and operational docs.
- Critical bugs have response and ownership process.
- Next-cycle roadmap is documented.

Verification commands:
- Release workflow execution logs
- Post-release smoke tests
- Issue triage report after first usage window

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
