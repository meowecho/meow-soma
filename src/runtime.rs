use anyhow::Result;

use crate::providers::LlmProvider;

pub struct RuntimeAgent {
    provider: Box<dyn LlmProvider>,
}

impl RuntimeAgent {
    pub fn new(provider: Box<dyn LlmProvider>) -> Self {
        Self { provider }
    }

    pub fn provider_name(&self) -> &str {
        self.provider.name()
    }

    pub fn provider_model(&self) -> &str {
        self.provider.model()
    }

    pub fn respond(&self, prompt: &str) -> Result<String> {
        self.provider.complete(prompt)
    }

    pub fn run_goal(&self, goal: &str) -> Result<String> {
        let execution_prompt = format!(
            "Goal: {goal}\n\nReturn a compact execution summary with next steps and risks."
        );
        self.provider.complete(&execution_prompt)
    }
}
