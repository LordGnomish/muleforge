//! OpenAI Chat Completions provider (also used for Azure OpenAI).

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::llm::{LlmProvider, TransformRequest, TransformResponse};
use crate::Result;

pub struct OpenAiProvider {
    client: Client,
    api_key: String,
    base_url: String,
    model: String,
    temperature: f32,
}

impl OpenAiProvider {
    pub fn new(api_key: String, base_url: String, model: String, temperature: f32) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url,
            model,
            temperature,
        }
    }
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    temperature: f32,
    max_tokens: u32,
    messages: Vec<ChatMessage>,
}

#[derive(Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChatMessage,
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    async fn transform(&self, req: TransformRequest) -> Result<TransformResponse> {
        let body = ChatRequest {
            model: self.model.clone(),
            temperature: self.temperature,
            max_tokens: 4096,
            messages: vec![
                ChatMessage {
                    role: "system".into(),
                    content: req.system,
                },
                ChatMessage {
                    role: "user".into(),
                    content: req.user,
                },
            ],
        };

        let url = format!("{}/chat/completions", self.base_url);
        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| crate::MuleForgeError::Llm(format!("OpenAI API request failed: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_else(|_| "unknown error".into());
            return Err(crate::MuleForgeError::Llm(format!(
                "OpenAI API error {}: {}",
                status, text
            )));
        }

        let parsed: ChatResponse = resp
            .json()
            .await
            .map_err(|e| crate::MuleForgeError::Llm(format!("OpenAI API parse error: {}", e)))?;

        let output = parsed
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();

        Ok(TransformResponse {
            output,
            confidence: 0.80,
            rationale: Some("Converted via OpenAI API".into()),
        })
    }
}
