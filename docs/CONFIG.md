# Configuration Model

## 1) Runtime Config (Meow)
- Canonical path: `~/.meow-soma/config.toml`
- Owned by end users of `meow`
- Used at runtime by CLI commands (`chat`, `ask`, `run`, `tool`, `mcp`, `session`)

Main sections:
- `[project]`
- `[runtime]`
- `[security]`
- `[storage]`
- `[mcp]`
- `[providers.*]`
- `[[profiles]]`

Template source in this repo:
- `config/meow.example.toml`

## 2) Development Config (Codex CLI)
- Path in repo: `.codex/config.toml`
- Used only during development to define a multi-agent team in Codex CLI
- Not loaded by `meow` runtime
- Follows OpenAI Codex multi-agent format (`[features]`, `[agents]`, `[agents.<name>]`)
- Per-agent model settings are stored in `.codex/agents/*.toml`

Agent roles defined:
- orchestrator
- planner
- researcher
- coder
- reviewer
- operator

## 3) Why Separation Matters
- Prevent runtime behavior from depending on development tooling choices
- Keep production/runtime config stable for end users
- Allow dev team orchestration changes without affecting `meow` users

## 4) Operational Guidance
- If changing runtime defaults, update both:
  - `src/config.rs` defaults
  - `config/meow.example.toml`
- If changing dev workflow roles/rules, update only:
  - `.codex/config.toml`
  - `.codex/agents/*.toml`
