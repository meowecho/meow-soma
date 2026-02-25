use anyhow::Result;

use crate::config::MeowConfig;

pub trait LlmProvider: Send + Sync {
    fn name(&self) -> &str;
    fn model(&self) -> &str;
    fn complete(&self, prompt: &str) -> Result<String>;
}

pub struct StubProvider {
    name: String,
    model: String,
}

impl StubProvider {
    pub fn new(name: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            model: model.into(),
        }
    }
}

impl LlmProvider for StubProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn complete(&self, prompt: &str) -> Result<String> {
        Ok(format!("[{}:{}] {}", self.name, self.model, prompt.trim()))
    }
}

pub fn build_provider(config: &MeowConfig) -> Box<dyn LlmProvider> {
    match config.runtime.default_provider.as_str() {
        "openai" => {
            let model = config
                .providers
                .openai
                .as_ref()
                .map(|p| p.model.clone())
                .unwrap_or_else(|| "gpt-4.1".to_owned());
            Box::new(StubProvider::new("openai", model))
        }
        "anthropic" => {
            let model = config
                .providers
                .anthropic
                .as_ref()
                .map(|p| p.model.clone())
                .unwrap_or_else(|| "claude-3-7-sonnet-latest".to_owned());
            Box::new(StubProvider::new("anthropic", model))
        }
        _ => {
            let model = config
                .providers
                .ollama
                .as_ref()
                .map(|p| p.model.clone())
                .unwrap_or_else(|| "llama3.1:8b".to_owned());
            Box::new(StubProvider::new("ollama", model))
        }
    }
}
