# Changelog

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog and this project follows Semantic Versioning.

## [Unreleased]

## [0.1.0] - 2026-03-09

### Added
- `meow` unified CLI with default interactive TUI plus task-focused commands:
  `ask`, `run`, `tool`, `mcp`, `session`, and `config`
- Provider runtime integrations for `openai`, `anthropic`, and `ollama` with
  timeout and retry controls
- Streaming response pipeline (provider stream -> runtime -> TUI feed)
- SQLite-backed persistent state for sessions, messages, runs, tool calls, and
  approval decisions
- Session export/import flows:
  - Single-session export as JSON or Markdown
  - Full backup snapshot export/import as JSON
- Built-in tool registry with `echo`, `fs.read`, `fs.write`, and `shell`
- Workspace safety boundary for `fs.write` with optional extra roots via
  `MEOW_FS_WRITE_ALLOW_ROOTS`
- Policy engine with allow/deny/approval decisions for risky shell/tool actions
- In-feed approval gate for risky `/tool` execution (`y/yes` or `n/no`)
- MCP stdio server (`meow mcp serve --transport stdio`) with protocol
  `meow.mcp.v1` and methods `ping`, `server/info`, `tools/list`, `tools/call`
- Runtime config helpers: `meow config init|setup|validate|path`
- TUI command and navigation features:
  - Slash commands (`/help`, `/new`, `/tool`, `/provider`, `/palette`, ...)
  - Command palette (`Ctrl+P`)
  - Prompt history + reverse search (`Up/Down`, `Ctrl+R`)
  - Transcript scrolling (`PgUp/PgDn`, `Home`, `End`)
  - Thinking status shown inline in the feed while waiting for responses

### Fixed
- Chat feed scrolling and long-transcript behavior to preserve latest-message visibility
- TUI stability fixes for long conversation rendering (including overflow/panic edge cases)
- Provider error classification surface for auth/rate-limit/timeout failures
