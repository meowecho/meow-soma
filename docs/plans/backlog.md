# Prioritized Backlog

This file is the active backlog for the current product cycle.
Archived completed cycle: `docs/plans/archive/backlog-2026-q1.md`.

## Current Cycle

- Status: Active (started 2026-03-12)

## P0

1. Voice Chat MVP Foundation (OpenAI STT/TTS + Push-to-talk)
- Problem statement: `meow` currently has no voice input/output path, so hands-free and spoken conversation workflows are not possible.
- Scope: add a baseline voice subsystem (capture/transcribe/synthesize/playback), add `[voice]` runtime config, and enforce no-audio-retention in MVP.
- Acceptance criteria: hold `F6` to record voice input, user speech appears as transcript text, assistant voice playback works, and no raw audio files are retained.
- Owner: runtime/provider maintainer
- Status: Planned
- Target release or cycle: current cycle

## P1

1. TUI Voice Interaction Flow
- Problem statement: the voice flow must run inside the existing TUI layout with clear in-feed state visibility.
- Scope: integrate `listening -> transcribing -> thinking -> speaking` into the current chat feed and add `/voice status|on|off|mute|unmute`.
- Acceptance criteria: end-to-end voice chat works in one screen, feed status transitions are clear, and mute/unmute behaves correctly.
- Owner: tui/runtime maintainer
- Status: Planned
- Target release or cycle: current cycle

## P2

1. Cross-platform Voice Hardening (macOS/Linux/Windows)
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
