//! Ollama (local) provider — for air-gapped / offline environments.

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::llm::{LlmProvider, TransformRequest, TransformResponse};
use crate::Result;

pub struct OllamaProvider {
    client: Client,
    host: String,
    model: String,
    temperature: f32,
}

impl OllamaProvider {
    pub fn new(host: String, model: String, temperature: f32) -> Self {
        Self {
            client: Client::new(),
            host,
            model,
            temperature,
        }
    }
}

#[derive(Serialize)]
struct GenerateRequest {
    model: String,
    system: String,
    prompt: String,
    stream: bool,
    options: OllamaOptions,
}

#[derive(Serialize)]
struct OllamaOptions {
    temperature: f32,
    num_predict: u32,
}

#[derive(Deserialize)]
struct GenerateResponse {
    response: String,
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    async fn transform(&self, req: TransformRequest) -> Result<TransformResponse> {
        let body = GenerateRequest {
            model: self.model.clone(),
            system: req.system,
            prompt: req.user,
            stream: false,
            options: OllamaOptions {
                temperature: self.temperature,
                num_predict: 4096,
            },
        };

        let url = format!("{}/api/generate", self.host);
        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                crate::MuleForgeError::Llm(format!(
                    "Ollama request failed (is Ollama running at {}?): {}",
                    self.host, e
                ))
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_else(|_| "unknown error".into());
            return Err(crate::MuleForgeError::Llm(format!(
                "Ollama error {}: {}",
                status, text
            )));
        }

        let parsed: GenerateResponse = resp
            .json()
            .await
            .map_err(|e| crate::MuleForgeError::Llm(format!("Ollama parse error: {}", e)))?;

        Ok(TransformResponse {
            output: parsed.response,
            confidence: 0.70,
            rationale: Some(format!("Converted via Ollama ({})", self.model)),
        })
    }
}
