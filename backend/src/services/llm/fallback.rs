use super::{ChatMessage, LlmOptions, LlmProvider};
use sha2::{Digest, Sha256};
use std::future::Future;
use std::pin::Pin;
use tracing::info;

pub struct FallbackProvider;

impl FallbackProvider {
    pub fn new() -> Self {
        Self
    }

    /// Generates a deterministic normalized 1536-dimensional embedding using n-gram hashing
    pub fn deterministic_embedding(text: &str) -> Vec<f32> {
        let mut vec = vec![0.0f32; 1536];
        let words: Vec<&str> = text.split_whitespace().collect();

        if words.is_empty() {
            vec[0] = 1.0;
            return vec;
        }

        for word in &words {
            let mut hasher = Sha256::new();
            hasher.update(word.to_lowercase().as_bytes());
            let hash = hasher.finalize();

            for chunk_idx in 0..8 {
                let offset = (chunk_idx * 4) as usize;
                let bucket = u32::from_le_bytes([
                    hash[offset],
                    hash[offset + 1],
                    hash[offset + 2],
                    hash[offset + 3],
                ]) as usize % 1536;

                let sign = if (hash[offset] % 2) == 0 { 1.0 } else { -1.0 };
                vec[bucket] += sign;
            }
        }

        // Normalize vector to L2 norm = 1.0
        let norm_sq: f32 = vec.iter().map(|v| v * v).sum();
        let norm = norm_sq.sqrt();
        if norm > 0.0 {
            for v in &mut vec {
                *v /= norm;
            }
        }

        vec
    }
}

impl Default for FallbackProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl LlmProvider for FallbackProvider {
    fn chat_completion<'a>(
        &'a self,
        messages: &'a [ChatMessage],
        _options: &'a LlmOptions,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
        Box::pin(async move {
            info!("Executing chat completion via local smart fallback");
            let last_user_msg = messages
                .iter()
                .rev()
                .find(|m| m.role == "user")
                .map(|m| m.content.as_str())
                .unwrap_or("Halo!");

            let is_summary = last_user_msg.to_lowercase().contains("ringkas")
                || last_user_msg.to_lowercase().contains("summarize");

            if is_summary {
                return Ok(
                    "Ringkasan Percakapan:\n1. Pelanggan menanyakan informasi terkait produk/layanan.\n2. Agen memberikan penjelasan dan langkah panduan penyelesaian.\n3. Masalah dalam pemantauan tim dukungan.".to_string(),
                );
            }

            Ok(format!(
                "Halo! Terima kasih telah menghubungi kami. Mengenai pertanyaan Anda tentang '{}', kami siap membantu Anda dengan solusi terbaik.",
                last_user_msg.trim()
            ))
        })
    }

    fn generate_embedding<'a>(
        &'a self,
        text: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<f32>, String>> + Send + 'a>> {
        Box::pin(async move {
            info!("Generating deterministic embedding for text of len {}", text.len());
            Ok(Self::deterministic_embedding(text))
        })
    }
}
