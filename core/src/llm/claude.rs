//! Claude (Anthropic Messages API) provider.

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::llm::{LlmProvider, TransformRequest, TransformResponse};
use crate::Result;

pub struct ClaudeProvider {
    client: Client,
    api_key: String,
    model: String,
    temperature: f32,
}

impl ClaudeProvider {
    pub fn new(api_key: String, model: String, temperature: f32) -> Self {
        Self {
            client: Client::new(),
            api_key,
            model,
            temperature,
        }
    }
}

#[derive(Serialize)]
struct MessagesRequest {
    model: String,
    max_tokens: u32,
    temperature: f32,
    system: String,
    messages: Vec<Message>,
}

#[derive(Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct MessagesResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    text: Option<String>,
}

#[async_trait]
impl LlmProvider for ClaudeProvider {
    async fn transform(&self, req: TransformRequest) -> Result<TransformResponse> {
        let body = MessagesRequest {
            model: self.model.clone(),
            max_tokens: 4096,
            temperature: self.temperature,
            system: req.system,
            messages: vec![Message {
                role: "user".into(),
                content: req.user,
            }],
        };

        let resp = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| crate::MuleForgeError::Llm(format!("Claude API request failed: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_else(|_| "unknown error".into());
            return Err(crate::MuleForgeError::Llm(format!(
                "Claude API error {}: {}",
                status, text
            )));
        }

        let parsed: MessagesResponse = resp
            .json()
            .await
            .map_err(|e| crate::MuleForgeError::Llm(format!("Claude API parse error: {}", e)))?;

        let output = parsed
            .content
            .into_iter()
            .filter_map(|b| b.text)
            .collect::<Vec<_>>()
            .join("\n");

        Ok(TransformResponse {
            output,
            confidence: 0.85,
            rationale: Some("Converted via Claude API".into()),
        })
    }
}
