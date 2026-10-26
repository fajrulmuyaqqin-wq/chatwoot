pub mod chunker;
pub mod scraper;


use crate::services::llm::LlmProvider;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use tracing::{error, info};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeMatch {
    pub id: i64,
    pub question: String,
    pub answer: String,
    pub score: f64,
}

/// Ingests document content: chunks it, computes embeddings, and stores in captain_assistant_responses
pub async fn ingest_document_chunks(
    pool: &PgPool,
    account_id: i64,
    assistant_id: i64,
    doc_id: i64,
    doc_name: &str,
    content: &str,
    llm_provider: Arc<dyn LlmProvider>,
) -> Result<usize, String> {
    let chunks = chunker::chunk_text(content, 800, 100);
    if chunks.is_empty() {
        return Ok(0);
    }

    info!(
        "Ingesting doc {} ('{}') into {} chunks for assistant {}",
        doc_id, doc_name, chunks.len(), assistant_id
    );

    // Remove existing chunks for this document if re-syncing
    let _ = sqlx::query(
        "DELETE FROM captain_assistant_responses WHERE document_id = $1 AND account_id = $2"
    )
    .bind(doc_id)
    .bind(account_id)
    .execute(pool)
    .await;

    let mut inserted_count = 0;

    for (idx, chunk) in chunks.iter().enumerate() {
        let question_label = if chunks.len() == 1 {
            doc_name.to_string()
        } else {
            format!("{} (Bagian {})", doc_name, idx + 1)
        };

        let embedding_res = llm_provider.generate_embedding(chunk).await;
        let embedding_str = match embedding_res {
            Ok(vec) => {
                let numbers: Vec<String> = vec.iter().map(|v| v.to_string()).collect();
                format!("[{}]", numbers.join(","))
            }
            Err(e) => {
                error!("Failed to generate embedding for chunk {}: {}", idx, e);
                continue;
            }
        };

        let insert_res = sqlx::query(
            r#"
            INSERT INTO captain_assistant_responses
                (question, answer, embedding, status, edited, assistant_id, account_id, document_id, documentable_id, documentable_type, created_at, updated_at)
            VALUES
                ($1, $2, $3::vector, 1, false, $4, $5, $6, $6, 'Captain::Document', NOW(), NOW())
            "#
        )
        .bind(&question_label)
        .bind(chunk)
        .bind(&embedding_str)
        .bind(assistant_id)
        .bind(account_id)
        .bind(doc_id)
        .execute(pool)
        .await;

        if let Ok(_) = insert_res {
            inserted_count += 1;
        }
    }

    // Mark document as available and synced
    let _ = sqlx::query(
        r#"
        UPDATE captain_documents
        SET status = 1, sync_status = 1, last_synced_at = NOW(), updated_at = NOW()
        WHERE id = $1
        "#
    )
    .bind(doc_id)
    .execute(pool)
    .await;

    info!(
        "Successfully inserted {}/{} chunks for document {}",
        inserted_count, chunks.len(), doc_id
    );

    Ok(inserted_count)
}

/// Searches the vector database for knowledge entries semantically closest to the query
pub async fn search_knowledge(
    pool: &PgPool,
    account_id: i64,
    assistant_id: Option<i64>,
    query: &str,
    limit: i64,
    llm_provider: Arc<dyn LlmProvider>,
) -> Result<Vec<KnowledgeMatch>, String> {
    let embedding_res = llm_provider.generate_embedding(query).await;
    let embedding_vec = match embedding_res {
        Ok(v) => v,
        Err(e) => return Err(format!("Failed to generate query embedding: {e}")),
    };

    let numbers: Vec<String> = embedding_vec.iter().map(|v| v.to_string()).collect();
    let embedding_str = format!("[{}]", numbers.join(","));

    let rows = if let Some(asst_id) = assistant_id {
        sqlx::query(
            r#"
            SELECT id, question, answer, (1 - (embedding <=> $1::vector)) AS score
            FROM captain_assistant_responses
            WHERE account_id = $2 AND assistant_id = $3 AND embedding IS NOT NULL
            ORDER BY embedding <=> $1::vector ASC
            LIMIT $4
            "#
        )
        .bind(&embedding_str)
        .bind(account_id)
        .bind(asst_id)
        .bind(limit)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query(
            r#"
            SELECT id, question, answer, (1 - (embedding <=> $1::vector)) AS score
            FROM captain_assistant_responses
            WHERE account_id = $1 AND embedding IS NOT NULL
            ORDER BY embedding <=> $2::vector ASC
            LIMIT $3
            "#
        )
        .bind(account_id)
        .bind(&embedding_str)
        .bind(limit)
        .fetch_all(pool)
        .await
    };

    let rows = rows.map_err(|e| format!("Database query error during vector search: {e}"))?;

    let mut matches = Vec::new();
    for row in rows {
        let id: i64 = row.get("id");
        let question: String = row.get("question");
        let answer: String = row.get("answer");
        let score: f64 = row.get("score");

        matches.push(KnowledgeMatch {
            id,
            question,
            answer,
            score,
        });
    }

    Ok(matches)
}
