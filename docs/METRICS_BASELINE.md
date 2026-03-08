# Metrics Baseline

This document defines the minimum operational metrics for launch and post-launch hardening.

## Metrics

- Startup time
  - Definition: time from process start to first usable output for `meow --help`
  - Goal: p95 <= 1.0s on reference hardware
- Response latency
  - Definition: time from submit to first token/response for `meow ask`
  - Goal: hosted provider p95 <= 6.0s, local provider p95 <= 10.0s
- Failure rate
  - Definition: percentage of commands ending with non-zero status or provider/tool error
  - Goal: <= 2% over rolling 24h usage window

## Collection Method

Startup time:

```bash
/usr/bin/time -p meow --help >/dev/null
```

Response latency (hosted provider):

```bash
/usr/bin/time -p meow ask "health check" >/dev/null
```

Response latency (local provider mode):

```bash
/usr/bin/time -p meow --config config/dev.local.toml ask "health check" >/dev/null
```

Failure rate:

- Count failing command runs from session logs and CI failures.
- Include provider errors (`auth`, `timeout`, `rate_limit`, `server`) and tool execution failures.

## Reporting Cadence

- Launch day: capture baseline snapshot
- Post-launch: 24h and 72h reports
- Ongoing: weekly summary during v0.1.x stabilization

## Alert Thresholds

- Startup p95 > 1.5s for two consecutive reports
- Response latency p95 exceeds goal by >= 30%
- Failure rate > 5% in 24h

Any threshold breach requires:

1. Issue creation with `triage` + severity label
2. Owner assignment
3. Mitigation plan and patch decision
