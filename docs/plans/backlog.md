# Prioritized Backlog

This file is the active backlog for the current product cycle.
Archived completed cycle: `docs/plans/archive/backlog-2026-q1.md`.

## Current Cycle

- Status: Active (started 2026-03-15)
- Backlog baseline: -
- Goal: collect candidate features first, then pick implementation slices.
- Runtime guardrail: single-agent runtime remains the target architecture; runtime multi-agent cowork stays deferred unless explicitly approved.

## P0

1. CLI Session Lifecycle Parity (interactive + headless + resume)
- Problem statement: the reference implementation supports a broad CLI lifecycle (`interactive`, `print/headless`, `continue/resume/fork`, named sessions) that users expect from an AI coding terminal.
- Scope: close gaps in command/flag parity for core session lifecycle and non-interactive scripting workflows.
- Acceptance criteria: users can start, continue, resume by id/name, fork sessions, and run print mode for automation with predictable behavior.
- Owner: cli/runtime maintainer
- Status: Planned
- Target release or cycle: current cycle

2. Built-in Slash Command Surface Parity
- Problem statement: the reference implementation exposes a large command surface (for config, context, model, tools, diagnostics, sessions, MCP, plugins, usage, git helpers) discoverable from `/`.
- Scope: inventory and implement missing high-value built-ins and aliases in a consistent `/` command UX.
- Acceptance criteria: `/` shows full command list with filtering, and implemented commands behave consistently across sessions.
- Owner: cli/tui maintainer
- Status: Planned
- Target release or cycle: current cycle

3. Instruction Memory System Parity (project instruction-file style)
- Problem statement: the reference implementation combines persistent instruction files and auto memory with clear scope hierarchy and precedence.
- Scope: implement scoped instruction loading, nested discovery, rule files, import syntax, and `/init` + `/memory` workflows.
- Acceptance criteria: effective instruction precedence is visible, project/user/local scopes work, and memory can be audited/edited in-session.
- Owner: runtime/config maintainer
- Status: Planned
- Target release or cycle: current cycle

4. Auto Memory and Learning Parity
- Problem statement: the reference implementation auto-writes machine-local memory and recalls it across sessions; this significantly improves iterative workflows.
- Scope: add configurable auto-memory directory, read/write lifecycle, index file, and memory on/off controls.
- Acceptance criteria: memory is written and recalled across sessions, togglable at runtime, and editable as plain markdown.
- Owner: runtime/state maintainer
- Status: Planned
- Target release or cycle: current cycle

5. Permission Modes and Rule Engine Parity
- Problem statement: the reference implementation has explicit permission modes, allow/ask/deny rules, tool-level specifiers, and precedence behavior.
- Scope: implement robust permission mode switching and rule matching for tool names + specifiers with deterministic first-match semantics.
- Acceptance criteria: rule evaluation order is deterministic, risky actions respect mode/rules, and deny/ask/allow behavior is test-covered.
- Owner: policy/runtime maintainer
- Status: Planned
- Target release or cycle: current cycle

6. Sandboxing and Filesystem/Network Controls
- Problem statement: the reference implementation supports sandbox controls, path allow/deny, domain allowlists, and command exclusions for safer execution.
- Scope: expose sandbox config for filesystem/network boundaries and merge with permission-rule-derived constraints.
- Acceptance criteria: sandbox boundaries are enforced in tool execution and can be verified with policy tests.
- Owner: policy/operator maintainer
- Status: Planned
- Target release or cycle: current cycle

7. Hooks Engine Full Lifecycle Parity
- Problem statement: the reference implementation hooks support lifecycle events (session, prompt, tool, permission, subagent/team, notification, worktree) with command/http/prompt handlers.
- Scope: add configurable hook events, matcher patterns, structured input/output, decision handling, and `/hooks` management UX.
- Acceptance criteria: hooks can allow/block/annotate actions, event payloads are stable, and hook failures are handled by policy.
- Owner: runtime/policy maintainer
- Status: Planned
- Target release or cycle: current cycle

8. MCP Integration Parity (servers, auth, resources, prompts)
- Problem statement: the reference implementation deeply integrates MCP (HTTP/stdio/SSE, OAuth, scope, resources via `@`, prompts via `/mcp__...`).
- Scope: strengthen MCP server management, auth flow, scoped config, resource referencing, and prompt-as-command discovery.
- Acceptance criteria: users can add/manage MCP servers, authenticate when needed, reference MCP resources in prompts, and execute MCP prompts via slash commands.
- Owner: mcp/runtime maintainer
- Status: Planned
- Target release or cycle: current cycle

9. Checkpointing + Rewind Parity
- Problem statement: the reference implementation tracks edit checkpoints and supports rewind/summarize workflows to recover from bad turns.
- Scope: session checkpoint capture, rewind menu/actions, and restore/summarize semantics.
- Acceptance criteria: users can jump to prior checkpoints safely and continue without corrupting session state.
- Owner: state/tui maintainer
- Status: Planned
- Target release or cycle: current cycle

10. Interactive Mode Parity (keyboard, vim, history, tasks)
- Problem statement: the reference implementation interactive mode includes dense keyboard ergonomics (history search, vim mode, side questions, background tasks).
- Scope: keybindings and interaction parity for high-velocity terminal usage.
- Acceptance criteria: power-user paths (`/vim`, command history search, quick command entry, task visibility) are stable and documented.
- Owner: tui maintainer
- Status: Planned
- Target release or cycle: current cycle

11. Programmatic/Headless Agent Loop Parity
- Problem statement: the reference implementation supports scripted usage with structured output, stream JSON, turn/budget limits, and tool approval handling.
- Scope: expand headless mode for CI/automation with deterministic machine-readable output.
- Acceptance criteria: scripts can run end-to-end with JSON/stream outputs, schema validation, max-turn and max-budget controls.
- Owner: runtime/operator maintainer
- Status: Planned
- Target release or cycle: current cycle

12. Voice Chat MVP Foundation (OpenAI STT/TTS + Push-to-talk)
- Problem statement: `meow` currently has no voice input/output path, so hands-free and spoken conversation workflows are not possible.
- Scope: add a baseline voice subsystem (capture/transcribe/synthesize/playback), add `[voice]` runtime config, and enforce no-audio-retention in MVP.
- Acceptance criteria: hold `F6` to record voice input, user speech appears as transcript text, assistant voice playback works, and no raw audio files are retained.
- Owner: runtime/provider maintainer
- Status: Planned
- Target release or cycle: current cycle

## P1

1. Skills System Parity (`SKILL.md`, frontmatter, arguments)
- Problem statement: the reference implementation skills package reusable behavior and slash commands with invocation controls and tool constraints.
- Scope: implement skill discovery scopes, frontmatter fields, argument substitution, and support files.
- Acceptance criteria: users can define and invoke skills, with automatic/manual invocation and scoped behavior controls.
- Owner: runtime/tui maintainer
- Status: Planned
- Target release or cycle: next cycle

2. Plugins Platform Parity
- Problem statement: the reference implementation plugins bundle skills, agents, hooks, MCP/LSP servers, and default settings for reusable extensions.
- Scope: plugin manifest schema, install/enable/disable/update lifecycle, and compatibility checks.
- Acceptance criteria: plugins can be installed and managed with predictable loading and error reporting.
- Owner: plugin/runtime maintainer
- Status: Planned
- Target release or cycle: next cycle

3. Plugin Marketplaces Parity
- Problem statement: the reference implementation supports marketplace-based plugin discovery from GitHub/git/url/npm/path with trust controls.
- Scope: marketplace registration, refresh/update behavior, team-shared marketplaces, and security prompts.
- Acceptance criteria: marketplace add/install/update flows work in CLI/TUI with trust boundary enforcement.
- Owner: plugin/operator maintainer
- Status: Planned
- Target release or cycle: next cycle

4. Scheduled Tasks Parity (`/loop`, cron tools)
- Problem statement: the reference implementation can schedule recurring/one-shot prompts for polling, reminders, and automation inside a session.
- Scope: session-scoped scheduler, interval/cron syntax, jitter/expiry rules, and task management UI.
- Acceptance criteria: scheduled prompts execute reliably, are listable/cancelable, and expire by policy.
- Owner: runtime/state maintainer
- Status: Planned
- Target release or cycle: next cycle

5. Output Styles Parity
- Problem statement: the reference implementation output styles let users adapt behavior/persona while preserving core tooling capabilities.
- Scope: built-in and custom output styles with frontmatter-based configuration.
- Acceptance criteria: users can switch styles at runtime and create custom styles without breaking tool behavior.
- Owner: runtime/config maintainer
- Status: Planned
- Target release or cycle: next cycle

6. VS Code Integration Parity
- Problem statement: the reference implementation extension UX includes panel workflows, inline diffs, checkpoints, context references, and plugin/mcp management.
- Scope: define and implement extension-side integration points for session resume, git workflows, and MCP controls.
- Acceptance criteria: primary VS Code workflows run without requiring terminal-only fallbacks for common tasks.
- Owner: integrations maintainer
- Status: Planned
- Target release or cycle: next cycle

7. JetBrains Integration Parity
- Problem statement: the reference implementation provides JetBrains plugin capabilities (quick launch, diff viewer, selection context, diagnostics sharing).
- Scope: implement JetBrains plugin parity baseline.
- Acceptance criteria: JetBrains users get equivalent launch, context-sharing, and diff/review flows.
- Owner: integrations maintainer
- Status: Planned
- Target release or cycle: next cycle

8. Chrome Browser Integration Parity
- Problem statement: the reference implementation can connect to Chrome for browser debugging/testing/automation from chat.
- Scope: browser extension handshake, `@browser` task routing, and error handling for long sessions.
- Acceptance criteria: users can run browser-assisted debugging/testing flows from meow sessions.
- Owner: integrations/runtime maintainer
- Status: Planned
- Target release or cycle: next cycle

9. Remote Control Parity (local session control from web/mobile)
- Problem statement: the reference implementation supports controlling local sessions from other devices while keeping execution local.
- Scope: remote-control session registration, secure transport, reconnect semantics, and session naming/state UX.
- Acceptance criteria: one local session can be controlled from browser/mobile with stable reconnect behavior and clear security boundaries.
- Owner: runtime/integrations maintainer
- Status: Planned
- Target release or cycle: next cycle

10. Cloud Web Session Handoff Parity (`--remote`/`--teleport`)
- Problem statement: the reference implementation supports kicking off cloud sessions, monitoring remotely, and teleporting back to local terminal.
- Scope: remote execution handoff model, repository/branch checks, and one-way handoff semantics.
- Acceptance criteria: users can start remote tasks, monitor progress, and resume compatible sessions locally.
- Owner: runtime/operator maintainer
- Status: Planned
- Target release or cycle: next cycle

11. Slack Integration Parity
- Problem statement: the reference implementation in Slack routes coding requests into the reference implementation web sessions with repo/auth context.
- Scope: mention-triggered task creation, repo selection, and result callback UX.
- Acceptance criteria: Slack-based coding delegation can start, track, and resolve tasks reliably.
- Owner: integrations maintainer
- Status: Planned
- Target release or cycle: next cycle

12. TUI Voice Interaction Flow
- Problem statement: the voice flow must run inside the existing TUI layout with clear in-feed state visibility.
- Scope: integrate `listening -> transcribing -> thinking -> speaking` into the current chat feed and add `/voice status|on|off|mute|unmute`.
- Acceptance criteria: end-to-end voice chat works in one screen, feed status transitions are clear, and mute/unmute behaves correctly.
- Owner: tui/runtime maintainer
- Status: Planned
- Target release or cycle: current cycle

## P2

1. Agent Teams Parity (Experimental, Deferred by Guardrail)
- Problem statement: the reference implementation supports multi-session agent teams with lead/teammates and shared task orchestration.
- Scope: feature design captured for parity tracking only; runtime implementation deferred by single-agent guardrail.
- Acceptance criteria: design spec exists, but runtime feature remains disabled unless product direction changes.
- Owner: architecture maintainer
- Status: Deferred (guardrail)
- Target release or cycle: future cycle

2. Enterprise Managed Settings and Policy Controls
- Problem statement: the reference implementation supports managed settings delivery and enterprise policy enforcement across scopes.
- Scope: managed configuration channels, policy precedence, and validation/observability.
- Acceptance criteria: enterprise admins can centrally enforce policy that cannot be overridden locally.
- Owner: enterprise/config maintainer
- Status: Planned
- Target release or cycle: future cycle

3. Multi-Provider Enterprise Deployment Paths
- Problem statement: the reference implementation docs cover multiple hosted deployment variants (for example Bedrock, Vertex, and other enterprise providers).
- Scope: add deployment adapters and guidance for enterprise environments and gateways.
- Acceptance criteria: provider-specific deployment modes are documented, configurable, and testable.
- Owner: operator/runtime maintainer
- Status: Planned
- Target release or cycle: future cycle

4. Code Review + CI/CD Integration Parity
- Problem statement: the reference implementation integrates with GitHub/GitLab automation flows for review and issue triage.
- Scope: standardized automation entrypoints and operational controls for CI usage.
- Acceptance criteria: CI can run deterministic review/triage workflows with policy-safe tool access.
- Owner: operator/integrations maintainer
- Status: Planned
- Target release or cycle: future cycle

5. Cross-platform Voice Hardening (macOS/Linux/Windows)
- Problem statement: audio stacks and key handling differ by OS, which can cause inconsistent voice behavior.
- Scope: harden behavior on all three OSes, add fallback/error messaging, add smoke-test matrix coverage, and publish troubleshooting guidance.
- Acceptance criteria: smoke tests pass on macOS/Linux/Windows, voice failures return actionable guidance, and non-OpenAI reasoning providers still work with OpenAI-based voice legs.
- Owner: platform/qa maintainer
- Status: Planned
- Target release or cycle: current cycle

## Intake Rule

New work should be appended with:

- Priority (`P0`, `P1`, `P2`)
- Problem statement
- Acceptance criteria
- Owner
- Target release or cycle
