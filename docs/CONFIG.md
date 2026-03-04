# Configuration Model

## 1) Runtime Config (Meow)
- Canonical path: `~/.meow-soma/config.toml`
- Owned by end users of `meow`
- Used at runtime by CLI commands (`ask`, `run`, `tool`, `mcp`, `session`, and default `meow` TUI mode)

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

First-run helper:
- `meow config setup --provider <openai|anthropic|ollama>` initializes config and sets `runtime.default_provider`
- `meow config init` remains available for raw template creation without provider selection guidance

Retry and timeout:
- `runtime.retry_budget` controls provider retry attempts for retryable failures
- `providers.<name>.timeout_secs` controls HTTP request timeout per attempt
- Retryable errors include timeout, rate-limit, and most server-side failures
- `runtime.max_steps` is used as the bounded recent-context window size for TUI chat/run prompt context loading

Security/runtime policy details:
- Policy decisions are classified as `allow`, `approve_required`, or `deny`
- Approval audit rows persist both human-readable reason and machine-readable `reason_code`
- `fs.write` is constrained to the current workspace root by default
- Extra write roots can be allowed via environment variable `MEOW_FS_WRITE_ALLOW_ROOTS`
  - Use OS path-list format (`:` on macOS/Linux)

Persistence and backup lifecycle:
- Local state is stored in SQLite at `storage.sqlite_path` (default under `~/.meow-soma/`)
- On startup, `meow` runs SQLite `quick_check` integrity validation before schema migrations
- Schema upgrades are automatic via transactional, idempotent migrations (`schema_migrations` table)
- Use `meow session export --all -o backup.json` for full-state backup snapshots
- Use `meow session import backup.json` to restore a snapshot onto local state
- Single-session exports remain available via `meow session export <SESSION_ID> [--format json|markdown]`

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
