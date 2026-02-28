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
- `meow session list|resume|export`
- `meow config init|validate|path`

### Config Separation

- Runtime config for Meow users: `~/.meow-soma/config.toml`
- Development multi-agent config for Codex CLI only: `.codex/config.toml`

### Reference Files

- Runtime config template: `config/meow.example.toml`
- Local dev config (state in repo): `config/dev.local.toml`
- Master plan: `docs/MEOWSOMA_MASTER_PLAN.md`
- Detailed phase plan: `docs/PHASE_IMPLEMENTATION_PLAN.md`
- Config responsibilities: `docs/CONFIG.md`
- Contributor/agent collaboration guide: `AGENTS.md`

### Local-Only Dev Testing (No `~/.meow-soma`)

Use the local config to keep all state inside this repo:

- `cargo run -- --config config/dev.local.toml config validate`
- `cargo run -- --config config/dev.local.toml session list`
- `cargo run -- --config config/dev.local.toml ask "hello"`
