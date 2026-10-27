//! LLM provider abstraction. Used by the mapper (for complex DataWeave)
//! and by docgen (for prose sections).
//!
//! Default provider: Claude (Anthropic). Additional providers: OpenAI,
//! Gemini, Azure OpenAI, Ollama (local).

use async_trait::async_trait;

use crate::Result;

#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub provider: LlmProviderKind,
    pub model: String,
    pub api_key_env: Option<String>,
    pub host: Option<String>,
    pub temperature: f32,
}

#[derive(Debug, Clone, Copy)]
pub enum LlmProviderKind {
    Claude,
    OpenAi,
    Gemini,
    Azure,
    Ollama,
}

pub struct TransformRequest {
    pub task: TaskKind,
    pub system: String,
    pub user: String,
}

pub enum TaskKind {
    /// Convert a DataWeave expression to a Camel-compatible artifact.
    DataWeaveToCamel,
    /// Fill a documentation section from structured context.
    DocgenProse { section: String },
}

pub struct TransformResponse {
    pub output: String,
    pub confidence: f32,
    pub rationale: Option<String>,
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn transform(&self, req: TransformRequest) -> Result<TransformResponse>;
}

pub async fn build_provider(_cfg: &LlmConfig) -> Result<Box<dyn LlmProvider>> {
    // TODO: dispatch on cfg.provider, return the matching impl.
    Err(crate::MuleForgeError::Llm(
        "provider construction not implemented yet".into(),
    ))
}
