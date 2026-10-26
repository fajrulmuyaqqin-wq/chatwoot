use std::sync::Arc;
use sqlx::PgPool;
use tracing::{error, info};

use crate::models::article::Article;
use crate::services::llm::{ChatMessage, LlmOptions, LlmProvider};

/// Mencari artikel relevan di knowledge base
pub async fn search_knowledge_base(
    pool: &PgPool,
    account_id: i32,
    query: &str,
) -> Result<Vec<Article>, sqlx::Error> {
    let pattern = format!("%{}%", query.trim());
    sqlx::query_as::<_, Article>(
        r#"
        SELECT * FROM articles
        WHERE account_id = $1 AND status = 1
          AND (title ILIKE $2 OR content ILIKE $2 OR description ILIKE $2)
        ORDER BY views DESC, id DESC
        LIMIT 3
        "#,
    )
    .bind(account_id)
    .bind(&pattern)
    .fetch_all(pool)
    .await
}

/// Menghasilkan saran balasan cerdas (Smart Reply Suggestion) untuk agen melalui Unified LLM Gateway
pub async fn generate_reply_suggestion(
    pool: &PgPool,
    account_id: i32,
    conversation_context: &str,
    llm_provider: Arc<dyn LlmProvider>,
) -> Result<String, String> {
    // 1. Ekstrak kata kunci pencarian
    let keywords = conversation_context
        .split_whitespace()
        .filter(|w| w.len() > 3)
        .take(3)
        .collect::<Vec<&str>>()
        .join(" ");

    let search_term = if keywords.is_empty() {
        conversation_context
    } else {
        &keywords
    };

    // 2. Ambil artikel pengetahuan yang relevan
    let articles = search_knowledge_base(pool, account_id, search_term)
        .await
        .unwrap_or_default();

    let context_knowledge = if !articles.is_empty() {
        articles
            .iter()
            .map(|a| {
                format!(
                    "Judul: {}\nRingkasan: {}\nKonten: {}",
                    a.title,
                    a.description.as_deref().unwrap_or(""),
                    a.content.as_deref().unwrap_or("")
                )
            })
            .collect::<Vec<String>>()
            .join("\n---\n")
    } else {
        "Tidak ada artikel khusus.".to_string()
    };

    // 3. Panggil LLM Provider (Boson Gateway)
    let system_prompt = format!(
        "Anda adalah asisten AI customer support profesional. Berikan saran balasan ramah, singkat, dan tepat berdasarkan artikel berikut:\n\n{}\n",
        context_knowledge
    );

    let messages = vec![
        ChatMessage::system(system_prompt),
        ChatMessage::user(conversation_context),
    ];

    let options = LlmOptions {
        model: Some("deepseek-chat".to_string()),
        temperature: Some(0.3),
        max_tokens: Some(512),
    };

    match llm_provider.chat_completion(&messages, &options).await {
        Ok(reply) if !reply.trim().is_empty() => {
            info!("Generated AI reply suggestion via Unified LLM Gateway (len: {})", reply.len());
            return Ok(reply.trim().to_string());
        }
        Err(e) => {
            error!("Failed to generate reply suggestion via gateway: {}, falling back to local heuristic", e);
        }
        _ => {}
    }

    // 4. Fallback / Mock Suggestion berbasis knowledge base
    if let Some(top_article) = articles.first() {
        let suggestion = format!(
            "Berdasarkan panduan kami mengenai '{}': {}",
            top_article.title,
            top_article
                .description
                .as_deref()
                .or(top_article.content.as_deref())
                .unwrap_or("Silakan ikuti petunjuk yang tertera di artikel bantuan kami.")
        );
        Ok(suggestion)
    } else {
        Ok(format!(
            "Terima kasih atas pesan Anda mengenai '{}'. Tim dukungan kami sedang mengecek detailnya dan akan segera membantu Anda.",
            conversation_context
        ))
    }
}
