use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::Row;
use tracing::{error, info};

use crate::models::conversation::Conversation;
use crate::services::llm::{get_llm_provider, ChatMessage, LlmOptions};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct RewritePayload {
    pub content: Option<String>,
    pub operation: Option<String>,
    pub conversation_display_id: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct SummarizePayload {
    pub conversation_display_id: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct ReplySuggestionPayload {
    pub conversation_display_id: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct FollowUpPayload {
    pub message: Option<String>,
    pub follow_up_context: Option<serde_json::Value>,
}

/// Helper: Ambil konteks riwayat percakapan terakhir
async fn get_conversation_context(
    state: &AppState,
    account_id: i32,
    display_id: i32,
) -> Option<(Conversation, String)> {
    let conv = sqlx::query_as::<_, Conversation>(
        "SELECT * FROM conversations WHERE account_id = $1 AND display_id = $2",
    )
    .bind(account_id)
    .bind(display_id)
    .fetch_optional(&state.db)
    .await
    .ok()??;

    let messages = sqlx::query(
        r#"
        SELECT content, message_type
        FROM messages
        WHERE conversation_id = $1 AND content IS NOT NULL AND content != ''
        ORDER BY created_at DESC
        LIMIT 10
        "#,
    )
    .bind(conv.id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let mut formatted = Vec::new();
    for m in messages.iter().rev() {
        let content: String = m.get("content");
        let m_type: i32 = m.get("message_type");
        let sender = if m_type == 0 { "Pelanggan" } else { "Agen" };
        formatted.push(format!("{}: {}", sender, content));
    }

    Some((conv, formatted.join("\n")))
}

/// POST /api/v1/accounts/:account_id/captain/tasks/rewrite
pub async fn rewrite_content(
    Path(account_id): Path<i32>,
    State(state): State<AppState>,
    Json(payload): Json<RewritePayload>,
) -> impl IntoResponse {
    let raw_content = payload.content.clone().unwrap_or_default();
    if raw_content.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Content cannot be empty" })),
        );
    }

    let operation = payload.operation.unwrap_or_else(|| "improve".to_string());
    let mut conv_context = String::new();

    if let Some(display_id) = payload.conversation_display_id {
        if let Some((_, ctx)) = get_conversation_context(&state, account_id, display_id).await {
            conv_context = ctx;
        }
    }

    let instruction = match operation.as_str() {
        "fix_spelling_grammar" => "Perbaiki kesalahan tata bahasa, ejaan, tanda baca, dan typo dalam teks berikut tanpa mengubah maknanya.",
        "casual" => "Ubah gaya bahasa teks berikut menjadi lebih santai, ramah, bersahabat, namun tetap sopan.",
        "professional" => "Ubah gaya bahasa teks berikut menjadi formal, profesional, elegan, dan percaya diri.",
        "shorten" => "Persingkat teks berikut menjadi padat, ringkas, to-the-point, dan mudah dipahami dalam 1-2 kalimat.",
        "expand" => "Kembangkan teks berikut dengan penjelasan yang lebih detail, jelas, dan membantu pelanggan.",
        _ => "Tingkatkan kualitas tulisan teks berikut agar lebih jelas, komunikatif, dan solutif.",
    };

    let system_prompt = format!(
        "Anda adalah asisten AI editor untuk agen customer support.\n\
        Instruksi Tugas: {}\n\n\
        Konteks Percakapan Tiket (jika relevan):\n{}\n\n\
        Hanya kembalikan hasil teks yang sudah direvisi. Jangan tambahkan tanda petik pengapit atau kalimat pengantar.",
        instruction,
        if conv_context.is_empty() { "Tidak ada konteks percakapan." } else { &conv_context }
    );

    let messages = vec![
        ChatMessage::system(system_prompt),
        ChatMessage::user(raw_content.clone()),
    ];

    let llm_provider = get_llm_provider(None, None, None, &state.config);
    let options = LlmOptions {
        model: Some("gpt-4o-mini".to_string()),
        temperature: Some(0.3),
        max_tokens: Some(1024),
    };

    let generated = match llm_provider.chat_completion(&messages, &options).await {
        Ok(t) => t.trim().to_string(),
        Err(e) => {
            error!("Captain rewrite error: {}", e);
            raw_content
        }
    };

    (
        StatusCode::OK,
        Json(json!({
            "message": generated,
            "follow_up_context": {
                "operation": operation,
                "original_content": payload.content,
                "conversation_display_id": payload.conversation_display_id
            }
        })),
    )
}

/// POST /api/v1/accounts/:account_id/captain/tasks/summarize
pub async fn summarize_conversation(
    Path(account_id): Path<i32>,
    State(state): State<AppState>,
    Json(payload): Json<SummarizePayload>,
) -> impl IntoResponse {
    let display_id = match payload.conversation_display_id {
        Some(d) => d,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "conversation_display_id is required" })),
            );
        }
    };

    let (_, conv_context) = match get_conversation_context(&state, account_id, display_id).await {
        Some(c) => c,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Conversation not found or has no messages" })),
            );
        }
    };

    let system_prompt = "Anda adalah AI summarizer tiket dukungan pelanggan Chatwoot.\n\
        Tugas Anda adalah membuat ringkasan singkat dalam Bahasa Indonesia yang mencakup:\n\
        1. Masalah utama yang dialami pelanggan.\n\
        2. Status penanganan atau solusi yang sudah diberikan agen.\n\
        3. Langkah selanjutnya yang perlu dilakukan (action item).\n\
        Buat seringkas mungkin dalam 2-3 poin jelas.";

    let messages = vec![
        ChatMessage::system(system_prompt.to_string()),
        ChatMessage::user(format!("Berikut riwayat percakapan:\n\n{}", conv_context)),
    ];

    let llm_provider = get_llm_provider(None, None, None, &state.config);
    let options = LlmOptions {
        model: Some("gpt-4o-mini".to_string()),
        temperature: Some(0.2),
        max_tokens: Some(512),
    };

    let summary = match llm_provider.chat_completion(&messages, &options).await {
        Ok(t) => t.trim().to_string(),
        Err(e) => {
            error!("Captain summarize error: {}", e);
            "Gagal meringkas percakapan saat ini.".to_string()
        }
    };

    (
        StatusCode::OK,
        Json(json!({
            "message": summary,
            "follow_up_context": {
                "task": "summarize",
                "conversation_display_id": display_id
            }
        })),
    )
}

/// POST /api/v1/accounts/:account_id/captain/tasks/reply_suggestion
pub async fn reply_suggestion(
    Path(account_id): Path<i32>,
    State(state): State<AppState>,
    Json(payload): Json<ReplySuggestionPayload>,
) -> impl IntoResponse {
    let display_id = match payload.conversation_display_id {
        Some(d) => d,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "conversation_display_id is required" })),
            );
        }
    };

    let (_, conv_context) = match get_conversation_context(&state, account_id, display_id).await {
        Some(c) => c,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Conversation not found or has no messages" })),
            );
        }
    };

    let system_prompt = "Anda adalah AI Copilot untuk agen dukungan pelanggan Chatwoot.\n\
        Berdasarkan konteks percakapan di bawah ini, buatkan 1 saran respons balasan yang sopan, solutif, empatik, dan siap dikirimkan kepada pelanggan.\n\
        Gunakan Bahasa Indonesia yang ramah dan profesional. Hanya berikan teks balasan tanpa pengantar.";

    let messages = vec![
        ChatMessage::system(system_prompt.to_string()),
        ChatMessage::user(format!("Riwayat percakapan:\n\n{}", conv_context)),
    ];

    let llm_provider = get_llm_provider(None, None, None, &state.config);
    let options = LlmOptions {
        model: Some("gpt-4o-mini".to_string()),
        temperature: Some(0.3),
        max_tokens: Some(512),
    };

    let suggestion = match llm_provider.chat_completion(&messages, &options).await {
        Ok(t) => t.trim().to_string(),
        Err(e) => {
            error!("Captain reply suggestion error: {}", e);
            "Terima kasih atas pesan Anda. Kami sedang menindaklanjuti kendala Anda.".to_string()
        }
    };

    (
        StatusCode::OK,
        Json(json!({
            "message": suggestion,
            "follow_up_context": {
                "task": "reply_suggestion",
                "conversation_display_id": display_id
            }
        })),
    )
}

/// POST /api/v1/accounts/:account_id/captain/tasks/follow_up
pub async fn follow_up(
    Path(_account_id): Path<i32>,
    State(state): State<AppState>,
    Json(payload): Json<FollowUpPayload>,
) -> impl IntoResponse {
    let user_msg = payload.message.unwrap_or_default();
    let prev_ctx = payload
        .follow_up_context
        .map(|c| c.to_string())
        .unwrap_or_default();

    let system_prompt = format!(
        "Anda adalah asisten AI editor Chatwoot.\n\
        Konteks tugas sebelumnya:\n{}\n\n\
        Pengguna ingin melakukan penyesuaian/tindak lanjut berdasarkan instruksi berikut.\n\
        Hanya kembalikan hasil teks yang sudah disesuaikan tanpa teks pengantar.",
        prev_ctx
    );

    let messages = vec![
        ChatMessage::system(system_prompt),
        ChatMessage::user(user_msg),
    ];

    let llm_provider = get_llm_provider(None, None, None, &state.config);
    let options = LlmOptions {
        model: Some("gpt-4o-mini".to_string()),
        temperature: Some(0.3),
        max_tokens: Some(1024),
    };

    let result = match llm_provider.chat_completion(&messages, &options).await {
        Ok(t) => t.trim().to_string(),
        Err(e) => {
            error!("Captain follow-up error: {}", e);
            "Gagal memproses follow-up.".to_string()
        }
    };

    (
        StatusCode::OK,
        Json(json!({
            "message": result,
            "follow_up_context": {
                "task": "follow_up"
            }
        })),
    )
}

/// GET /api/v1/accounts/:account_id/captain/agent_sessions/:id
pub async fn get_agent_session_citation(
    Path((account_id, message_id)): Path<(i32, i64)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let session_row = sqlx::query(
        r#"
        SELECT id, assistant_id, user_id, session_type, subject_type, subject_id,
               result_type, result_id, cited_document_ids, used_faq_ids, scenario_ids,
               llm_model, credits_consumed, created_at
        FROM agent_sessions
        WHERE account_id = $1 AND result_type = 'Message' AND result_id = $2
        ORDER BY id DESC
        LIMIT 1
        "#,
    )
    .bind(account_id as i64)
    .bind(message_id)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    if let Some(row) = session_row {
        let sid: i64 = row.get("id");
        let stype: i32 = row.get("session_type");
        let model: Option<String> = row.get("llm_model");
        let credits: Option<f64> = row.get("credits_consumed");
        let created_at: chrono::NaiveDateTime = row.get("created_at");
        let cited_doc_ids: serde_json::Value = row.get("cited_document_ids");
        let used_faq_ids: serde_json::Value = row.get("used_faq_ids");
        let _scenario_ids: serde_json::Value = row.get("scenario_ids");


        // Ambil dokumen yang dikutip
        let doc_ids: Vec<i64> = cited_doc_ids
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_i64()).collect())
            .unwrap_or_default();

        let documents = if !doc_ids.is_empty() {
            sqlx::query(
                r#"
                SELECT id, name
                FROM documents
                WHERE account_id = $1 AND id = ANY($2)
                "#,
            )
            .bind(account_id as i64)
            .bind(&doc_ids)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default()
        } else {
            vec![]
        };

        let citations: Vec<serde_json::Value> = documents
            .into_iter()
            .map(|d| {
                let did: i64 = d.get("id");
                let dname: String = d.get("name");
                json!({ "id": did, "name": dname })
            })
            .collect();

        (
            StatusCode::OK,
            Json(json!({
                "agent_session": {
                    "id": sid,
                    "session_type": stype,
                    "llm_model": model,
                    "credits_consumed": credits,
                    "created_at": created_at.and_utc().timestamp()
                },
                "citations": citations,
                "used_faqs": used_faq_ids,
                "scenario_titles": {}
            })),
        )
    } else {
        (
            StatusCode::OK,
            Json(json!({
                "agent_session": null,
                "citations": [],
                "used_faqs": [],
                "scenario_titles": {}
            })),
        )
    }
}

/// POST /api/v1/accounts/:account_id/captain/copilot_runs/:id/approve
pub async fn approve_copilot_run(
    Path((account_id, run_id)): Path<(i32, i64)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let _ = sqlx::query(
        r#"
        UPDATE copilot_runs
        SET status = 'approved', updated_at = NOW()
        WHERE account_id = $1 AND id = $2
        "#,
    )
    .bind(account_id as i64)
    .bind(run_id)
    .execute(&state.db)
    .await;

    info!("Copilot run {} approved for account {}", run_id, account_id);
    (
        StatusCode::OK,
        Json(json!({
            "status": "approved",
            "id": run_id
        })),
    )
}

/// POST /api/v1/accounts/:account_id/captain/copilot_runs/:id/reject
pub async fn reject_copilot_run(
    Path((account_id, run_id)): Path<(i32, i64)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let _ = sqlx::query(
        r#"
        UPDATE copilot_runs
        SET status = 'rejected', updated_at = NOW()
        WHERE account_id = $1 AND id = $2
        "#,
    )
    .bind(account_id as i64)
    .bind(run_id)
    .execute(&state.db)
    .await;

    info!("Copilot run {} rejected for account {}", run_id, account_id);
    (
        StatusCode::OK,
        Json(json!({
            "status": "rejected",
            "id": run_id
        })),
    )
}
