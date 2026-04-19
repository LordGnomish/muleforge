//! LLM provider abstraction. Used by the mapper (for complex DataWeave)
//! and by docgen (for prose sections).
//!
//! Default provider: Claude (Anthropic). Additional providers: OpenAI,
//! Gemini, Azure OpenAI, Ollama (local).

pub mod claude;
pub mod ollama;
pub mod openai;

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

pub async fn build_provider(cfg: &LlmConfig) -> Result<Box<dyn LlmProvider>> {
    let api_key = cfg
        .api_key_env
        .as_ref()
        .and_then(|env_var| std::env::var(env_var).ok());

    match cfg.provider {
        LlmProviderKind::Claude => {
            let key = api_key.ok_or_else(|| {
                crate::MuleForgeError::Llm(
                    "ANTHROPIC_API_KEY not set (configure llm.api_key_env)".into(),
                )
            })?;
            Ok(Box::new(claude::ClaudeProvider::new(
                key,
                cfg.model.clone(),
                cfg.temperature,
            )))
        }
        LlmProviderKind::OpenAi => {
            let key = api_key.ok_or_else(|| {
                crate::MuleForgeError::Llm(
                    "OPENAI_API_KEY not set (configure llm.api_key_env)".into(),
                )
            })?;
            let base_url = cfg
                .host
                .clone()
                .unwrap_or_else(|| "https://api.openai.com/v1".into());
            Ok(Box::new(openai::OpenAiProvider::new(
                key,
                base_url,
                cfg.model.clone(),
                cfg.temperature,
            )))
        }
        LlmProviderKind::Azure => {
            let key = api_key.ok_or_else(|| {
                crate::MuleForgeError::Llm(
                    "AZURE_OPENAI_API_KEY not set (configure llm.api_key_env)".into(),
                )
            })?;
            let base_url = cfg.host.clone().ok_or_else(|| {
                crate::MuleForgeError::Llm(
                    "Azure OpenAI requires llm.host (e.g. https://myinstance.openai.azure.com)"
                        .into(),
                )
            })?;
            Ok(Box::new(openai::OpenAiProvider::new(
                key,
                base_url,
                cfg.model.clone(),
                cfg.temperature,
            )))
        }
        LlmProviderKind::Ollama => {
            let host = cfg
                .host
                .clone()
                .unwrap_or_else(|| "http://localhost:11434".into());
            Ok(Box::new(ollama::OllamaProvider::new(
                host,
                cfg.model.clone(),
                cfg.temperature,
            )))
        }
        LlmProviderKind::Gemini => Err(crate::MuleForgeError::Llm(
            "Gemini provider not yet implemented — use Claude, OpenAI, or Ollama".into(),
        )),
    }
}

// --- DataWeave prompt helpers ---

/// Build a system prompt for DataWeave → Java conversion.
pub fn dataweave_system_prompt() -> String {
    r#"You are an expert at converting MuleSoft DataWeave expressions to equivalent Java code for Apache Camel Quarkus.

Rules:
1. Output a single Java class that implements org.apache.camel.Processor or a plain POJO bean.
2. The class must be in package "generated.beans".
3. Use standard Java libraries (java.util, java.time, com.fasterxml.jackson) — no DataWeave runtime.
4. Preserve the transformation semantics exactly.
5. Add @ApplicationScoped annotation for CDI injection.
6. Include a brief Javadoc explaining what the original DataWeave did.
7. Output ONLY the Java source code, no markdown fences, no explanation.
8. If the DataWeave is too complex to convert reliably, output a TODO comment explaining what needs manual attention."#.into()
}

/// Build a user prompt for a specific DataWeave expression.
pub fn dataweave_user_prompt(dw_expression: &str, context: &str) -> String {
    format!(
        "Convert this DataWeave expression to a Java bean:\n\n```dataweave\n{}\n```\n\nContext: {}",
        dw_expression, context
    )
}

/// Build a system prompt for docgen prose.
pub fn docgen_system_prompt(style: &str) -> String {
    format!(
        r#"You are a technical writer generating documentation for a Camel Quarkus project that was migrated from MuleSoft.

Style: {}
Rules:
1. Write clear, actionable documentation.
2. Explain what each route does and why the original Mule flow was designed that way.
3. Include code examples where helpful.
4. Output Markdown.
5. Do not include meta-commentary about the documentation itself."#,
        style
    )
}
