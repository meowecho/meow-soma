use std::env;
use std::thread;
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use reqwest::StatusCode;
use reqwest::blocking::Client;
use serde_json::{Value, json};

use crate::config::{MeowConfig, ProviderConfig, ProvidersConfig};

pub trait LlmProvider: Send + Sync {
    fn name(&self) -> &str;
    fn model(&self) -> &str;
    fn complete(&self, prompt: &str) -> Result<String>;
}

pub fn build_provider(config: &MeowConfig) -> Box<dyn LlmProvider> {
    let mut candidates = Vec::new();
    candidates.push(config.runtime.default_provider.clone());
    for provider in ["openai", "anthropic", "ollama"] {
        if !candidates.iter().any(|item| item == provider) {
            candidates.push(provider.to_owned());
        }
    }

    for candidate in candidates {
        if let Some(provider) =
            provider_from_name(&candidate, &config.providers, config.runtime.retry_budget)
        {
            return provider;
        }
    }

    Box::new(UnavailableProvider {
        provider_name: config.runtime.default_provider.clone(),
        message: "no configured provider available".to_owned(),
    })
}

fn provider_from_name(
    name: &str,
    providers: &ProvidersConfig,
    retry_budget: u8,
) -> Option<Box<dyn LlmProvider>> {
    match name {
        "openai" => providers.openai.as_ref().map(|cfg| {
            Box::new(HttpProvider::new(
                ProviderKind::OpenAi,
                cfg.clone(),
                retry_budget,
            )) as Box<dyn LlmProvider>
        }),
        "anthropic" => providers.anthropic.as_ref().map(|cfg| {
            Box::new(HttpProvider::new(
                ProviderKind::Anthropic,
                cfg.clone(),
                retry_budget,
            )) as Box<dyn LlmProvider>
        }),
        "ollama" => providers.ollama.as_ref().map(|cfg| {
            Box::new(HttpProvider::new(
                ProviderKind::Ollama,
                cfg.clone(),
                retry_budget,
            )) as Box<dyn LlmProvider>
        }),
        _ => None,
    }
}

struct UnavailableProvider {
    provider_name: String,
    message: String,
}

impl LlmProvider for UnavailableProvider {
    fn name(&self) -> &str {
        &self.provider_name
    }

    fn model(&self) -> &str {
        "unavailable"
    }

    fn complete(&self, _prompt: &str) -> Result<String> {
        bail!(
            "provider '{}' unavailable: {}",
            self.provider_name,
            self.message
        )
    }
}

#[derive(Clone, Copy)]
enum ProviderKind {
    OpenAi,
    Anthropic,
    Ollama,
}

struct HttpProvider {
    kind: ProviderKind,
    config: ProviderConfig,
    retry_budget: u8,
    client: Client,
}

impl HttpProvider {
    fn new(kind: ProviderKind, config: ProviderConfig, retry_budget: u8) -> Self {
        Self {
            kind,
            config,
            retry_budget,
            client: Client::new(),
        }
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(self.config.timeout_secs.max(1))
    }

    fn endpoint(&self, fallback: &str) -> String {
        self.config
            .endpoint
            .clone()
            .unwrap_or_else(|| fallback.to_owned())
            .trim_end_matches('/')
            .to_owned()
    }

    fn complete_once(&self, prompt: &str) -> std::result::Result<String, ProviderError> {
        match self.kind {
            ProviderKind::OpenAi => self.complete_openai(prompt),
            ProviderKind::Anthropic => self.complete_anthropic(prompt),
            ProviderKind::Ollama => self.complete_ollama(prompt),
        }
    }

    fn complete_openai(&self, prompt: &str) -> std::result::Result<String, ProviderError> {
        let api_key = resolve_api_key(self.config.api_key_env.as_deref(), "OPENAI_API_KEY")?;
        let url = format!(
            "{}/chat/completions",
            self.endpoint("https://api.openai.com/v1")
        );
        let body = json!({
            "model": self.config.model,
            "messages": [{"role": "user", "content": prompt}],
            "temperature": 0.2
        });

        let response = self
            .client
            .post(&url)
            .timeout(self.timeout())
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .map_err(classify_transport_error)?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(classify_http_error(status, &body));
        }

        let json: Value = response
            .json()
            .map_err(|err| ProviderError::parse(format!("invalid OpenAI response: {err}")))?;

        parse_openai_text(&json).ok_or_else(|| {
            ProviderError::parse("missing OpenAI choices[0].message.content".to_owned())
        })
    }

    fn complete_anthropic(&self, prompt: &str) -> std::result::Result<String, ProviderError> {
        let api_key = resolve_api_key(self.config.api_key_env.as_deref(), "ANTHROPIC_API_KEY")?;
        let url = format!("{}/v1/messages", self.endpoint("https://api.anthropic.com"));
        let body = json!({
            "model": self.config.model,
            "max_tokens": 1024,
            "messages": [{"role": "user", "content": prompt}],
        });

        let response = self
            .client
            .post(&url)
            .timeout(self.timeout())
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .map_err(classify_transport_error)?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(classify_http_error(status, &body));
        }

        let json: Value = response
            .json()
            .map_err(|err| ProviderError::parse(format!("invalid Anthropic response: {err}")))?;

        parse_anthropic_text(&json)
            .ok_or_else(|| ProviderError::parse("missing Anthropic content text block".to_owned()))
    }

    fn complete_ollama(&self, prompt: &str) -> std::result::Result<String, ProviderError> {
        let url = format!("{}/api/generate", self.endpoint("http://127.0.0.1:11434"));
        let body = json!({
            "model": self.config.model,
            "prompt": prompt,
            "stream": false,
        });

        let response = self
            .client
            .post(&url)
            .timeout(self.timeout())
            .json(&body)
            .send()
            .map_err(classify_transport_error)?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(classify_http_error(status, &body));
        }

        let json: Value = response
            .json()
            .map_err(|err| ProviderError::parse(format!("invalid Ollama response: {err}")))?;

        parse_ollama_text(&json)
            .ok_or_else(|| ProviderError::parse("missing Ollama response field".to_owned()))
    }
}

impl LlmProvider for HttpProvider {
    fn name(&self) -> &str {
        match self.kind {
            ProviderKind::OpenAi => "openai",
            ProviderKind::Anthropic => "anthropic",
            ProviderKind::Ollama => "ollama",
        }
    }

    fn model(&self) -> &str {
        &self.config.model
    }

    fn complete(&self, prompt: &str) -> Result<String> {
        let mut last_error: Option<ProviderError> = None;

        for attempt in 0..=self.retry_budget {
            match self.complete_once(prompt) {
                Ok(text) => return Ok(text),
                Err(err) => {
                    let should_retry = err.retryable && attempt < self.retry_budget;
                    if should_retry {
                        last_error = Some(err);
                        thread::sleep(retry_delay_ms(attempt));
                        continue;
                    }

                    return Err(anyhow!(
                        "provider={} model={} kind={} status={:?} message={}",
                        self.name(),
                        self.model(),
                        err.kind.as_str(),
                        err.status,
                        err.message
                    ));
                }
            }
        }

        let err = last_error.unwrap_or_else(|| ProviderError::unknown("provider request failed"));
        Err(anyhow!(
            "provider={} model={} kind={} status={:?} message={}",
            self.name(),
            self.model(),
            err.kind.as_str(),
            err.status,
            err.message
        ))
    }
}

#[derive(Clone, Copy, Debug)]
enum ProviderErrorKind {
    Auth,
    RateLimit,
    Timeout,
    InvalidRequest,
    Server,
    Transport,
    Parse,
    Unknown,
}

impl ProviderErrorKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Auth => "auth",
            Self::RateLimit => "rate_limit",
            Self::Timeout => "timeout",
            Self::InvalidRequest => "invalid_request",
            Self::Server => "server",
            Self::Transport => "transport",
            Self::Parse => "parse",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone)]
struct ProviderError {
    kind: ProviderErrorKind,
    status: Option<u16>,
    message: String,
    retryable: bool,
}

impl ProviderError {
    fn auth(message: impl Into<String>) -> Self {
        Self {
            kind: ProviderErrorKind::Auth,
            status: None,
            message: message.into(),
            retryable: false,
        }
    }

    fn parse(message: impl Into<String>) -> Self {
        Self {
            kind: ProviderErrorKind::Parse,
            status: None,
            message: message.into(),
            retryable: false,
        }
    }

    fn transport(message: impl Into<String>, retryable: bool) -> Self {
        Self {
            kind: ProviderErrorKind::Transport,
            status: None,
            message: message.into(),
            retryable,
        }
    }

    fn timeout(message: impl Into<String>) -> Self {
        Self {
            kind: ProviderErrorKind::Timeout,
            status: None,
            message: message.into(),
            retryable: true,
        }
    }

    fn unknown(message: impl Into<String>) -> Self {
        Self {
            kind: ProviderErrorKind::Unknown,
            status: None,
            message: message.into(),
            retryable: false,
        }
    }
}

fn resolve_api_key(
    env_name: Option<&str>,
    default_name: &str,
) -> std::result::Result<String, ProviderError> {
    let key_name = env_name.unwrap_or(default_name);
    match env::var(key_name) {
        Ok(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(ProviderError::auth(format!(
            "missing API credential in environment variable '{key_name}'"
        ))),
    }
}

fn classify_transport_error(err: reqwest::Error) -> ProviderError {
    if err.is_timeout() {
        return ProviderError::timeout(format!("request timed out: {err}"));
    }

    if err.is_connect() || err.is_request() || err.is_body() {
        return ProviderError::transport(format!("transport error: {err}"), true);
    }

    ProviderError::transport(format!("request failed: {err}"), false)
}

fn classify_http_error(status: StatusCode, body: &str) -> ProviderError {
    let message = extract_error_message(body).unwrap_or_else(|| body.to_owned());
    let status_code = Some(status.as_u16());

    if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
        return ProviderError {
            kind: ProviderErrorKind::Auth,
            status: status_code,
            message,
            retryable: false,
        };
    }

    if status == StatusCode::TOO_MANY_REQUESTS {
        return ProviderError {
            kind: ProviderErrorKind::RateLimit,
            status: status_code,
            message,
            retryable: true,
        };
    }

    if matches!(
        status,
        StatusCode::REQUEST_TIMEOUT | StatusCode::GATEWAY_TIMEOUT
    ) {
        return ProviderError {
            kind: ProviderErrorKind::Timeout,
            status: status_code,
            message,
            retryable: true,
        };
    }

    if status.is_server_error() {
        return ProviderError {
            kind: ProviderErrorKind::Server,
            status: status_code,
            message,
            retryable: true,
        };
    }

    if status.is_client_error() {
        return ProviderError {
            kind: ProviderErrorKind::InvalidRequest,
            status: status_code,
            message,
            retryable: false,
        };
    }

    ProviderError {
        kind: ProviderErrorKind::Unknown,
        status: status_code,
        message,
        retryable: false,
    }
}

fn retry_delay_ms(attempt: u8) -> Duration {
    let base: u64 = 150;
    let factor = 1_u64 << attempt.min(4);
    Duration::from_millis(base * factor)
}

fn extract_error_message(raw: &str) -> Option<String> {
    let value: Value = serde_json::from_str(raw).ok()?;

    if let Some(msg) = value.pointer("/error/message").and_then(Value::as_str) {
        return Some(msg.to_owned());
    }

    if let Some(msg) = value.get("message").and_then(Value::as_str) {
        return Some(msg.to_owned());
    }

    None
}

fn parse_openai_text(value: &Value) -> Option<String> {
    let content = value.pointer("/choices/0/message/content")?;

    if let Some(text) = content.as_str() {
        return Some(text.to_owned());
    }

    if let Some(parts) = content.as_array() {
        let merged = parts
            .iter()
            .filter_map(|part| {
                part.get("text")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                    .or_else(|| part.as_str().map(ToOwned::to_owned))
            })
            .collect::<Vec<_>>()
            .join("\n");
        if !merged.trim().is_empty() {
            return Some(merged);
        }
    }

    None
}

fn parse_anthropic_text(value: &Value) -> Option<String> {
    let blocks = value.get("content")?.as_array()?;
    let merged = blocks
        .iter()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");

    if merged.trim().is_empty() {
        None
    } else {
        Some(merged)
    }
}

fn parse_ollama_text(value: &Value) -> Option<String> {
    value
        .get("response")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    use super::*;

    #[test]
    fn parse_openai_text_supports_string_and_array() {
        let simple = json!({
            "choices": [
                {
                    "message": {
                        "content": "hello"
                    }
                }
            ]
        });
        assert_eq!(parse_openai_text(&simple), Some("hello".to_owned()));

        let rich = json!({
            "choices": [
                {
                    "message": {
                        "content": [
                            {"type": "output_text", "text": "line 1"},
                            {"type": "output_text", "text": "line 2"}
                        ]
                    }
                }
            ]
        });
        assert_eq!(parse_openai_text(&rich), Some("line 1\nline 2".to_owned()));
    }

    #[test]
    fn parse_anthropic_text_reads_text_blocks() {
        let payload = json!({
            "content": [
                {"type": "text", "text": "part a"},
                {"type": "text", "text": "part b"}
            ]
        });
        assert_eq!(
            parse_anthropic_text(&payload),
            Some("part a\npart b".to_owned())
        );
    }

    #[test]
    fn classify_http_error_maps_status() {
        let err_auth = classify_http_error(
            StatusCode::UNAUTHORIZED,
            "{\"error\": {\"message\": \"nope\"}}",
        );
        assert_eq!(err_auth.kind.as_str(), "auth");
        assert!(!err_auth.retryable);

        let err_rate = classify_http_error(StatusCode::TOO_MANY_REQUESTS, "{\"message\":\"slow\"}");
        assert_eq!(err_rate.kind.as_str(), "rate_limit");
        assert!(err_rate.retryable);

        let err_server = classify_http_error(StatusCode::BAD_GATEWAY, "upstream");
        assert_eq!(err_server.kind.as_str(), "server");
        assert!(err_server.retryable);
    }

    #[test]
    fn ollama_provider_retries_and_succeeds() {
        let (endpoint, requests, handle) = spawn_mock_server(vec![
            MockResponse {
                status: 500,
                body: "{\"error\":\"boom\"}".to_owned(),
            },
            MockResponse {
                status: 200,
                body: "{\"response\":\"ok from ollama\"}".to_owned(),
            },
        ]);

        let provider = HttpProvider::new(
            ProviderKind::Ollama,
            ProviderConfig {
                model: "llama3.1:8b".to_owned(),
                endpoint: Some(endpoint),
                api_key_env: None,
                timeout_secs: 2,
            },
            1,
        );

        let output = provider
            .complete("hello")
            .expect("provider should succeed after retry");
        assert_eq!(output, "ok from ollama");

        let req1 = requests
            .recv_timeout(Duration::from_secs(1))
            .expect("first request should be captured");
        let req2 = requests
            .recv_timeout(Duration::from_secs(1))
            .expect("second request should be captured");

        assert!(req1.contains("POST /api/generate"));
        assert!(req2.contains("POST /api/generate"));

        handle.join().expect("mock server thread should join");
    }

    struct MockResponse {
        status: u16,
        body: String,
    }

    fn spawn_mock_server(
        responses: Vec<MockResponse>,
    ) -> (String, mpsc::Receiver<String>, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let addr = listener.local_addr().expect("resolve mock address");
        let (tx, rx) = mpsc::channel();

        let handle = thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().expect("accept mock connection");

                let mut buf = [0_u8; 16 * 1024];
                let read = stream.read(&mut buf).expect("read request bytes");
                let request = String::from_utf8_lossy(&buf[..read]).to_string();
                tx.send(request).expect("send captured request");

                let reason = reason_phrase(response.status);
                let payload = response.body;
                let wire = format!(
                    "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response.status,
                    reason,
                    payload.len(),
                    payload
                );

                stream
                    .write_all(wire.as_bytes())
                    .expect("write mock response");
                stream.flush().expect("flush mock response");
            }
        });

        (format!("http://{addr}"), rx, handle)
    }

    fn reason_phrase(status: u16) -> &'static str {
        match status {
            200 => "OK",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            408 => "Request Timeout",
            429 => "Too Many Requests",
            500 => "Internal Server Error",
            502 => "Bad Gateway",
            504 => "Gateway Timeout",
            _ => "Mock Status",
        }
    }
}
