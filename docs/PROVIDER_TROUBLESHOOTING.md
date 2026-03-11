# Provider Troubleshooting Runbook

Use this runbook when `meow ask` or `meow run` fails in provider calls.

## Error Format You Will See

Provider failures are emitted in this format:

```text
provider=<name> model=<model> kind=<error_kind> status=<http_status_or_None> message=<detail>
```

Example:

```text
provider=openai model=gpt-4.1 kind=timeout status=Some(504) message=...
```

`kind` values used by runtime:
- `auth`
- `rate_limit`
- `timeout`
- `invalid_request`
- `server`
- `transport`
- `parse`
- `unknown`

## Quick Triage Flow

1. Pick one config scope and keep it consistent for all commands in this runbook.
- Canonical runtime scope:
  - `CFG=()`
- Repo-local dev scope:
  - `CFG=(--config config/dev.local.toml)`

2. Reproduce once and capture the full error line.
- `meow "${CFG[@]}" ask "health check"`

3. Confirm the config file and validate it.
- `CONFIG_PATH="$(meow "${CFG[@]}" config path)"`
- `echo "$CONFIG_PATH"`
- `meow "${CFG[@]}" config validate`
- `sed -n '1,220p' "$CONFIG_PATH"`

4. Confirm active provider settings (`runtime.default_provider`, endpoint, model, timeout).
- `rg -n "default_provider|retry_budget|\\[providers\\.|model|endpoint|api_key_env|timeout_secs" "$CONFIG_PATH"`

5. Branch on `kind=` and follow the matching section below.

6. Check failure trend quickly.
- `meow "${CFG[@]}" metrics summary --days 1`

Note:
- Runtime retries retryable provider failures using `runtime.retry_budget` (default `2`).
- `providers.<name>.timeout_secs` is per-attempt timeout (default `60`).
- Provider selection starts from `runtime.default_provider`; if that provider block is missing, runtime falls back to the next configured provider (`openai`, `anthropic`, `ollama`).

## Provider Baselines (Defaults)

- OpenAI: endpoint `https://api.openai.com/v1`, env `OPENAI_API_KEY`, model `gpt-4.1`
- Anthropic: endpoint `https://api.anthropic.com`, env `ANTHROPIC_API_KEY`, model `claude-3-7-sonnet-latest`
- Ollama: endpoint `http://127.0.0.1:11434`, no API key by default, model `llama3.1:8b`

## `kind=auth`

Typical causes:
- Missing or empty credential environment variable
- Wrong/expired credential (usually HTTP `401` or `403`)
- Misconfigured `api_key_env`

OpenAI diagnostics:
- `echo "${OPENAI_API_KEY:+set}"`
- `curl -sS https://api.openai.com/v1/models -H "Authorization: Bearer $OPENAI_API_KEY" -o /tmp/openai_models.json -w "\nHTTP %{http_code}\n"`

OpenAI recovery:
- `export OPENAI_API_KEY=<valid_key>`
- If using custom env name, set `[providers.openai].api_key_env` in config and export that variable.

Anthropic diagnostics:
- `echo "${ANTHROPIC_API_KEY:+set}"`
- `curl -sS https://api.anthropic.com/v1/messages -H "x-api-key: $ANTHROPIC_API_KEY" -H "anthropic-version: 2023-06-01" -H "content-type: application/json" -d '{"model":"claude-3-7-sonnet-latest","max_tokens":8,"messages":[{"role":"user","content":"ping"}]}' -o /tmp/anthropic_ping.json -w "\nHTTP %{http_code}\n"`

Anthropic recovery:
- `export ANTHROPIC_API_KEY=<valid_key>`
- If using custom env name, set `[providers.anthropic].api_key_env` and export that variable.

Ollama diagnostics:
- `rg -n "\\[providers\\.ollama\\]|api_key_env" "$CONFIG_PATH"`
- `curl -sS http://127.0.0.1:11434/api/tags -o /tmp/ollama_tags.json -w "\nHTTP %{http_code}\n"`

Ollama recovery:
- Remove `api_key_env` override for `[providers.ollama]` unless your Ollama endpoint requires auth.
- Ensure local server is running: `ollama serve`

## `kind=rate_limit`

Typical causes:
- Provider returned HTTP `429`
- Request volume exceeds provider/project quota window

Diagnostics:
- `meow "${CFG[@]}" metrics summary --days 1`
- `rg -n "retry_budget|timeout_secs" "$CONFIG_PATH"`

OpenAI / Anthropic targeted probe:
- Run one minimal request (see `auth` curl probes above) and check for `HTTP 429`.

Ollama targeted probe:
- `curl -sS http://127.0.0.1:11434/api/generate -H "content-type: application/json" -d '{"model":"llama3.1:8b","prompt":"ping","stream":false}' -o /tmp/ollama_generate.json -w "\nHTTP %{http_code}\n"`

Recovery:
- Reduce parallel traffic to the same provider.
- Increase retry attempts if needed by raising `runtime.retry_budget` from default `2`.
- Retry after cooldown when hosted provider quota windows reset.

## `kind=timeout`

Typical causes:
- Network timeout (`request timed out`)
- HTTP timeout responses (`408`/`504`)
- Provider is reachable but too slow for current `timeout_secs`

Diagnostics:
- `rg -n "retry_budget|timeout_secs" "$CONFIG_PATH"`
- OpenAI: `curl -sS --max-time 15 https://api.openai.com/v1/models -H "Authorization: Bearer $OPENAI_API_KEY" -o /tmp/openai_timeout_probe.json -w "\nHTTP %{http_code}\n"`
- Anthropic: `curl -sS --max-time 15 https://api.anthropic.com/v1/messages -H "x-api-key: $ANTHROPIC_API_KEY" -H "anthropic-version: 2023-06-01" -H "content-type: application/json" -d '{"model":"claude-3-7-sonnet-latest","max_tokens":8,"messages":[{"role":"user","content":"ping"}]}' -o /tmp/anthropic_timeout_probe.json -w "\nHTTP %{http_code}\n"`
- Ollama: `curl -sS --max-time 15 http://127.0.0.1:11434/api/tags -o /tmp/ollama_timeout_probe.json -w "\nHTTP %{http_code}\n"`

Recovery:
- Raise `providers.<name>.timeout_secs` above `60` for slower models/networks.
- Keep or raise `runtime.retry_budget` for transient failures.
- For Ollama, make sure server/model are ready: `ollama serve` and `ollama list`.

## `kind=transport` (brief)

Meaning:
- Network/connect/request/read failure before valid provider response

Diagnostics:
- Verify `endpoint` in config matches provider default or your intended custom endpoint.
- OpenAI: `curl -sS https://api.openai.com/v1/models -H "Authorization: Bearer $OPENAI_API_KEY" -o /tmp/openai_transport_probe.json -w "\nHTTP %{http_code}\n"`
- Anthropic: `curl -sS https://api.anthropic.com/v1/messages -H "x-api-key: $ANTHROPIC_API_KEY" -H "anthropic-version: 2023-06-01" -H "content-type: application/json" -d '{"model":"claude-3-7-sonnet-latest","max_tokens":8,"messages":[{"role":"user","content":"ping"}]}' -o /tmp/anthropic_transport_probe.json -w "\nHTTP %{http_code}\n"`
- Ollama: `curl -sS http://127.0.0.1:11434/api/tags -o /tmp/ollama_transport_probe.json -w "\nHTTP %{http_code}\n"`

Recovery:
- Fix endpoint DNS/host/port/TLS route.
- Restore local Ollama daemon if down (`ollama serve`).

## `kind=server` (brief)

Meaning:
- Provider returned HTTP `5xx`

Diagnostics:
- Capture exact `status=` and `message=` from error line.
- Confirm issue with the provider-specific curl probe.

Recovery:
- Retry (server errors are retryable by runtime).
- Temporarily switch provider by setting `runtime.default_provider` to another configured provider and rerun.

## `kind=invalid_request` (brief)

Meaning:
- Provider returned HTTP `4xx` that is not auth/rate-limit

Diagnostics:
- Check configured `model` and `endpoint` values in config.
- Re-run a minimal provider curl request with the same model.

Recovery:
- Correct invalid model/endpoint/request setup in `[providers.<name>]`.
- Re-validate config and rerun:
  - `meow "${CFG[@]}" config validate`
  - `meow "${CFG[@]}" ask "health check"`

## `kind=parse` (brief)

Meaning:
- Response was not in the expected provider schema (invalid JSON/event shape or missing expected text fields)

Diagnostics:
- Confirm endpoint points to real provider API (not HTML/proxy/error page).
- Inspect raw probe body file in `/tmp` from curl diagnostics above.

Recovery:
- Fix endpoint/proxy to return provider-native JSON/SSE payloads.
- Retry once endpoint response format is corrected.

## `kind=unknown` (brief)

Meaning:
- Failure did not map to a known classification bucket.

Diagnostics:
- Capture the full error line and repro command.
- Re-run with the provider-specific diagnostics from this runbook.

Recovery:
- Treat as temporary incident first: retry once, then switch to another configured provider if needed.
- If reproducible, open an issue with full error line, provider, model, config snippet (redacted), and repro steps.
