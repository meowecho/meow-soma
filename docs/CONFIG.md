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

Provider authentication and runtime behavior:
- OpenAI:
  - Endpoint default: `https://api.openai.com/v1`
  - API key env default: `OPENAI_API_KEY`
- Anthropic:
  - Endpoint default: `https://api.anthropic.com`
  - API key env default: `ANTHROPIC_API_KEY`
- Ollama:
  - Endpoint default: `http://127.0.0.1:11434`
  - No API key required by default

Retry and timeout:
- `runtime.retry_budget` controls provider retry attempts for retryable failures
- `providers.<name>.timeout_secs` controls HTTP request timeout per attempt
- Retryable errors include timeout, rate-limit, and most server-side failures
- `runtime.max_steps` is used as the bounded recent-context window size for chat/run prompt context loading

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
