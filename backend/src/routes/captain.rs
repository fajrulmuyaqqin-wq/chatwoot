use axum::{
    body::to_bytes,
    extract::{Extension, FromRequest, Multipart, Path, Query, Request, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::Row;
use std::time::Instant;
use tracing::{error, info};

use crate::middleware::auth::CurrentUser;
use crate::middleware::rbac::{verify_account_access, AccountRole};
use crate::models::captain::{
    CaptainAssistant, CaptainAssistantResponseRecord, CaptainCustomTool, CaptainDocument,
    CaptainScenario,
};
use crate::models::inbox::{Inbox, InboxResponse};
use crate::services::event_dispatcher::dispatch_event;
use crate::services::llm::{get_llm_provider, ChatMessage, LlmOptions};
use crate::services::rag::{ingest_document_chunks, search_knowledge};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    pub page: Option<i64>,
    pub search_key: Option<String>,
    pub search: Option<String>,
    pub assistant_id: Option<i64>,
    pub document_id: Option<i64>,
}


#[derive(Debug, Deserialize)]
pub struct CreateAssistantPayload {
    pub name: Option<String>,
    pub description: Option<String>,
    pub config: Option<serde_json::Value>,
    pub response_guidelines: Option<serde_json::Value>,
    pub guardrails: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAssistantPayload {
    pub name: Option<String>,
    pub description: Option<String>,
    pub config: Option<serde_json::Value>,
    pub response_guidelines: Option<serde_json::Value>,
    pub guardrails: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct CreateDocumentPayload {
    pub name: Option<String>,
    pub external_link: Option<String>,
    pub content: Option<String>,
    pub assistant_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateAssistantResponsePayload {
    pub question: String,
    pub answer: String,
    pub assistant_id: Option<i64>,
    pub document_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAssistantResponsePayload {
    pub question: Option<String>,
    pub answer: Option<String>,
    pub assistant_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct PlaygroundPayload {
    pub message_content: String,
    pub message_history: Option<Vec<serde_json::Value>>,
    pub request_id: Option<String>,
    pub playground_config: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct CreateCustomToolPayload {
    pub slug: String,
    pub title: String,
    pub description: Option<String>,
    pub http_method: Option<String>,
    pub endpoint_url: String,
    pub request_template: Option<String>,
    pub response_template: Option<String>,
    pub auth_type: Option<String>,
    pub auth_config: Option<serde_json::Value>,
    pub param_schema: Option<serde_json::Value>,
    pub headers: Option<serde_json::Value>,
    pub settings: Option<serde_json::Value>,
    pub enabled: Option<bool>,
    pub assistant_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateScenarioPayload {
    pub assistant_id: i64,
    pub title: Option<String>,
    pub description: Option<String>,
    pub instruction: Option<String>,
    pub tools: Option<serde_json::Value>,
    pub enabled: Option<bool>,
}

fn assistant_to_json(a: CaptainAssistant) -> serde_json::Value {
    json!({
        "id": a.id,
        "name": a.name,
        "account_id": a.account_id,
        "description": a.description,
        "config": a.config,
        "response_guidelines": a.response_guidelines.unwrap_or_else(|| json!([])),
        "guardrails": a.guardrails.unwrap_or_else(|| json!([])),
        "created_at": a.created_at.and_utc().timestamp(),
        "updated_at": a.updated_at.and_utc().timestamp()
    })
}

fn document_to_json(d: CaptainDocument, responses_count: i64) -> serde_json::Value {
    let is_pdf = d
        .external_link
        .as_deref()
        .map(|l| l.starts_with("PDF:") || l.ends_with(".pdf"))
        .unwrap_or(false);

    json!({
        "id": d.id,
        "account_id": d.account_id,
        "assistant_id": d.assistant_id,
        "name": d.name,
        "external_link": d.external_link,
        "display_url": d.external_link,
        "content": d.content,
        "content_type": if is_pdf { "application/pdf" } else { "text/plain" },
        "created_at": d.created_at.and_utc().timestamp(),
        "updated_at": d.updated_at.and_utc().timestamp(),
        "status": if d.status == 1 { "available" } else { "in_progress" },
        "sync_status": if d.sync_status == Some(1) { "synced" } else { "syncing" },
        "syncable": !is_pdf,
        "sync_in_progress": false,
        "pdf_document": is_pdf,
        "markdown_document": false,
        "responses_count": responses_count,
        "file_size": d.content.as_ref().map(|c| c.len()).unwrap_or(0),
        "last_synced_at": d.last_synced_at.map(|t| t.and_utc().timestamp()),
        "last_sync_attempted_at": d.last_sync_attempted_at.map(|t| t.and_utc().timestamp()),
        "assistant": {
            "id": d.assistant_id,
            "name": "AI Assistant"
        }
    })
}


fn assistant_response_to_json(r: CaptainAssistantResponseRecord) -> serde_json::Value {
    json!({
        "id": r.id,
        "account_id": r.account_id,
        "assistant_id": r.assistant_id,
        "document_id": r.document_id,
        "question": r.question,
        "answer": r.answer,
        "status": r.status,
        "edited": r.edited,
        "created_at": r.created_at.and_utc().timestamp(),
        "updated_at": r.updated_at.and_utc().timestamp(),
        "assistant": {
            "id": r.assistant_id,
            "name": "AI Assistant"
        },
        "documentable": {
            "type": r.documentable_type.unwrap_or_else(|| "User".to_string()),
            "id": r.documentable_id.unwrap_or(1)
        }
    })
}

// ==========================================
// Assistants Endpoints
// ==========================================

pub async fn list_assistants(
    Path(account_id): Path<i32>,
    Query(query): Query<PaginationQuery>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let assistants = sqlx::query_as::<_, CaptainAssistant>(
        r#"
        SELECT id, name, account_id, description, config, response_guidelines, guardrails, created_at, updated_at
        FROM captain_assistants
        WHERE account_id = $1
        ORDER BY id ASC
        "#,
    )
    .bind(account_id as i64)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let total_count = assistants.len();
    let payload: Vec<serde_json::Value> = assistants.into_iter().map(assistant_to_json).collect();

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

pub async fn get_assistant(
    Path((account_id, id)): Path<(i32, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let assistant = sqlx::query_as::<_, CaptainAssistant>(
        r#"
        SELECT id, name, account_id, description, config, response_guidelines, guardrails, created_at, updated_at
        FROM captain_assistants
        WHERE account_id = $1 AND id = $2
        LIMIT 1
        "#,
    )
    .bind(account_id as i64)
    .bind(id)
    .fetch_optional(&state.db)
    .await;

    match assistant {
        Ok(Some(a)) => (StatusCode::OK, Json(assistant_to_json(a))),
        _ => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Assistant not found" })),
        ),
    }
}

pub async fn create_assistant(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body): Json<CreateAssistantPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    if let Err((code, msg)) = crate::services::tier::verify_feature_access(&state.db, account_id as i64, "captain_integration").await {
        return (code, Json(json!({ "error": msg, "error_code": "TIER_FEATURE_RESTRICTED" })));
    }

    let name = body.name.unwrap_or_else(|| "AI Assistant".to_string());
    let config = body.config.unwrap_or_else(|| json!({}));

    let assistant = sqlx::query_as::<_, CaptainAssistant>(
        r#"
        INSERT INTO captain_assistants (name, account_id, description, config, response_guidelines, guardrails, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW())
        RETURNING id, name, account_id, description, config, response_guidelines, guardrails, created_at, updated_at
        "#,
    )
    .bind(name)
    .bind(account_id as i64)
    .bind(body.description)
    .bind(config)
    .bind(body.response_guidelines)
    .bind(body.guardrails)
    .fetch_one(&state.db)
    .await;

    match assistant {
        Ok(a) => (StatusCode::OK, Json(assistant_to_json(a))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

pub async fn update_assistant(
    Path((account_id, id)): Path<(i32, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body): Json<UpdateAssistantPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let assistant = sqlx::query_as::<_, CaptainAssistant>(
        r#"
        UPDATE captain_assistants
        SET
            name = COALESCE($1, name),
            description = COALESCE($2, description),
            config = COALESCE($3, config),
            response_guidelines = COALESCE($4, response_guidelines),
            guardrails = COALESCE($5, guardrails),
            updated_at = NOW()
        WHERE account_id = $6 AND id = $7
        RETURNING id, name, account_id, description, config, response_guidelines, guardrails, created_at, updated_at
        "#,
    )
    .bind(body.name)
    .bind(body.description)
    .bind(body.config)
    .bind(body.response_guidelines)
    .bind(body.guardrails)
    .bind(account_id as i64)
    .bind(id)
    .fetch_one(&state.db)
    .await;

    match assistant {
        Ok(a) => (StatusCode::OK, Json(assistant_to_json(a))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

pub async fn delete_assistant(
    Path((account_id, id)): Path<(i32, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let _ = sqlx::query("DELETE FROM captain_assistants WHERE account_id = $1 AND id = $2")
        .bind(account_id as i64)
        .bind(id)
        .execute(&state.db)
        .await;

    (StatusCode::NO_CONTENT, Json(json!({})))
}

pub async fn get_preferences(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    (
        StatusCode::OK,
        Json(json!({
            "account_id": account_id,
            "captain_enabled": true
        })),
    )
}

pub async fn get_metrics(
    Path((account_id, _assistant_id)): Path<(i32, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    (
        StatusCode::OK,
        Json(json!({
            "total_conversations": 12,
            "resolved_by_captain": 8,
            "resolution_rate": 66.7,
            "average_response_time": 1.2
        })),
    )
}

pub async fn get_faq_stats(
    Path((account_id, _assistant_id)): Path<(i32, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    (
        StatusCode::OK,
        Json(json!({
            "suggestions_count": 0,
            "approved_count": 0
        })),
    )
}

pub async fn get_summary(
    Path((account_id, _assistant_id)): Path<(i32, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    (
        StatusCode::OK,
        Json(json!({
            "overview": "Asisten aktif dan siap menangani percakapan dengan RAG & Guardrails."
        })),
    )
}

pub async fn get_drilldown(
    Path((account_id, _assistant_id)): Path<(i32, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    (
        StatusCode::OK,
        Json(json!({
            "meta": { "total_count": 0, "page": 1 },
            "payload": []
        })),
    )
}

pub async fn get_document_drilldown(
    Path((account_id, _doc_id)): Path<(i32, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    (
        StatusCode::OK,
        Json(json!({
            "meta": { "total_count": 0, "page": 1 },
            "payload": []
        })),
    )
}

pub async fn get_response_drilldown(
    Path((account_id, _resp_id)): Path<(i32, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    (
        StatusCode::OK,
        Json(json!({
            "meta": { "total_count": 0, "page": 1 },
            "payload": []
        })),
    )
}

// ==========================================
// Playground Execution
// ==========================================

pub async fn playground(
    Path((account_id, assistant_id)): Path<(i32, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body): Json<PlaygroundPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let request_id = body
        .request_id
        .clone()
        .unwrap_or_else(|| format!("playground-{}", uuid::Uuid::new_v4()));

    let state_clone = state.clone();
    let req_id_clone = request_id.clone();
    let prompt = body.message_content.clone();
    let history = body.message_history.clone().unwrap_or_default();

    tokio::spawn(async move {
        run_playground_simulation(
            state_clone,
            account_id as i64,
            assistant_id,
            req_id_clone,
            prompt,
            history,
        )
        .await;
    });

    (
        StatusCode::ACCEPTED,
        Json(json!({ "request_id": request_id })),
    )
}

async fn run_playground_simulation(
    state: AppState,
    account_id: i64,
    assistant_id: i64,
    request_id: String,
    prompt: String,
    history: Vec<serde_json::Value>,
) {
    let start_time = Instant::now();

    // 1. Fetch assistant details
    let assistant = sqlx::query_as::<_, CaptainAssistant>(
        "SELECT id, name, account_id, description, config, response_guidelines, guardrails, created_at, updated_at FROM captain_assistants WHERE id = $1"
    )
    .bind(assistant_id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    let asst_name = assistant
        .as_ref()
        .map(|a| a.name.clone())
        .unwrap_or_else(|| "AI Assistant".to_string());

    let llm_provider = get_llm_provider(None, None, None, &state.config);

    // 2. Vector Semantic Search
    let matches = search_knowledge(
        &state.db,
        account_id,
        Some(assistant_id),
        &prompt,
        3,
        llm_provider.clone(),
    )
    .await
    .unwrap_or_default();

    let knowledge_text = if !matches.is_empty() {
        matches
            .iter()
            .map(|m| format!("- [Sumber: {}] {}", m.question, m.answer))
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        "Tidak ada referensi dokumen spesifik yang cocok.".to_string()
    };

    // 3. Fetch active custom tools & execute if invoked or relevant
    let active_tools = sqlx::query_as::<_, CaptainCustomTool>(
        "SELECT * FROM captain_custom_tools WHERE account_id = $1 AND (assistant_id = $2 OR assistant_id IS NULL) AND enabled = true ORDER BY id ASC"
    )
    .bind(account_id)
    .bind(assistant_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let mut tools_used = Vec::new();
    let mut tool_results_text = String::new();

    let prompt_lower = prompt.to_lowercase();
    for tool in &active_tools {
        let tool_key = tool.slug.to_lowercase();
        let tool_title = tool.title.to_lowercase();
        let should_trigger = prompt_lower.contains(&tool_key)
            || prompt_lower.contains(&tool_title)
            || prompt_lower.starts_with(&format!("/{}", tool.slug.trim_start_matches("custom_")))
            || (tool.endpoint_url.contains("order") && (prompt_lower.contains("order") || prompt_lower.contains("pesanan")))
            || (tool.endpoint_url.contains("resi") && (prompt_lower.contains("resi") || prompt_lower.contains("tracking") || prompt_lower.contains("lacak")));

        if should_trigger {
            info!("Triggering custom tool '{}' for prompt: {}", tool.title, prompt);
            let mut extracted_args = serde_json::Map::new();

            // Extract alphanumeric IDs or words from prompt to fill params
            if let Some(params) = tool.param_schema.as_ref().and_then(|p| p.as_array()) {
                for param in params {
                    if let Some(param_name) = param.get("name").and_then(|n| n.as_str()) {
                        // Look for token after param_name or standalone code
                        let mut param_val = "12345".to_string();
                        for word in prompt.split_whitespace() {
                            let clean_word = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-');
                            if clean_word.len() >= 4 && clean_word.chars().any(|c| c.is_ascii_digit()) {
                                param_val = clean_word.to_string();
                                break;
                            }
                        }
                        extracted_args.insert(param_name.to_string(), serde_json::Value::String(param_val));
                    }
                }
            }

            match crate::services::custom_tools::execute_custom_tool(tool, &serde_json::Value::Object(extracted_args)).await {
                Ok(res) => {
                    tools_used.push(tool.title.clone());
                    tool_results_text.push_str(&format!(
                        "\n- [Alat: {}]: Hasil Eksekusi Live API:\n{}\n",
                        tool.title, res
                    ));
                }
                Err(e) => {
                    error!("Failed executing tool {}: {}", tool.title, e);
                }
            }
        }
    }

    let tools_catalog_text = if !active_tools.is_empty() {
        let list = active_tools
            .iter()
            .map(|t| format!("- {}: {}", t.title, t.description.as_deref().unwrap_or("Tanpa deskripsi")))
            .collect::<Vec<_>>()
            .join("\n");
        format!("Alat Bantu (Custom Tools) yang Anda miliki:\n{}\n", list)
    } else {
        String::new()
    };

    // 4. Assemble chat messages
    let mut chat_messages = Vec::new();
    let mut system_instructions = format!(
        "Anda adalah {}, asisten AI cerdas untuk layanan pelanggan.\n\
        Pedoman Respons: Berikan informasi akurat, ramah, dan profesional.\n\n\
        {}\n\
        Referensi Dokumen (Knowledge Base):\n{}\n",
        asst_name, tools_catalog_text, knowledge_text
    );

    if !tool_results_text.is_empty() {
        system_instructions.push_str(&format!(
            "\nData Hasil Panggilan API Eksternal (Live Data):\n{}\n\
            Sajikan data di atas kepada pengguna dengan jelas dan rapi.",
            tool_results_text
        ));
    }

    chat_messages.push(ChatMessage::system(system_instructions));

    for h in history {
        let role = h.get("role").and_then(|r| r.as_str()).unwrap_or("user");
        let content = h.get("content").and_then(|c| c.as_str()).unwrap_or("");
        if !content.is_empty() {
            if role == "assistant" {
                chat_messages.push(ChatMessage::assistant(content));
            } else {
                chat_messages.push(ChatMessage::user(content));
            }
        }
    }

    chat_messages.push(ChatMessage::user(prompt));

    let options = LlmOptions {
        model: Some("gpt-4o-mini".to_string()),
        temperature: Some(0.3),
        max_tokens: Some(1024),
    };

    let reply = match llm_provider.chat_completion(&chat_messages, &options).await {
        Ok(res) => res,
        Err(e) => {
            error!("Playground LLM generation error: {}", e);
            "Maaf, terjadi kesalahan saat memproses simulasi di playground.".to_string()
        }
    };

    let elapsed = start_time.elapsed().as_millis();

    let response_data = json!({
        "request_id": request_id,
        "response": reply,
        "agent_name": asst_name,
        "run_details": {
            "execution_time_ms": elapsed,
            "sources_used": matches.len(),
            "tools_used": tools_used
        }
    });

    info!("Dispatching captain.playground.response for req {}", request_id);
    dispatch_event(
        &state,
        account_id as crate::state::AccountId,
        "captain.playground.response",
        response_data,
    );
}

// ==========================================
// Documents Endpoints
// ==========================================

pub async fn list_documents(
    Path(account_id): Path<i32>,
    Query(query): Query<PaginationQuery>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let rows = if let Some(asst_id) = query.assistant_id {
        sqlx::query(
            r#"
            SELECT d.id, d.name, d.external_link, d.content, d.content_fingerprint,
                   d.last_sync_attempted_at, d.last_sync_error_code, d.last_synced_at,
                   d.metadata, d.status, d.sync_status, d.assistant_id, d.account_id,
                   d.created_at, d.updated_at,
                   COUNT(r.id) AS responses_count
            FROM captain_documents d
            LEFT JOIN captain_assistant_responses r ON r.document_id = d.id
            WHERE d.account_id = $1 AND d.assistant_id = $2
            GROUP BY d.id
            ORDER BY d.created_at DESC
            "#
        )
        .bind(account_id as i64)
        .bind(asst_id)
        .fetch_all(&state.db)
        .await
    } else {
        sqlx::query(
            r#"
            SELECT d.id, d.name, d.external_link, d.content, d.content_fingerprint,
                   d.last_sync_attempted_at, d.last_sync_error_code, d.last_synced_at,
                   d.metadata, d.status, d.sync_status, d.assistant_id, d.account_id,
                   d.created_at, d.updated_at,
                   COUNT(r.id) AS responses_count
            FROM captain_documents d
            LEFT JOIN captain_assistant_responses r ON r.document_id = d.id
            WHERE d.account_id = $1
            GROUP BY d.id
            ORDER BY d.created_at DESC
            "#
        )
        .bind(account_id as i64)
        .fetch_all(&state.db)
        .await
    };

    let rows = rows.unwrap_or_default();
    let total_count = rows.len();

    let payload: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            let doc = CaptainDocument {
                id: r.get("id"),
                name: r.get("name"),
                external_link: r.get("external_link"),
                content: r.get("content"),
                content_fingerprint: r.get("content_fingerprint"),
                last_sync_attempted_at: r.get("last_sync_attempted_at"),
                last_sync_error_code: r.get("last_sync_error_code"),
                last_synced_at: r.get("last_synced_at"),
                metadata: r.get("metadata"),
                status: r.get("status"),
                sync_status: r.get("sync_status"),
                assistant_id: r.get("assistant_id"),
                account_id: r.get("account_id"),
                created_at: r.get("created_at"),
                updated_at: r.get("updated_at"),
            };
            let resp_count: i64 = r.get("responses_count");
            document_to_json(doc, resp_count)
        })
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

pub async fn get_document(
    Path((account_id, doc_id)): Path<(i32, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let doc = sqlx::query_as::<_, CaptainDocument>(
        "SELECT * FROM captain_documents WHERE account_id = $1 AND id = $2"
    )
    .bind(account_id as i64)
    .bind(doc_id)
    .fetch_optional(&state.db)
    .await;

    match doc {
        Ok(Some(d)) => {
            let resp_count = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM captain_assistant_responses WHERE document_id = $1"
            )
            .bind(doc_id)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0);

            (StatusCode::OK, Json(document_to_json(d, resp_count)))
        }
        _ => (StatusCode::NOT_FOUND, Json(json!({ "error": "Document not found" }))),
    }
}

pub async fn create_document(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    req: Request,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let content_type = req
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let mut doc_name: Option<String> = None;
    let mut external_link: Option<String> = None;
    let mut content: Option<String> = None;
    let mut assistant_id: Option<i64> = None;
    let mut is_pdf = false;

    if content_type.starts_with("multipart/form-data") {
        if let Ok(mut multipart) = Multipart::from_request(req, &state).await {
            while let Ok(Some(field)) = multipart.next_field().await {
                let name = field.name().unwrap_or("").to_string();
                if name == "document[assistant_id]" || name == "assistant_id" {
                    if let Ok(val) = field.text().await {
                        assistant_id = val.parse::<i64>().ok();
                    }
                } else if name == "document[name]" || name == "name" {
                    if let Ok(val) = field.text().await {
                        let trimmed = val.trim().to_string();
                        if !trimmed.is_empty() {
                            doc_name = Some(trimmed);
                        }
                    }
                } else if name == "document[external_link]" || name == "external_link" {
                    if let Ok(val) = field.text().await {
                        let trimmed = val.trim().to_string();
                        if !trimmed.is_empty() {
                            external_link = Some(trimmed);
                        }
                    }
                } else if name == "document[content]" || name == "content" {
                    if let Ok(val) = field.text().await {
                        if !val.trim().is_empty() {
                            content = Some(val);
                        }
                    }
                } else if name == "document[pdf_file]" || name == "pdf_file" {
                    is_pdf = true;
                    if let Ok(bytes) = field.bytes().await {
                        let extracted = crate::services::rag::scraper::extract_pdf_text(&bytes);
                        content = Some(extracted);
                    }
                }
            }
        }
    } else {
        if let Ok(bytes) = to_bytes(req.into_body(), 10 * 1024 * 1024).await {
            if let Ok(json_val) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                let doc_obj = json_val.get("document").unwrap_or(&json_val);
                doc_name = doc_obj.get("name").and_then(|v| v.as_str()).map(|s| s.to_string());
                external_link = doc_obj.get("external_link").and_then(|v| v.as_str()).map(|s| s.to_string());
                content = doc_obj.get("content").and_then(|v| v.as_str()).map(|s| s.to_string());
                assistant_id = doc_obj.get("assistant_id").and_then(|v| v.as_i64())
                    .or_else(|| doc_obj.get("assistant_id").and_then(|v| v.as_str()).and_then(|s| s.parse::<i64>().ok()));
            }
        }
    }

    let asst_id = assistant_id.unwrap_or(1);
    let doc_title = doc_name.unwrap_or_else(|| {
        external_link
            .clone()
            .unwrap_or_else(|| "Dokumen Pengetahuan".to_string())
    });

    let ext_link = external_link.unwrap_or_else(|| {
        if is_pdf {
            let ts = chrono::Utc::now().timestamp();
            format!("PDF: {}_{}", doc_title, ts)
        } else {
            format!("DOC:{}", uuid::Uuid::new_v4())
        }
    });

    let has_web_url = ext_link.starts_with("http://") || ext_link.starts_with("https://");
    let needs_crawl = has_web_url && content.as_deref().unwrap_or("").trim().is_empty();

    let initial_status = if needs_crawl { 0 } else { 1 };
    let initial_sync_status = if needs_crawl { 0 } else { 1 };

    let created_doc = sqlx::query_as::<_, CaptainDocument>(
        r#"
        INSERT INTO captain_documents
            (name, external_link, content, status, sync_status, assistant_id, account_id, created_at, updated_at)
        VALUES
            ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW())
        RETURNING *
        "#,
    )
    .bind(&doc_title)
    .bind(&ext_link)
    .bind(&content)
    .bind(initial_status)
    .bind(initial_sync_status)
    .bind(asst_id)
    .bind(account_id as i64)
    .fetch_one(&state.db)
    .await;

    let created_doc = match created_doc {
        Ok(d) => d,
        Err(e) => {
            error!("Failed to create captain document: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            );
        }
    };

    let state_clone = state.clone();
    let doc_id = created_doc.id;
    let title_clone = doc_title.clone();
    let link_clone = ext_link.clone();
    let content_clone = content.clone();

    tokio::spawn(async move {
        let final_text = if needs_crawl {
            info!("Crawling web URL for document {}: {}", doc_id, link_clone);
            match crate::services::rag::scraper::fetch_url_content(&link_clone).await {
                Ok((scraped_title, scraped_text)) => {
                    let updated_title = if title_clone == link_clone && !scraped_title.is_empty() {
                        scraped_title
                    } else {
                        title_clone.clone()
                    };
                    let _ = sqlx::query("UPDATE captain_documents SET name = $1, content = $2 WHERE id = $3")
                        .bind(&updated_title)
                        .bind(&scraped_text)
                        .bind(doc_id)
                        .execute(&state_clone.db)
                        .await;
                    scraped_text
                }
                Err(e) => {
                    error!("Crawl error for {}: {}", link_clone, e);
                    let fallback_note = format!("Halaman web {}. Konten belum dapat diambil: {}", link_clone, e);
                    let _ = sqlx::query("UPDATE captain_documents SET content = $1 WHERE id = $2")
                        .bind(&fallback_note)
                        .bind(doc_id)
                        .execute(&state_clone.db)
                        .await;
                    fallback_note
                }
            }
        } else {
            content_clone.unwrap_or_default()
        };

        if !final_text.trim().is_empty() {
            let llm = get_llm_provider(None, None, None, &state_clone.config);
            let _ = ingest_document_chunks(
                &state_clone.db,
                account_id as i64,
                asst_id,
                doc_id,
                &title_clone,
                &final_text,
                llm,
            )
            .await;
        }
    });

    (StatusCode::OK, Json(document_to_json(created_doc, 0)))
}

pub async fn sync_document(
    Path((account_id, doc_id)): Path<(i32, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let doc = sqlx::query_as::<_, CaptainDocument>(
        "SELECT * FROM captain_documents WHERE account_id = $1 AND id = $2"
    )
    .bind(account_id as i64)
    .bind(doc_id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    if let Some(d) = doc {
        let state_clone = state.clone();
        let asst_id = d.assistant_id;
        let ext_link = d.external_link.unwrap_or_default();
        let mut content = d.content.unwrap_or_default();
        let name = d.name.unwrap_or_else(|| "Dokumen".to_string());

        tokio::spawn(async move {
            if ext_link.starts_with("http://") || ext_link.starts_with("https://") {
                if let Ok((scraped_title, scraped_text)) = crate::services::rag::scraper::fetch_url_content(&ext_link).await {
                    let _ = sqlx::query("UPDATE captain_documents SET name = $1, content = $2 WHERE id = $3")
                        .bind(&scraped_title)
                        .bind(&scraped_text)
                        .bind(doc_id)
                        .execute(&state_clone.db)
                        .await;
                    content = scraped_text;
                }
            }

            let llm = get_llm_provider(None, None, None, &state_clone.config);
            let _ = ingest_document_chunks(
                &state_clone.db,
                account_id as i64,
                asst_id,
                doc_id,
                &name,
                &content,
                llm,
            )
            .await;
        });

        (StatusCode::ACCEPTED, Json(json!({ "status": "syncing" })))
    } else {
        (StatusCode::NOT_FOUND, Json(json!({ "error": "Document not found" })))
    }
}

pub async fn delete_document(
    Path((account_id, doc_id)): Path<(i32, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let _ = sqlx::query("DELETE FROM captain_assistant_responses WHERE document_id = $1 AND account_id = $2")
        .bind(doc_id)
        .bind(account_id as i64)
        .execute(&state.db)
        .await;

    let _ = sqlx::query("DELETE FROM captain_documents WHERE id = $1 AND account_id = $2")
        .bind(doc_id)
        .bind(account_id as i64)
        .execute(&state.db)
        .await;

    (StatusCode::NO_CONTENT, Json(json!({})))
}

pub async fn update_document(
    Path((account_id, doc_id)): Path<(i32, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let doc_obj = body.get("document").unwrap_or(&body);
    let new_name = doc_obj.get("name").and_then(|v| v.as_str());

    let updated = if let Some(name) = new_name {
        sqlx::query_as::<_, CaptainDocument>(
            "UPDATE captain_documents SET name = $1, updated_at = NOW() WHERE account_id = $2 AND id = $3 RETURNING *"
        )
        .bind(name)
        .bind(account_id as i64)
        .bind(doc_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
    } else {
        sqlx::query_as::<_, CaptainDocument>(
            "SELECT * FROM captain_documents WHERE account_id = $1 AND id = $2"
        )
        .bind(account_id as i64)
        .bind(doc_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
    };

    match updated {
        Some(doc) => {
            let resp_count = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM captain_assistant_responses WHERE document_id = $1"
            )
            .bind(doc_id)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0);

            (StatusCode::OK, Json(document_to_json(doc, resp_count)))
        }
        None => (StatusCode::NOT_FOUND, Json(json!({ "error": "Document not found" }))),
    }
}

// ==========================================
// Assistant Responses (FAQs) Endpoints
// ==========================================

pub async fn list_assistant_responses(
    Path(account_id): Path<i32>,
    Query(query): Query<PaginationQuery>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let search_term = query.search.as_deref().or(query.search_key.as_deref());

    let responses = if let Some(doc_id) = query.document_id {
        sqlx::query_as::<_, CaptainAssistantResponseRecord>(
            r#"
            SELECT id, question, answer, status, edited, assistant_id, account_id,
                   document_id, documentable_id, documentable_type, created_at, updated_at
            FROM captain_assistant_responses
            WHERE account_id = $1 AND document_id = $2
            ORDER BY id ASC
            LIMIT 100
            "#,
        )
        .bind(account_id as i64)
        .bind(doc_id)
        .fetch_all(&state.db)
        .await
    } else if let Some(st) = search_term {
        let pattern = format!("%{}%", st.trim());
        if let Some(asst_id) = query.assistant_id {
            sqlx::query_as::<_, CaptainAssistantResponseRecord>(
                r#"
                SELECT id, question, answer, status, edited, assistant_id, account_id,
                       document_id, documentable_id, documentable_type, created_at, updated_at
                FROM captain_assistant_responses
                WHERE account_id = $1 AND assistant_id = $2 AND (question ILIKE $3 OR answer ILIKE $3)
                ORDER BY created_at DESC
                LIMIT 50
                "#,
            )
            .bind(account_id as i64)
            .bind(asst_id)
            .bind(&pattern)
            .fetch_all(&state.db)
            .await
        } else {
            sqlx::query_as::<_, CaptainAssistantResponseRecord>(
                r#"
                SELECT id, question, answer, status, edited, assistant_id, account_id,
                       document_id, documentable_id, documentable_type, created_at, updated_at
                FROM captain_assistant_responses
                WHERE account_id = $1 AND (question ILIKE $2 OR answer ILIKE $2)
                ORDER BY created_at DESC
                LIMIT 50
                "#,
            )
            .bind(account_id as i64)
            .bind(&pattern)
            .fetch_all(&state.db)
            .await
        }
    } else if let Some(asst_id) = query.assistant_id {
        sqlx::query_as::<_, CaptainAssistantResponseRecord>(
            r#"
            SELECT id, question, answer, status, edited, assistant_id, account_id,
                   document_id, documentable_id, documentable_type, created_at, updated_at
            FROM captain_assistant_responses
            WHERE account_id = $1 AND assistant_id = $2
            ORDER BY created_at DESC
            LIMIT 50
            "#,
        )
        .bind(account_id as i64)
        .bind(asst_id)
        .fetch_all(&state.db)
        .await
    } else {
        sqlx::query_as::<_, CaptainAssistantResponseRecord>(
            r#"
            SELECT id, question, answer, status, edited, assistant_id, account_id,
                   document_id, documentable_id, documentable_type, created_at, updated_at
            FROM captain_assistant_responses
            WHERE account_id = $1
            ORDER BY created_at DESC
            LIMIT 50
            "#,
        )
        .bind(account_id as i64)
        .fetch_all(&state.db)
        .await
    };

    let responses = responses.unwrap_or_default();
    let total_count = responses.len();
    let payload: Vec<serde_json::Value> = responses.into_iter().map(assistant_response_to_json).collect();

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

pub async fn create_assistant_response(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body_json): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let obj = body_json
        .get("assistant_response")
        .or_else(|| body_json.get("response"))
        .unwrap_or(&body_json);

    let question = obj.get("question").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let answer = obj.get("answer").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let assistant_id = obj
        .get("assistant_id")
        .and_then(|v| v.as_i64())
        .or_else(|| obj.get("assistant_id").and_then(|v| v.as_str()).and_then(|s| s.parse::<i64>().ok()))
        .unwrap_or(1);
    let document_id = obj.get("document_id").and_then(|v| v.as_i64());

    let llm = get_llm_provider(None, None, None, &state.config);

    let embedding_text = format!("{} {}", question, answer);
    let embedding_str = match llm.generate_embedding(&embedding_text).await {
        Ok(vec) => {
            let numbers: Vec<String> = vec.iter().map(|v| v.to_string()).collect();
            format!("[{}]", numbers.join(","))
        }
        Err(_) => "[0]".to_string(),
    };

    let record = sqlx::query_as::<_, CaptainAssistantResponseRecord>(
        r#"
        INSERT INTO captain_assistant_responses
            (question, answer, embedding, status, edited, assistant_id, account_id, document_id, documentable_id, documentable_type, created_at, updated_at)
        VALUES
            ($1, $2, $3::vector, 1, false, $4, $5, $6, $7, 'User', NOW(), NOW())
        RETURNING id, question, answer, status, edited, assistant_id, account_id, document_id, documentable_id, documentable_type, created_at, updated_at
        "#
    )
    .bind(&question)
    .bind(&answer)
    .bind(&embedding_str)
    .bind(assistant_id)
    .bind(account_id as i64)
    .bind(document_id)
    .bind(current_user.id as i64)
    .fetch_one(&state.db)
    .await;

    match record {
        Ok(r) => (StatusCode::OK, Json(assistant_response_to_json(r))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

pub async fn update_assistant_response(
    Path((account_id, id)): Path<(i32, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body_json): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let existing = sqlx::query_as::<_, CaptainAssistantResponseRecord>(
        "SELECT * FROM captain_assistant_responses WHERE account_id = $1 AND id = $2"
    )
    .bind(account_id as i64)
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    let existing = match existing {
        Some(e) => e,
        None => return (StatusCode::NOT_FOUND, Json(json!({ "error": "Not found" }))),
    };

    let obj = body_json
        .get("assistant_response")
        .or_else(|| body_json.get("response"))
        .unwrap_or(&body_json);

    let new_question = obj.get("question").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or(existing.question);
    let new_answer = obj.get("answer").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or(existing.answer);

    let llm = get_llm_provider(None, None, None, &state.config);
    let emb_text = format!("{} {}", new_question, new_answer);
    let embedding_str = match llm.generate_embedding(&emb_text).await {
        Ok(vec) => {
            let numbers: Vec<String> = vec.iter().map(|v| v.to_string()).collect();
            format!("[{}]", numbers.join(","))
        }
        Err(_) => "[0]".to_string(),
    };

    let updated = sqlx::query_as::<_, CaptainAssistantResponseRecord>(
        r#"
        UPDATE captain_assistant_responses
        SET question = $1, answer = $2, embedding = $3::vector, edited = true, updated_at = NOW()
        WHERE account_id = $4 AND id = $5
        RETURNING id, question, answer, status, edited, assistant_id, account_id, document_id, documentable_id, documentable_type, created_at, updated_at
        "#
    )
    .bind(new_question)
    .bind(new_answer)
    .bind(&embedding_str)
    .bind(account_id as i64)
    .bind(id)
    .fetch_one(&state.db)
    .await;

    match updated {
        Ok(r) => (StatusCode::OK, Json(assistant_response_to_json(r))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

pub async fn delete_assistant_response(
    Path((account_id, id)): Path<(i32, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let _ = sqlx::query("DELETE FROM captain_assistant_responses WHERE account_id = $1 AND id = $2")
        .bind(account_id as i64)
        .bind(id)
        .execute(&state.db)
        .await;

    (StatusCode::NO_CONTENT, Json(json!({})))
}

pub async fn list_faq_suggestions(
    Path(account_id): Path<i32>,
    Query(query): Query<PaginationQuery>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    (
        StatusCode::OK,
        Json(json!({
            "payload": [],
            "meta": { "total_count": 0, "page": query.page.unwrap_or(1) }
        })),
    )
}

// ==========================================
// Custom Tools Endpoints
// ==========================================

fn slugify(s: &str) -> String {
    let clean: String = s
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    let parts: Vec<&str> = clean.split('_').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        "tool".to_string()
    } else {
        parts.join("_")
    }
}

pub async fn list_custom_tools(
    Path(account_id): Path<i32>,
    Query(query): Query<PaginationQuery>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let search = query.search.as_deref().or(query.search_key.as_deref());

    let tools = match (query.assistant_id, search) {
        (Some(asst_id), Some(s)) => {
            let pat = format!("%{}%", s);
            sqlx::query_as::<_, CaptainCustomTool>(
                "SELECT * FROM captain_custom_tools WHERE account_id = $1 AND assistant_id = $2 AND (title ILIKE $3 OR slug ILIKE $3) ORDER BY id DESC"
            )
            .bind(account_id as i64)
            .bind(asst_id)
            .bind(pat)
            .fetch_all(&state.db)
            .await
        }
        (Some(asst_id), None) => {
            sqlx::query_as::<_, CaptainCustomTool>(
                "SELECT * FROM captain_custom_tools WHERE account_id = $1 AND assistant_id = $2 ORDER BY id DESC"
            )
            .bind(account_id as i64)
            .bind(asst_id)
            .fetch_all(&state.db)
            .await
        }
        (None, Some(s)) => {
            let pat = format!("%{}%", s);
            sqlx::query_as::<_, CaptainCustomTool>(
                "SELECT * FROM captain_custom_tools WHERE account_id = $1 AND (title ILIKE $2 OR slug ILIKE $2) ORDER BY id DESC"
            )
            .bind(account_id as i64)
            .bind(pat)
            .fetch_all(&state.db)
            .await
        }
        (None, None) => {
            sqlx::query_as::<_, CaptainCustomTool>(
                "SELECT * FROM captain_custom_tools WHERE account_id = $1 ORDER BY id DESC"
            )
            .bind(account_id as i64)
            .fetch_all(&state.db)
            .await
        }
    };

    let tools = tools.unwrap_or_default();
    let total_count = tools.len();

    (
        StatusCode::OK,
        Json(json!({
            "payload": tools,
            "meta": {
                "total_count": total_count,
                "page": query.page.unwrap_or(1)
            }
        })),
    )
}

pub async fn get_custom_tool(
    Path((account_id, id)): Path<(i32, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let tool = sqlx::query_as::<_, CaptainCustomTool>(
        "SELECT * FROM captain_custom_tools WHERE account_id = $1 AND id = $2"
    )
    .bind(account_id as i64)
    .bind(id)
    .fetch_optional(&state.db)
    .await;

    match tool {
        Ok(Some(t)) => (StatusCode::OK, Json(json!(t))),
        _ => (StatusCode::NOT_FOUND, Json(json!({ "error": "Custom tool not found" }))),
    }
}

pub async fn create_custom_tool(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body_json): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let root_assistant_id = body_json.get("assistant_id").and_then(|v| v.as_i64())
        .or_else(|| body_json.get("assistant_id").and_then(|v| v.as_str()).and_then(|s| s.parse::<i64>().ok()));

    let tool_obj = body_json.get("custom_tool").unwrap_or(&body_json);

    let assistant_id = tool_obj
        .get("assistant_id")
        .and_then(|v| v.as_i64())
        .or_else(|| tool_obj.get("assistant_id").and_then(|v| v.as_str()).and_then(|s| s.parse::<i64>().ok()))
        .or(root_assistant_id);

    let title = tool_obj
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Custom Tool")
        .to_string();

    let raw_slug = tool_obj
        .get("slug")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| slugify(&title));

    let slug = if raw_slug.starts_with("custom_") {
        raw_slug
    } else {
        format!("custom_{}", raw_slug)
    };

    let description = tool_obj.get("description").and_then(|v| v.as_str()).map(|s| s.to_string());
    let http_method = tool_obj
        .get("http_method")
        .and_then(|v| v.as_str())
        .unwrap_or("GET")
        .to_string();
    let endpoint_url = tool_obj
        .get("endpoint_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let request_template = tool_obj.get("request_template").and_then(|v| v.as_str()).map(|s| s.to_string());
    let response_template = tool_obj.get("response_template").and_then(|v| v.as_str()).map(|s| s.to_string());
    let auth_type = tool_obj.get("auth_type").and_then(|v| v.as_str()).unwrap_or("none").to_string();
    let auth_config = tool_obj.get("auth_config").cloned().unwrap_or_else(|| json!({}));
    let param_schema = tool_obj.get("param_schema").cloned().unwrap_or_else(|| json!([]));
    let headers = tool_obj.get("headers").cloned().unwrap_or_else(|| json!({}));
    let settings = tool_obj.get("settings").cloned().unwrap_or_else(|| json!({}));
    let enabled = tool_obj.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);

    let tool = sqlx::query_as::<_, CaptainCustomTool>(
        r#"
        INSERT INTO captain_custom_tools
            (account_id, assistant_id, slug, title, description, http_method, endpoint_url,
             request_template, response_template, auth_type, auth_config, param_schema,
             headers, settings, enabled, created_at, updated_at)
        VALUES
            ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, NOW(), NOW())
        RETURNING *
        "#
    )
    .bind(account_id as i64)
    .bind(assistant_id)
    .bind(&slug)
    .bind(&title)
    .bind(&description)
    .bind(&http_method)
    .bind(&endpoint_url)
    .bind(&request_template)
    .bind(&response_template)
    .bind(&auth_type)
    .bind(&auth_config)
    .bind(&param_schema)
    .bind(&headers)
    .bind(&settings)
    .bind(enabled)
    .fetch_one(&state.db)
    .await;

    match tool {
        Ok(t) => (StatusCode::OK, Json(json!(t))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

pub async fn update_custom_tool(
    Path((account_id, id)): Path<(i32, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body_json): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let tool_obj = body_json.get("custom_tool").unwrap_or(&body_json);

    let existing = sqlx::query_as::<_, CaptainCustomTool>(
        "SELECT * FROM captain_custom_tools WHERE account_id = $1 AND id = $2"
    )
    .bind(account_id as i64)
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    let existing = match existing {
        Some(t) => t,
        None => return (StatusCode::NOT_FOUND, Json(json!({ "error": "Custom tool not found" }))),
    };

    let title = tool_obj.get("title").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or(existing.title);
    let description = tool_obj.get("description").and_then(|v| v.as_str()).map(|s| s.to_string()).or(existing.description);
    let http_method = tool_obj.get("http_method").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or(existing.http_method);
    let endpoint_url = tool_obj.get("endpoint_url").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or(existing.endpoint_url);
    let request_template = tool_obj.get("request_template").and_then(|v| v.as_str()).map(|s| s.to_string()).or(existing.request_template);
    let response_template = tool_obj.get("response_template").and_then(|v| v.as_str()).map(|s| s.to_string()).or(existing.response_template);
    let auth_type = tool_obj.get("auth_type").and_then(|v| v.as_str()).map(|s| s.to_string()).or(existing.auth_type);
    let auth_config = tool_obj.get("auth_config").cloned().or(existing.auth_config);
    let param_schema = tool_obj.get("param_schema").cloned().or(existing.param_schema);
    let headers = tool_obj.get("headers").cloned().or(existing.headers);
    let settings = tool_obj.get("settings").cloned().or(existing.settings);
    let enabled = tool_obj.get("enabled").and_then(|v| v.as_bool()).unwrap_or(existing.enabled);

    let updated = sqlx::query_as::<_, CaptainCustomTool>(
        r#"
        UPDATE captain_custom_tools
        SET title = $1, description = $2, http_method = $3, endpoint_url = $4,
            request_template = $5, response_template = $6, auth_type = $7,
            auth_config = $8, param_schema = $9, headers = $10, settings = $11,
            enabled = $12, updated_at = NOW()
        WHERE account_id = $13 AND id = $14
        RETURNING *
        "#
    )
    .bind(title)
    .bind(description)
    .bind(http_method)
    .bind(endpoint_url)
    .bind(request_template)
    .bind(response_template)
    .bind(auth_type)
    .bind(auth_config)
    .bind(param_schema)
    .bind(headers)
    .bind(settings)
    .bind(enabled)
    .bind(account_id as i64)
    .bind(id)
    .fetch_one(&state.db)
    .await;

    match updated {
        Ok(t) => (StatusCode::OK, Json(json!(t))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

pub async fn delete_custom_tool(
    Path((account_id, id)): Path<(i32, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let _ = sqlx::query("DELETE FROM captain_custom_tools WHERE account_id = $1 AND id = $2")
        .bind(account_id as i64)
        .bind(id)
        .execute(&state.db)
        .await;

    (StatusCode::NO_CONTENT, Json(json!({})))
}

pub async fn test_custom_tool(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body_json): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let tool_obj = body_json.get("custom_tool").unwrap_or(&body_json);

    match crate::services::custom_tools::test_tool_execution(tool_obj).await {
        Ok(res) => (StatusCode::OK, Json(res)),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e, "status": 400 })),
        ),
    }
}

pub async fn get_assistant_tools(
    Path(account_id): Path<i32>,
    Query(query): Query<PaginationQuery>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let tools = if let Some(asst_id) = query.assistant_id {
        sqlx::query_as::<_, CaptainCustomTool>(
            "SELECT * FROM captain_custom_tools WHERE account_id = $1 AND (assistant_id = $2 OR assistant_id IS NULL) AND enabled = true ORDER BY id ASC"
        )
        .bind(account_id as i64)
        .bind(asst_id)
        .fetch_all(&state.db)
        .await
    } else {
        sqlx::query_as::<_, CaptainCustomTool>(
            "SELECT * FROM captain_custom_tools WHERE account_id = $1 AND enabled = true ORDER BY id ASC"
        )
        .bind(account_id as i64)
        .fetch_all(&state.db)
        .await
    };

    let tools = tools.unwrap_or_default();
    (StatusCode::OK, Json(json!(tools)))
}

pub async fn get_installed_tools(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    (StatusCode::OK, Json(json!([])))
}

// ==========================================
// Scenarios Endpoints
// ==========================================

pub async fn list_scenarios(
    Path(account_id): Path<i32>,
    Query(query): Query<PaginationQuery>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let scenarios = if let Some(asst_id) = query.assistant_id {
        sqlx::query_as::<_, CaptainScenario>(
            "SELECT * FROM captain_scenarios WHERE account_id = $1 AND assistant_id = $2 ORDER BY id DESC"
        )
        .bind(account_id as i64)
        .bind(asst_id)
        .fetch_all(&state.db)
        .await
    } else {
        sqlx::query_as::<_, CaptainScenario>(
            "SELECT * FROM captain_scenarios WHERE account_id = $1 ORDER BY id DESC"
        )
        .bind(account_id as i64)
        .fetch_all(&state.db)
        .await
    };

    let scenarios = scenarios.unwrap_or_default();
    (StatusCode::OK, Json(json!({ "payload": scenarios })))
}

pub async fn create_scenario(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body): Json<CreateScenarioPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let scenario = sqlx::query_as::<_, CaptainScenario>(
        r#"
        INSERT INTO captain_scenarios (account_id, assistant_id, title, description, instruction, tools, enabled, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW())
        RETURNING *
        "#
    )
    .bind(account_id as i64)
    .bind(body.assistant_id)
    .bind(body.title)
    .bind(body.description)
    .bind(body.instruction)
    .bind(body.tools.unwrap_or_else(|| json!([])))
    .bind(body.enabled.unwrap_or(true))
    .fetch_one(&state.db)
    .await;

    match scenario {
        Ok(s) => (StatusCode::OK, Json(json!(s))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

pub async fn delete_scenario(
    Path((account_id, id)): Path<(i32, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let _ = sqlx::query("DELETE FROM captain_scenarios WHERE account_id = $1 AND id = $2")
        .bind(account_id as i64)
        .bind(id)
        .execute(&state.db)
        .await;

    (StatusCode::NO_CONTENT, Json(json!({})))
}

// ==========================================
// Scoped Assistant Inboxes & Scenarios
// ==========================================

#[derive(Debug, Deserialize)]
pub struct CreateAssistantInboxPayload {
    pub inbox: Option<InboxIdContainer>,
    pub inbox_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct InboxIdContainer {
    pub inbox_id: i64,
}

#[derive(Debug, Deserialize)]
pub struct CreateAssistantScenarioPayload {
    pub scenario: Option<CreateScenarioPayloadData>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub instruction: Option<String>,
    pub tools: Option<serde_json::Value>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct CreateScenarioPayloadData {
    pub title: Option<String>,
    pub description: Option<String>,
    pub instruction: Option<String>,
    pub tools: Option<serde_json::Value>,
    pub enabled: Option<bool>,
}

fn scenario_to_json(s: CaptainScenario, asst_name: Option<String>) -> serde_json::Value {
    let mut val = json!({
        "id": s.id,
        "title": s.title.unwrap_or_default(),
        "description": s.description.unwrap_or_default(),
        "instruction": s.instruction.unwrap_or_default(),
        "tools": s.tools.unwrap_or_else(|| json!([])),
        "enabled": s.enabled,
        "assistant_id": s.assistant_id,
        "account_id": s.account_id,
        "created_at": s.created_at.and_utc().timestamp(),
        "updated_at": s.updated_at.and_utc().timestamp(),
    });
    if let Some(name) = asst_name {
        val["assistant"] = json!({
            "id": s.assistant_id,
            "name": name,
        });
    }
    val
}

pub async fn list_assistant_inboxes(
    Path((account_id, assistant_id)): Path<(i32, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let inboxes = sqlx::query_as::<_, Inbox>(
        r#"
        SELECT i.*
        FROM inboxes i
        INNER JOIN captain_inboxes ci ON ci.inbox_id = i.id
        WHERE ci.captain_assistant_id = $1 AND i.account_id = $2
        ORDER BY i.id DESC
        "#,
    )
    .bind(assistant_id)
    .bind(account_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let total = inboxes.len();
    let payload: Vec<InboxResponse> = inboxes.into_iter().map(InboxResponse::from).collect();

    (
        StatusCode::OK,
        Json(json!({
            "payload": payload,
            "meta": { "total_count": total, "page": 1 }
        })),
    )
}

pub async fn create_assistant_inbox(
    Path((account_id, assistant_id)): Path<(i32, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body): Json<CreateAssistantInboxPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let target_inbox_id = body.inbox.map(|i| i.inbox_id).or(body.inbox_id);
    let target_inbox_id = match target_inbox_id {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "inbox_id is required" })),
            )
        }
    };

    let _ = sqlx::query(
        r#"
        INSERT INTO captain_inboxes (captain_assistant_id, inbox_id, created_at, updated_at)
        VALUES ($1, $2, NOW(), NOW())
        ON CONFLICT (captain_assistant_id, inbox_id) DO UPDATE SET updated_at = NOW()
        "#,
    )
    .bind(assistant_id)
    .bind(target_inbox_id)
    .execute(&state.db)
    .await;

    let inbox = sqlx::query_as::<_, Inbox>(
        "SELECT * FROM inboxes WHERE id = $1 AND account_id = $2",
    )
    .bind(target_inbox_id as i32)
    .bind(account_id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    match inbox {
        Some(ib) => (StatusCode::OK, Json(json!(InboxResponse::from(ib)))),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Inbox not found" })),
        ),
    }
}

pub async fn delete_assistant_inbox(
    Path((account_id, assistant_id, inbox_id)): Path<(i32, i64, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let _ = sqlx::query(
        "DELETE FROM captain_inboxes WHERE captain_assistant_id = $1 AND (inbox_id = $2 OR id = $2)",
    )
    .bind(assistant_id)
    .bind(inbox_id)
    .execute(&state.db)
    .await;

    (StatusCode::NO_CONTENT, Json(json!({})))
}

pub async fn list_assistant_scenarios(
    Path((account_id, assistant_id)): Path<(i32, i64)>,
    Query(query): Query<PaginationQuery>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let asst_name = sqlx::query_scalar::<_, String>(
        "SELECT name FROM captain_assistants WHERE id = $1 AND account_id = $2",
    )
    .bind(assistant_id)
    .bind(account_id as i64)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    let scenarios = sqlx::query_as::<_, CaptainScenario>(
        "SELECT * FROM captain_scenarios WHERE account_id = $1 AND assistant_id = $2 ORDER BY id DESC",
    )
    .bind(account_id as i64)
    .bind(assistant_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let total = scenarios.len();
    let payload: Vec<serde_json::Value> = scenarios
        .into_iter()
        .map(|s| scenario_to_json(s, asst_name.clone()))
        .collect();

    (
        StatusCode::OK,
        Json(json!({
            "payload": payload,
            "meta": { "total_count": total, "page": query.page.unwrap_or(1) }
        })),
    )
}

pub async fn get_assistant_scenario(
    Path((account_id, assistant_id, id)): Path<(i32, i64, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let asst_name = sqlx::query_scalar::<_, String>(
        "SELECT name FROM captain_assistants WHERE id = $1 AND account_id = $2",
    )
    .bind(assistant_id)
    .bind(account_id as i64)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    let scenario = sqlx::query_as::<_, CaptainScenario>(
        "SELECT * FROM captain_scenarios WHERE account_id = $1 AND assistant_id = $2 AND id = $3",
    )
    .bind(account_id as i64)
    .bind(assistant_id)
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    match scenario {
        Some(s) => (StatusCode::OK, Json(scenario_to_json(s, asst_name))),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Scenario not found" })),
        ),
    }
}

pub async fn create_assistant_scenario(
    Path((account_id, assistant_id)): Path<(i32, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body): Json<CreateAssistantScenarioPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let asst_name = sqlx::query_scalar::<_, String>(
        "SELECT name FROM captain_assistants WHERE id = $1 AND account_id = $2",
    )
    .bind(assistant_id)
    .bind(account_id as i64)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    let title = body.scenario.as_ref().and_then(|s| s.title.clone()).or(body.title);
    let description = body
        .scenario
        .as_ref()
        .and_then(|s| s.description.clone())
        .or(body.description);
    let instruction = body
        .scenario
        .as_ref()
        .and_then(|s| s.instruction.clone())
        .or(body.instruction);
    let tools = body
        .scenario
        .as_ref()
        .and_then(|s| s.tools.clone())
        .or(body.tools)
        .unwrap_or_else(|| json!([]));
    let enabled = body
        .scenario
        .as_ref()
        .and_then(|s| s.enabled)
        .or(body.enabled)
        .unwrap_or(true);

    let scenario = sqlx::query_as::<_, CaptainScenario>(
        r#"
        INSERT INTO captain_scenarios (account_id, assistant_id, title, description, instruction, tools, enabled, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW())
        RETURNING *
        "#,
    )
    .bind(account_id as i64)
    .bind(assistant_id)
    .bind(title)
    .bind(description)
    .bind(instruction)
    .bind(tools)
    .bind(enabled)
    .fetch_one(&state.db)
    .await;

    match scenario {
        Ok(s) => (StatusCode::OK, Json(scenario_to_json(s, asst_name))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

pub async fn update_assistant_scenario(
    Path((account_id, assistant_id, id)): Path<(i32, i64, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body): Json<CreateAssistantScenarioPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let asst_name = sqlx::query_scalar::<_, String>(
        "SELECT name FROM captain_assistants WHERE id = $1 AND account_id = $2",
    )
    .bind(assistant_id)
    .bind(account_id as i64)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    let title = body.scenario.as_ref().and_then(|s| s.title.clone()).or(body.title);
    let description = body
        .scenario
        .as_ref()
        .and_then(|s| s.description.clone())
        .or(body.description);
    let instruction = body
        .scenario
        .as_ref()
        .and_then(|s| s.instruction.clone())
        .or(body.instruction);
    let tools = body
        .scenario
        .as_ref()
        .and_then(|s| s.tools.clone())
        .or(body.tools);
    let enabled = body
        .scenario
        .as_ref()
        .and_then(|s| s.enabled)
        .or(body.enabled);

    let scenario = sqlx::query_as::<_, CaptainScenario>(
        r#"
        UPDATE captain_scenarios
        SET title = COALESCE($4, title),
            description = COALESCE($5, description),
            instruction = COALESCE($6, instruction),
            tools = COALESCE($7, tools),
            enabled = COALESCE($8, enabled),
            updated_at = NOW()
        WHERE account_id = $1 AND assistant_id = $2 AND id = $3
        RETURNING *
        "#,
    )
    .bind(account_id as i64)
    .bind(assistant_id)
    .bind(id)
    .bind(title)
    .bind(description)
    .bind(instruction)
    .bind(tools)
    .bind(enabled)
    .fetch_one(&state.db)
    .await;

    match scenario {
        Ok(s) => (StatusCode::OK, Json(scenario_to_json(s, asst_name))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

pub async fn delete_assistant_scenario(
    Path((account_id, assistant_id, id)): Path<(i32, i64, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let _ = sqlx::query(
        "DELETE FROM captain_scenarios WHERE account_id = $1 AND assistant_id = $2 AND id = $3",
    )
    .bind(account_id as i64)
    .bind(assistant_id)
    .bind(id)
    .execute(&state.db)
    .await;

    (StatusCode::NO_CONTENT, Json(json!({})))
}

// ==========================================
// Scoped Assistant Stats & Analytics
// ==========================================

pub async fn get_assistant_stats_overview(
    Path((account_id, _assistant_id)): Path<(i32, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    // Default overview metrics
    let metrics = json!({
        "conversations_handled": { "current": 0, "previous": 0, "change": 0.0 },
        "auto_resolution_rate": { "current": 0.0, "previous": 0.0, "change": 0.0 },
        "autonomous_resolutions": { "current": 0, "previous": 0, "change": 0.0 },
        "handoff_rate": { "current": 0.0, "previous": 0.0, "change": 0.0 },
        "handoff_count": { "current": 0, "previous": 0, "change": 0.0 },
        "hours_saved": { "current": 0.0, "previous": 0.0, "change": 0.0 },
        "reopen_rate": { "current": 0.0, "previous": 0.0, "change": 0.0 },
        "conversation_depth": { "current": 0.0, "previous": 0.0, "change": 0.0 },
        "durable_resolution_rate": { "current": 0.0, "previous": 0.0, "change": 0.0 },
        "autonomous_csat_score": { "current": 0.0, "previous": 0.0, "change": 0.0 },
        "assisted_csat_score": { "current": 0.0, "previous": 0.0, "change": 0.0 },
        "median_resolution_seconds": { "current": 0.0, "previous": 0.0, "change": 0.0 },
        "human_only_csat_score": { "current": 0.0, "previous": 0.0, "change": 0.0 }
    });

    (StatusCode::OK, Json(metrics))
}

pub async fn get_assistant_stats_overview_summary(
    Path((account_id, _assistant_id)): Path<(i32, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    (
        StatusCode::OK,
        Json(json!({
            "summary": "Asisten aktif dan siap menangani percakapan pelanggan secara otonom.",
            "insights": []
        })),
    )
}

pub async fn get_assistant_stats_resolution_flow(
    Path((account_id, _assistant_id)): Path<(i32, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    (
        StatusCode::OK,
        Json(json!({
            "nodes": [],
            "links": []
        })),
    )
}

pub async fn get_assistant_stats_resolution_trend(
    Path((account_id, _assistant_id)): Path<(i32, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    (
        StatusCode::OK,
        Json(json!({
            "labels": [],
            "datasets": []
        })),
    )
}

pub async fn get_assistant_stats_drilldown(
    Path((account_id, _assistant_id)): Path<(i32, i64)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    (
        StatusCode::OK,
        Json(json!({
            "records": [],
            "meta": { "total_count": 0, "page": 1 }
        })),
    )
}


