use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

use anyhow::{Result, anyhow};
use chrono::Utc;

use crate::providers::LlmProvider;

#[derive(Debug, Clone, Copy)]
pub enum RuntimeOperation {
    Chat,
    Ask,
    Run,
}

#[derive(Debug, Clone)]
pub struct ContextMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct RuntimeExecutionContext {
    pub operation: RuntimeOperation,
    pub profile: String,
    pub session_id: Option<String>,
    pub context_messages: Vec<ContextMessage>,
}

impl RuntimeExecutionContext {
    pub fn new(
        operation: RuntimeOperation,
        profile: impl Into<String>,
        session_id: Option<String>,
        context_messages: Vec<ContextMessage>,
    ) -> Self {
        Self {
            operation,
            profile: profile.into(),
            session_id,
            context_messages,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeResponse {
    pub text: String,
    pub started_at: String,
    pub finished_at: String,
    pub duration_ms: u128,
}

#[derive(Clone)]
pub struct CancellationToken {
    interrupted: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new(interrupted: Arc<AtomicBool>) -> Self {
        Self { interrupted }
    }

    pub fn clear(&self) {
        self.interrupted.store(false, Ordering::SeqCst);
    }

    #[cfg(test)]
    pub fn request_cancel(&self) {
        self.interrupted.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.interrupted.load(Ordering::SeqCst)
    }
}

pub struct RuntimeAgent {
    provider: Arc<dyn LlmProvider>,
}

impl RuntimeAgent {
    pub fn new(provider: Box<dyn LlmProvider>) -> Self {
        Self {
            provider: provider.into(),
        }
    }

    pub fn provider_name(&self) -> &str {
        self.provider.name()
    }

    pub fn provider_model(&self) -> &str {
        self.provider.model()
    }

    pub fn respond_with_context(
        &self,
        context: &RuntimeExecutionContext,
        prompt: &str,
        token: &CancellationToken,
    ) -> Result<RuntimeResponse> {
        let rendered = render_prompt(context, prompt, None);
        self.complete_with_interrupt(rendered, token)
    }

    pub fn run_goal_with_context(
        &self,
        context: &RuntimeExecutionContext,
        goal: &str,
        token: &CancellationToken,
    ) -> Result<RuntimeResponse> {
        let rendered = render_prompt(
            context,
            goal,
            Some(
                "Execution output format: concise summary, key steps, risks, and immediate next actions.",
            ),
        );
        self.complete_with_interrupt(rendered, token)
    }

    fn complete_with_interrupt(
        &self,
        rendered_prompt: String,
        token: &CancellationToken,
    ) -> Result<RuntimeResponse> {
        let started = Utc::now();
        let provider = Arc::clone(&self.provider);
        let (tx, rx) = mpsc::channel();

        thread::spawn(move || {
            let result = provider.complete(&rendered_prompt);
            let _ = tx.send(result);
        });

        loop {
            if token.is_cancelled() {
                return Err(anyhow!("request interrupted by user"));
            }

            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(result) => {
                    let text = result?;
                    let finished = Utc::now();
                    let duration_ms = (finished - started).num_milliseconds().max(0) as u128;
                    return Ok(RuntimeResponse {
                        text,
                        started_at: started.to_rfc3339(),
                        finished_at: finished.to_rfc3339(),
                        duration_ms,
                    });
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(anyhow!("provider worker disconnected before response"));
                }
            }
        }
    }
}

fn render_prompt(
    context: &RuntimeExecutionContext,
    prompt: &str,
    output_hint: Option<&str>,
) -> String {
    let mut rendered = String::new();
    rendered.push_str("System:\n");
    rendered.push_str(&profile_template(&context.profile, context.operation));

    if let Some(session_id) = &context.session_id {
        rendered.push_str("\n\nSession:\n");
        rendered.push_str(session_id);
    }

    if !context.context_messages.is_empty() {
        rendered.push_str("\n\nRecent context:\n");
        for msg in &context.context_messages {
            rendered.push_str("- ");
            rendered.push_str(&msg.role);
            rendered.push_str(": ");
            rendered.push_str(&normalize_inline(&msg.content));
            rendered.push('\n');
        }
    }

    rendered.push_str("\nUser request:\n");
    rendered.push_str(prompt.trim());

    if let Some(hint) = output_hint {
        rendered.push_str("\n\n");
        rendered.push_str(hint);
    }

    rendered
}

fn profile_template(profile: &str, op: RuntimeOperation) -> String {
    let profile_lower = profile.to_lowercase();

    let base = if profile_lower.contains("coding") {
        "You are a coding-focused terminal assistant. Be precise, produce actionable code guidance, and keep outputs deterministic."
    } else if profile_lower.contains("research") {
        "You are a research-focused terminal assistant. Prioritize factual clarity, cite assumptions, and summarize tradeoffs."
    } else {
        "You are a general terminal assistant. Stay concise, actionable, and safe when suggesting execution steps."
    };

    let mode = match op {
        RuntimeOperation::Chat => "Mode: interactive chat.",
        RuntimeOperation::Ask => "Mode: one-shot answer.",
        RuntimeOperation::Run => "Mode: goal execution planning.",
    };

    format!("{base} {mode}")
}

fn normalize_inline(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    struct RecordingProvider {
        name: String,
        model: String,
        response: String,
        delay_ms: u64,
        prompts: Arc<Mutex<Vec<String>>>,
    }

    impl RecordingProvider {
        fn new(response: &str, delay_ms: u64) -> (Self, Arc<Mutex<Vec<String>>>) {
            let prompts = Arc::new(Mutex::new(Vec::new()));
            (
                Self {
                    name: "test".to_owned(),
                    model: "test-model".to_owned(),
                    response: response.to_owned(),
                    delay_ms,
                    prompts: Arc::clone(&prompts),
                },
                prompts,
            )
        }
    }

    impl LlmProvider for RecordingProvider {
        fn name(&self) -> &str {
            &self.name
        }

        fn model(&self) -> &str {
            &self.model
        }

        fn complete(&self, prompt: &str) -> Result<String> {
            if self.delay_ms > 0 {
                thread::sleep(Duration::from_millis(self.delay_ms));
            }
            self.prompts
                .lock()
                .expect("prompt recorder lock should not fail")
                .push(prompt.to_owned());
            Ok(self.response.clone())
        }
    }

    fn token() -> CancellationToken {
        CancellationToken::new(Arc::new(AtomicBool::new(false)))
    }

    #[test]
    fn respond_with_context_includes_template_and_recent_messages() {
        let (provider, prompts) = RecordingProvider::new("ok", 0);
        let agent = RuntimeAgent::new(Box::new(provider));

        let context = RuntimeExecutionContext::new(
            RuntimeOperation::Chat,
            "coding",
            Some("session-1".to_owned()),
            vec![
                ContextMessage {
                    role: "user".to_owned(),
                    content: "previous question".to_owned(),
                },
                ContextMessage {
                    role: "assistant".to_owned(),
                    content: "previous answer".to_owned(),
                },
            ],
        );

        let response = agent
            .respond_with_context(&context, "new prompt", &token())
            .expect("response should succeed");
        assert_eq!(response.text, "ok");

        let rendered = prompts
            .lock()
            .expect("prompt recorder lock should not fail")
            .first()
            .expect("rendered prompt should exist")
            .clone();

        assert!(rendered.contains("coding-focused terminal assistant"));
        assert!(rendered.contains("Mode: interactive chat"));
        assert!(rendered.contains("Session:\nsession-1"));
        assert!(rendered.contains("- user: previous question"));
        assert!(rendered.contains("User request:\nnew prompt"));
    }

    #[test]
    fn run_goal_with_context_adds_execution_hint() {
        let (provider, prompts) = RecordingProvider::new("done", 0);
        let agent = RuntimeAgent::new(Box::new(provider));

        let context = RuntimeExecutionContext::new(RuntimeOperation::Run, "default", None, vec![]);
        let _ = agent
            .run_goal_with_context(&context, "ship phase 2", &token())
            .expect("run goal should succeed");

        let rendered = prompts
            .lock()
            .expect("prompt recorder lock should not fail")
            .first()
            .expect("rendered prompt should exist")
            .clone();

        assert!(rendered.contains("Mode: goal execution planning"));
        assert!(rendered.contains("Execution output format"));
        assert!(rendered.contains("User request:\nship phase 2"));
    }

    #[test]
    fn cancellation_token_interrupts_long_running_request() {
        let (provider, _prompts) = RecordingProvider::new("late", 350);
        let agent = RuntimeAgent::new(Box::new(provider));

        let token = token();
        let cancel_handle = token.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(40));
            cancel_handle.request_cancel();
        });

        let context = RuntimeExecutionContext::new(RuntimeOperation::Ask, "default", None, vec![]);
        let err = agent
            .respond_with_context(&context, "interrupt me", &token)
            .expect_err("request should be interrupted");

        assert!(err.to_string().contains("interrupted"));
    }
}
