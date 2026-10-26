use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::Row;
use tracing::{error, info};


use crate::middleware::auth::CurrentUser;
use crate::middleware::rbac::{verify_account_access, AccountRole};
use crate::models::captain::{CopilotMessage, CopilotThread};
use crate::services::event_dispatcher::dispatch_event;
use crate::services::llm::{get_llm_provider, ChatMessage, LlmOptions};
use crate::services::rag::search_knowledge;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CopilotThreadQuery {
    pub page: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateCopilotThreadPayload {
    pub message: String,
    pub assistant_id: Option<i64>,
    pub conversation_id: Option<i64>,
    pub request_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateCopilotMessagePayload {
    pub message: String,
    pub conversation_id: Option<i64>,
}

fn thread_to_json(t: &CopilotThread, current_user: &CurrentUser) -> serde_json::Value {
    json!({
        "id": t.id,
        "title": t.title,
        "created_at": t.created_at.and_utc().timestamp(),
        "account_id": t.account_id,
        "user": {
            "id": current_user.id,
            "name": current_user.email.split('@').next().unwrap_or("User"),
            "email": current_user.email
        },
        "assistant": {
            "id": 1,
            "name": "Captain Copilot"
        }
    })
}

fn message_to_json(m: &CopilotMessage) -> serde_json::Value {
    json!({
        "id": m.id,
        "message": m.message,
        "message_type": m.message_type,
        "created_at": m.created_at.and_utc().timestamp(),
        "updated_at": m.updated_at.and_utc().timestamp() as f64,
        "copilot_thread": {
            "id": m.copilot_thread_id
        },
        "account_id": m.account_id
    })
}

pub async fn list_threads(
    Path(account_id): Path<i32>,
    Query(query): Query<CopilotThreadQuery>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let threads = sqlx::query_as::<_, CopilotThread>(
        r#"
        SELECT id, title, user_id, account_id, conversation_id, created_at, updated_at
        FROM copilot_threads
        WHERE account_id = $1 AND user_id = $2
        ORDER BY created_at DESC, id DESC
        LIMIT 50
        "#,
    )
    .bind(account_id as i64)
    .bind(current_user.id as i64)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let total_count = threads.len();
    let payload: Vec<serde_json::Value> = threads
        .iter()
        .map(|t| thread_to_json(t, &current_user))
        .collect();

    (
        StatusCode::OK,
        Json(json!({
            "payload": payload,
            "meta": {
                "total_count": total_count,
                "page": query.page.unwrap_or(1)
            }
        })),
    )
}

pub async fn create_thread(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body): Json<CreateCopilotThreadPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let title = if body.message.len() > 60 {
        format!("{}...", &body.message[..57])
    } else {
        body.message.clone()
    };

    let thread = sqlx::query_as::<_, CopilotThread>(
        r#"
        INSERT INTO copilot_threads (title, user_id, account_id, conversation_id, created_at, updated_at)
        VALUES ($1, $2, $3, $4, NOW(), NOW())
        RETURNING id, title, user_id, account_id, conversation_id, created_at, updated_at
        "#,
    )
    .bind(&title)
    .bind(current_user.id as i64)
    .bind(account_id as i64)
    .bind(body.conversation_id)
    .fetch_one(&state.db)
    .await;

    let thread = match thread {
        Ok(t) => t,
        Err(e) => {
            error!("Failed to create copilot thread: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            );
        }
    };

    // Insert user's message
    let user_msg_content = json!({ "content": body.message });
    let _ = sqlx::query(
        r#"
        INSERT INTO copilot_messages (copilot_thread_id, user_id, account_id, message_type, message, created_at, updated_at)
        VALUES ($1, $2, $3, 0, $4, NOW(), NOW())
        "#,
    )
    .bind(thread.id)
    .bind(current_user.id as i64)
    .bind(account_id as i64)
    .bind(&user_msg_content)
    .execute(&state.db)
    .await;

    // Asynchronously generate and dispatch AI response
    let state_clone = state.clone();
    let thread_id = thread.id;
    let user_id = current_user.id as i64;
    let user_prompt = body.message.clone();
    let conversation_id = body.conversation_id;
    let assistant_id = body.assistant_id;

    tokio::spawn(async move {
        generate_and_dispatch_copilot_reply(
            state_clone,
            account_id as i64,
            user_id,
            thread_id,
            user_prompt,
            conversation_id,
            assistant_id,
        )
        .await;
    });

    (StatusCode::OK, Json(thread_to_json(&thread, &current_user)))
}

pub async fn list_messages(
    Path((account_id, thread_id)): Path<(i32, i64)>,
    Query(query): Query<CopilotThreadQuery>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let messages = sqlx::query_as::<_, CopilotMessage>(
        r#"
        SELECT id, copilot_thread_id, user_id, account_id, message_type, message, created_at, updated_at
        FROM copilot_messages
        WHERE account_id = $1 AND copilot_thread_id = $2
        ORDER BY created_at ASC, id ASC
        "#,
    )
    .bind(account_id as i64)
    .bind(thread_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let total_count = messages.len();
    let payload: Vec<serde_json::Value> = messages.iter().map(message_to_json).collect();

    (
        StatusCode::OK,
        Json(json!({
            "payload": payload,
            "meta": {
                "total_count": total_count,
                "page": query.page.unwrap_or(1)
            }
        })),
    )
}

pub async fn create_message(
    Path((account_id, thread_id)): Path<(i32, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body): Json<CreateCopilotMessagePayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let user_msg_content = json!({ "content": body.message });
    let created_msg = sqlx::query_as::<_, CopilotMessage>(
        r#"
        INSERT INTO copilot_messages (copilot_thread_id, user_id, account_id, message_type, message, created_at, updated_at)
        VALUES ($1, $2, $3, 0, $4, NOW(), NOW())
        RETURNING id, copilot_thread_id, user_id, account_id, message_type, message, created_at, updated_at
        "#,
    )
    .bind(thread_id)
    .bind(current_user.id as i64)
    .bind(account_id as i64)
    .bind(&user_msg_content)
    .fetch_one(&state.db)
    .await;

    let created_msg = match created_msg {
        Ok(m) => m,
        Err(e) => {
            error!("Failed to create copilot message: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            );
        }
    };

    // Asynchronously generate and dispatch AI response
    let state_clone = state.clone();
    let user_id = current_user.id as i64;
    let user_prompt = body.message.clone();
    let conversation_id = body.conversation_id;

    tokio::spawn(async move {
        generate_and_dispatch_copilot_reply(
            state_clone,
            account_id as i64,
            user_id,
            thread_id,
            user_prompt,
            conversation_id,
            None,
        )
        .await;
    });

    (StatusCode::OK, Json(message_to_json(&created_msg)))
}

async fn generate_and_dispatch_copilot_reply(
    state: AppState,
    account_id: i64,
    _user_id: i64,
    thread_id: i64,
    user_prompt: String,
    conversation_id: Option<i64>,
    assistant_id: Option<i64>,
) {
    let llm_provider = get_llm_provider(None, None, None, &state.config);

    // 1. Fetch Knowledge from RAG semantic search
    let knowledge_matches = search_knowledge(
        &state.db,
        account_id,
        assistant_id,
        &user_prompt,
        3,
        llm_provider.clone(),
    )
    .await
    .unwrap_or_default();

    let knowledge_context = if !knowledge_matches.is_empty() {
        knowledge_matches
            .iter()
            .map(|km| format!("- [Q: {}] {}", km.question, km.answer))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        "Tidak ada informasi referensi spesifik yang ditemukan di basis pengetahuan.".to_string()
    };

    // 2. Fetch conversation context if conversation_id is provided
    let mut conv_context = String::new();
    if let Some(conv_id) = conversation_id {
        let messages = sqlx::query(
            r#"
            SELECT content, message_type
            FROM messages
            WHERE conversation_id = $1
            ORDER BY created_at DESC
            LIMIT 6
            "#,
        )
        .bind(conv_id)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

        if !messages.is_empty() {
            let mut formatted = Vec::new();
            for m in messages.iter().rev() {
                let content: String = m.get("content");
                let m_type: i32 = m.get("message_type");
                let sender = if m_type == 0 { "Pelanggan" } else { "Agen" };
                formatted.push(format!("{}: {}", sender, content));
            }
            conv_context = formatted.join("\n");
        }
    }

    // 3. Build system prompt
    let system_prompt = format!(
        "Anda adalah AI Copilot untuk agen dukungan pelanggan Chatwoot.\n\
        Tugas Anda adalah membantu agen menjawab pesan pelanggan, meringkas riwayat percakapan, atau menyarankan jawaban profesional.\n\n\
        Konteks Basis Pengetahuan:\n{}\n\n\
        Konteks Percakapan Tiket:\n{}\n\n\
        Berikan jawaban yang jelas, ramah, to-the-point, dan berbahasa Indonesia baku/profesional.",
        knowledge_context,
        if conv_context.is_empty() { "Tidak ada percakapan tiket terkait." } else { &conv_context }
    );

    let chat_messages = vec![
        ChatMessage::system(system_prompt),
        ChatMessage::user(user_prompt),
    ];

    let options = LlmOptions {
        model: Some("gpt-4o-mini".to_string()),
        temperature: Some(0.3),
        max_tokens: Some(1024),
    };

    let reply_content = match llm_provider.chat_completion(&chat_messages, &options).await {
        Ok(text) => text,
        Err(e) => {
            error!("Copilot LLM execution error: {}", e);
            "Maaf, Copilot mengalami kendala saat memproses permintaan Anda. Silakan coba kembali sesaat lagi.".to_string()
        }
    };

    // 4. Save AI message to copilot_messages table
    let assistant_msg_content = json!({ "content": reply_content });
    let saved_msg = sqlx::query_as::<_, CopilotMessage>(
        r#"
        INSERT INTO copilot_messages (copilot_thread_id, user_id, account_id, message_type, message, created_at, updated_at)
        VALUES ($1, NULL, $2, 1, $3, NOW(), NOW())
        RETURNING id, copilot_thread_id, user_id, account_id, message_type, message, created_at, updated_at
        "#,
    )
    .bind(thread_id)
    .bind(account_id)
    .bind(&assistant_msg_content)
    .fetch_one(&state.db)
    .await;

    if let Ok(msg) = saved_msg {
        let msg_json = message_to_json(&msg);
        info!("Dispatching copilot.message.created for thread {}", thread_id);
        dispatch_event(
            &state,
            account_id as crate::state::AccountId,
            "copilot.message.created",
            msg_json,
        );
    }
}
