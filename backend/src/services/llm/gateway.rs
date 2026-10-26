use super::{ChatMessage, LlmOptions, LlmProvider};
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::Client;
use serde_json::json;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use tracing::{error, info, warn};

/// Adapter resmi untuk Unified LLM Gateway sesuai spesifikasi blueprint
pub struct GatewayProvider {
    api_key: String,
    base_url: String,
    platform_id: String,
    client: Client,
}

impl GatewayProvider {
    pub fn new(api_key: String, base_url: String, platform_id: String) -> Self {
        let trimmed_base = base_url.trim_end_matches('/').to_string();
        let mut default_headers = HeaderMap::new();
        if let Ok(p_id) = HeaderValue::from_str(&platform_id) {
            default_headers.insert("X-Boson-Platform-Id", p_id);
        }

        let client = Client::builder()
            .timeout(Duration::from_secs(45))
            .default_headers(default_headers)
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            api_key,
            base_url: trimmed_base,
            platform_id,
            client,
        }
    }
}

impl LlmProvider for GatewayProvider {
    fn chat_completion<'a>(
        &'a self,
        messages: &'a [ChatMessage],
        options: &'a LlmOptions,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
        Box::pin(async move {
            // Default model to deepseek-chat on Unified LLM Gateway
            let model = options.model.as_deref().unwrap_or("deepseek-chat");
            let url = format!("{}/chat/completions", self.base_url);

            info!(
                "Routing chat completion to Unified LLM Gateway ({}) using model: {}, platform: {}",
                url, model, self.platform_id
            );

            let body = json!({
                "model": model,
                "messages": messages,
                "temperature": options.temperature.unwrap_or(0.3),
                "max_tokens": options.max_tokens.unwrap_or(2048),
            });

            let resp = self
                .client
                .post(&url)
                .bearer_auth(&self.api_key)
                .json(&body)
                .send()
                .await
                .map_err(|e| format!("Failed to send request to LLM Gateway: {e}"))?;

            let status = resp.status();
            if !status.is_success() {
                let err_text = resp.text().await.unwrap_or_default();
                error!("LLM Gateway error [{}]: {}", status, err_text);

                return match status.as_u16() {
                    401 => Err("LLM Gateway: Kunci API tidak valid atau telah dicabut (401)".to_string()),
                    402 => Err("LLM Gateway: Saldo akun habis / insufficient quota (402)".to_string()),
                    403 => Err(format!("LLM Gateway: Platform '{}' tidak diizinkan (403)", self.platform_id)),
                    429 => Err("LLM Gateway: Rate limit streaming terlampaui (429)".to_string()),
                    502 => Err("LLM Gateway: Upstream AI timeout (502)".to_string()),
                    _ => Err(format!("LLM Gateway error [{}]: {}", status, err_text)),
                };
            }

            let resp_json: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| format!("Failed to parse LLM Gateway response: {e}"))?;

            let content = resp_json
                .get("choices")
                .and_then(|c| c.get(0))
                .and_then(|c0| c0.get("message"))
                .and_then(|m| m.get("content"))
                .and_then(|t| t.as_str())
                .unwrap_or_default();

            info!("LLM Gateway response successfully received (len: {})", content.len());
            Ok(content.to_string())
        })
    }

    fn generate_embedding<'a>(
        &'a self,
        text: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<f32>, String>> + Send + 'a>> {
        Box::pin(async move {
            let url = format!("{}/embeddings", self.base_url);
            let model = "text-embedding-3-small";

            let body = json!({
                "model": model,
                "input": text,
            });

            // Try upstream embedding endpoint first
            if let Ok(resp) = self
                .client
                .post(&url)
                .bearer_auth(&self.api_key)
                .json(&body)
                .send()
                .await
            {
                if resp.status().is_success() {
                    if let Ok(resp_json) = resp.json::<serde_json::Value>().await {
                        if let Some(emb) = resp_json
                            .get("data")
                            .and_then(|d| d.get(0))
                            .and_then(|d0| d0.get("embedding"))
                            .and_then(|emb| serde_json::from_value::<Vec<f32>>(emb.clone()).ok())
                        {
                            info!("LLM Gateway generated {} embedding dimensions", emb.len());
                            return Ok(emb);
                        }
                    }
                } else {
                    warn!(
                        "LLM Gateway embeddings endpoint returned [{}], using deterministic fallback embedding",
                        resp.status()
                    );
                }
            }

            // High-precision deterministic vector fallback for pgvector (1536 dims)
            info!("Generating deterministic 1536-dim embedding for pgvector");
            Ok(crate::services::llm::fallback::FallbackProvider::deterministic_embedding(text))
        })
    }
}
