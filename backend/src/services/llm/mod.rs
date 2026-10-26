pub mod fallback;
pub mod gateway;

use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct LlmOptions {
    pub model: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
}

pub trait LlmProvider: Send + Sync {
    fn chat_completion<'a>(
        &'a self,
        messages: &'a [ChatMessage],
        options: &'a LlmOptions,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>>;

    fn generate_embedding<'a>(
        &'a self,
        text: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<f32>, String>> + Send + 'a>>;
}

/// Resolves an LLM provider based on settings or falls back gracefully.
/// Seluruh inferensi AI dialirkan melalui Unified LLM Gateway sesuai standar blueprint.
pub fn get_llm_provider(
    _provider_name: Option<&str>,
    api_key_override: Option<&str>,
    base_url_override: Option<&str>,
    config: &crate::config::Config,
) -> Arc<dyn LlmProvider> {
    if !config.llm_gateway_api_key.is_empty() {
        let key = api_key_override
            .map(|s| s.to_string())
            .unwrap_or_else(|| config.llm_gateway_api_key.clone());
        let base = base_url_override
            .map(|s| s.to_string())
            .unwrap_or_else(|| config.llm_gateway_url.clone());
        let platform = config.llm_gateway_platform_id.clone();

        return Arc::new(gateway::GatewayProvider::new(key, base, platform));
    }

    // Fallback lokal deterministik jika gateway belum terkonfigurasi
    Arc::new(fallback::FallbackProvider::new())
}
