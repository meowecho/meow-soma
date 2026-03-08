# Meow Soma

**Pronunciation:** /ˈso.ma/ (Greek) · /ˈsoʊmə/ (English)  
*(soh-ma / soh-muh)*

---

## Meow Soma – The Body of Intelligence  
**Enter the body. Command the mind.**

Meow Soma is an AI-native terminal environment designed to unify intelligence, execution, and collaboration inside a single command.

It is not just a CLI tool.  
It is the embodied runtime where AI reasoning meets action.

---

## Vision

Modern AI tools are fragmented:
- One tool for chat  
- Another for code  
- Another for automation  
- Another for agents  

Meow Soma brings them into one coherent body.

It is designed as:

- A unified AI CLI  
- A context-aware multi-repository environment  
- An extensible agent runtime  
- A foundation for future AI-native workflows  

Where other tools assist, Meow Soma inhabits.

---

## Philosophy

Every intelligence needs a body.

The model is the mind.  
The runtime is the body.  
The terminal is the gateway.

Meow Soma is the body of intelligence —  
where thought becomes execution.

---

## Core Principles

- **Embodied Intelligence** — AI should act, not just respond.  
- **Context First** — Projects, repositories, and workflows are first-class citizens.  
- **Extensible by Design** — Providers, tools, memory, and agents are modular.  
- **Shell-Native** — Built for developers who live in the terminal.  
- **Future-Ready** — Designed to evolve into an AI-native operating layer.  

---

## What Meow Soma Aims to Become

- A unified AI CLI (`meow`)  
- A programmable agent runtime  
- A collaborative cowork environment  
- A long-term AI-native shell ecosystem  

---

If intelligence is the mind,  
Meow Soma is the body.

---

## Current MVP Scaffold (Implemented)

This repository now includes a working Rust CLI scaffold with command name `meow`.

### Command Surface

- `meow` (default: start full-screen TUI)
- `meow ask "<prompt>"`
- `meow run "<goal>"`
- `meow tool list`
- `meow tool exec <tool> ... [--approve]`
- `meow mcp serve --transport stdio`
- `meow session list|resume|export|import`
- `meow config init|setup|validate|path`

### Config Separation

- Runtime config for Meow users: `~/.meow-soma/config.toml`
- Development multi-agent config for Codex CLI only: `.codex/config.toml`

### Reference Files

- Runtime config template: `config/meow.example.toml`
- Local dev config (state in repo): `config/dev.local.toml`
- Master plan: `docs/MEOWSOMA_MASTER_PLAN.md`
- Detailed phase plan: `docs/PHASE_IMPLEMENTATION_PLAN.md`
- Config responsibilities: `docs/CONFIG.md`
- Testing guide: `docs/TESTING.md`
- Install guide (macOS/Linux): `docs/INSTALL.md`
- Release process: `docs/RELEASE_PROCESS.md`
- Release checklist: `docs/RELEASE_CHECKLIST.md`
- Launch checklist: `docs/LAUNCH_CHECKLIST.md`
- Triage and SLA: `docs/TRIAGE_SLA.md`
- Metrics baseline: `docs/METRICS_BASELINE.md`
- Patch workflow: `docs/PATCH_RELEASE_WORKFLOW.md`
- Next-minor backlog: `docs/BACKLOG_V0_2.md`
- Changelog: `CHANGELOG.md`
- Contributor/agent collaboration guide: `AGENTS.md`

### Installation (macOS/Linux/Windows)

From source:

- `cargo build --release`
- `install -m 0755 target/release/meow /usr/local/bin/meow`
- `meow --help`

From a tagged release artifact:

- Download `meow-v<version>-<os>-<arch>.tar.gz` from GitHub Releases
- `tar -xzf meow-v<version>-<os>-<arch>.tar.gz`
- `install -m 0755 meow /usr/local/bin/meow`

See `docs/INSTALL.md` for step-by-step commands.

### First-Run Quickstart (Under 10 Minutes)

OpenAI:

- `meow config setup --provider openai`
- `export OPENAI_API_KEY=<your_openai_key>`
- `meow config validate`
- `meow ask "health check"`

Anthropic:

- `meow config setup --provider anthropic`
- `export ANTHROPIC_API_KEY=<your_anthropic_key>`
- `meow config validate`
- `meow ask "health check"`

Local Ollama:

- `meow config setup --provider ollama`
- `ollama serve`
- `meow config validate`
- `meow ask "health check"`

### Local-Only Dev Testing (No `~/.meow-soma`)

Use the local config to keep all state inside this repo:

- `cargo run -- --config config/dev.local.toml config validate`
- `cargo run -- --config config/dev.local.toml session list`
- `cargo run -- --config config/dev.local.toml ask "hello"`

### Test Commands

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo check`
- `cargo test`

See `docs/TESTING.md` for focused suites, fixture usage, and failure triage workflow.

### Release Commands

- Local package build: `scripts/release-local.sh v0.1.0`
- CI package build (tag-triggered): `.github/workflows/release.yml`
- CI build helper: `scripts/release-ci.sh`

### Session Backup and Restore

- Export one session as JSON (default): `meow session export <SESSION_ID> [-o path.json]`
- Export one session as Markdown: `meow session export <SESSION_ID> --format markdown [-o path.md]`
- Export full backup snapshot (all persisted state): `meow session export --all [-o backup.json]`
- Restore from backup snapshot: `meow session import <backup.json>`

Notes:
- `--all` exports a JSON backup snapshot intended for migration/restore workflows.
- Markdown export is supported only for single-session exports.
- `meow` runs SQLite integrity checks at startup and prints recovery guidance if corruption is detected.

### TUI Controls (Claude-Style Flow)

- Start TUI: `cargo run -- --config config/dev.local.toml`
- Send: `Enter`
- Streaming: assistant output renders chunk-by-chunk in chat feed
- Exit: `Esc`, `Ctrl+C`, `/quit`
- Slash commands: `/help`, `/home`, `/clear`, `/session`, `/provider`, `/profile <name>`, `/new [title]`, `/tool [name ...]`, `/palette`
- History: `Up` / `Down`
- History search: `Ctrl+R` (cycle matching command history)
- Command palette: `Ctrl+P` (filter + run quick commands)
- Scroll transcript: `PgUp` / `PgDn`, `Home`, `End`
- Clear input/transcript: `Ctrl+U` / `Ctrl+L`
- Inline approval: risky `/tool` actions show in-feed approval prompt (`y/yes` or `n/no`) without leaving TUI

### Tool Safety (Workspace Boundary)

- `fs.write` is restricted to your current workspace by default
- To allow extra write roots, set `MEOW_FS_WRITE_ALLOW_ROOTS` as an OS path list
  - macOS/Linux example: `export MEOW_FS_WRITE_ALLOW_ROOTS=\"/tmp:/Users/me/shared\"`

### MCP Protocol (v1)

- Transport: stdio (`meow mcp serve --transport stdio`)
- Protocol version: `meow.mcp.v1`
- Request correlation: `id` is preserved, and `meta.request_id` is always returned
- Discovery: `method = "tools/list"`

Example requests (one JSON line each):
- `{"version":"meow.mcp.v1","id":"1","method":"ping"}`
- `{"version":"meow.mcp.v1","id":"2","method":"server/info"}`
- `{"version":"meow.mcp.v1","id":"3","method":"tools/list"}`
- `{"version":"meow.mcp.v1","id":"4","method":"tools/call","params":{"tool":"echo","args":["hello"]}}`

Error codes:
- `invalid_json`
- `invalid_request`
- `unsupported_version`
- `unknown_method`
- `unknown_tool`
- `approval_required`
- `policy_denied`
- `tool_execution_error`
