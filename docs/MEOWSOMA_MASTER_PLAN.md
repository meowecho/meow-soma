# Meow-Soma Master Plan

## Goal
Build `meow-soma` as a Rust CLI with command name `meow` that unifies:
- Chat/Q&A in terminal
- Task execution for coding/automation workflows
- MCP tool exposure for external LLM clients

The runtime is intentionally single-agent for MVP.
Multi-agent is used only for development workflow via Codex CLI config.

## Execution Source of Truth
- High-level roadmap: this file
- Detailed phase-by-phase implementation: `docs/PHASE_IMPLEMENTATION_PLAN.md`
- Contributor and agent operating rules: `AGENTS.md`

## Locked Decisions
- Project name: `meow-soma`
- CLI binary: `meow`
- Runtime config path: `~/.meow-soma/config.toml`
- Dev multi-agent config path: `.codex/config.toml`
- Providers target: OpenAI + Anthropic + Ollama
- Security model: Permission Gate
- State model: SQLite + local files
- Interop model: MCP-first (stdio first)

## Architecture
- `src/cli.rs`: CLI command surface
- `src/app.rs`: command dispatcher and runtime entrypoints
- `src/config.rs`: config schema, load/init/validate, path resolution
- `src/providers.rs`: provider abstraction and adapter scaffold
- `src/runtime.rs`: single-agent runtime loop scaffold
- `src/tools.rs`: tool registry and built-in tools
- `src/policy.rs`: permission-gate policy decisions
- `src/state.rs`: SQLite state/audit persistence

## Runtime Commands
- `meow chat`
- `meow ask "<prompt>"`
- `meow run "<goal>"`
- `meow tool list|exec`
- `meow mcp serve`
- `meow session list|resume|export`
- `meow config init|validate|path`

## Milestones
1. Foundation
- Rust package + binary naming
- Config schema and canonical path
- Storage directory bootstrap

2. Chat and Ask
- Interactive session support
- One-shot prompts
- Message persistence

3. Run Loop
- Single-agent goal execution scaffold
- Run history persistence

4. Claude Code/Codex-Style TUI
- Full-screen terminal UI (`meow tui`)
- Streaming responses and inline approval flows
- Continuous transcript/status/tool timeline panels

5. Tool Body
- Built-in tool registry
- Policy-gated execution with approvals
- Tool/audit logs

6. MCP Serve
- stdio server loop
- JSON request/response contract
- Tool invocation via runtime gate

7. Hardening
- Replace stub providers with live adapters
- Add integration tests and failure handling
- Packaging and release tasks

## Delivery Artifacts
- `.codex/config.toml` for dev multi-agent workflow (OpenAI Codex multi-agent format)
- `.codex/agents/*.toml` per-agent model configs (`gpt-5.3-codex`, `model_reasoning_effort = "xhigh"`)
- `config/meow.example.toml` runtime template
- `docs/CONFIG.md` config responsibilities and separation

## Test Strategy
Functional:
- Chat session creation, persistence, and resume
- Ask one-shot path
- Run command path and run log insertion
- Tool list and tool exec policy behavior
- MCP stdio tool invocation

Failure scenarios:
- Invalid config values
- Risky command without approval
- Unknown tool requests
- DB initialization/read failures

## Risks and Mitigations
- Risk: confusion between runtime config and dev config
  - Mitigation: strict separation in docs and filenames
- Risk: shell tool safety gaps
  - Mitigation: denylist + approval-required gate + audit logging
- Risk: provider API drift
  - Mitigation: provider trait boundary + adapter isolation

## Immediate Next Steps
1. Start Phase 2.5 implementation (`meow tui` + streaming + inline approvals)
2. Expand unit/integration tests for policy + state + CLI flows
3. Harden tool safety and workspace boundaries (Phase 3)
