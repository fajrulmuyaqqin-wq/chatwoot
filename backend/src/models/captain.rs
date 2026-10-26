use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CaptainAssistant {
    pub id: i64,
    pub name: String,
    pub account_id: i64,
    pub description: Option<String>,
    pub config: JsonValue,
    pub response_guidelines: Option<JsonValue>,
    pub guardrails: Option<JsonValue>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptainAssistantPayload {
    pub name: String,
    pub description: Option<String>,
    pub config: Option<JsonValue>,
    pub response_guidelines: Option<JsonValue>,
    pub guardrails: Option<JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CaptainDocument {
    pub id: i64,
    pub name: Option<String>,
    pub external_link: Option<String>,
    pub content: Option<String>,
    pub content_fingerprint: Option<String>,
    pub last_sync_attempted_at: Option<NaiveDateTime>,
    pub last_sync_error_code: Option<String>,
    pub last_synced_at: Option<NaiveDateTime>,
    pub metadata: Option<JsonValue>,
    pub status: i32,
    pub sync_status: Option<i32>,
    pub assistant_id: i64,
    pub account_id: i64,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptainDocumentPayload {
    pub name: Option<String>,
    pub external_link: Option<String>,
    pub content: Option<String>,
    pub assistant_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CaptainAssistantResponseRecord {
    pub id: i64,
    pub question: String,
    pub answer: String,
    pub status: i32,
    pub edited: bool,
    pub assistant_id: i64,
    pub account_id: i64,
    pub document_id: Option<i64>,
    pub documentable_id: Option<i64>,
    pub documentable_type: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptainAssistantResponsePayload {
    pub question: String,
    pub answer: String,
    pub document_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CaptainInbox {
    pub id: i64,
    pub captain_assistant_id: i64,
    pub inbox_id: i64,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CaptainFaqSuggestion {
    pub id: i64,
    pub question: String,
    pub answer: String,
    pub assistant_id: i64,
    pub account_id: i64,
    pub language: String,
    pub source_count: i32,
    pub status: i32,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CaptainCustomTool {
    pub id: i64,
    pub account_id: i64,
    pub assistant_id: Option<i64>,
    pub slug: String,
    pub title: String,
    pub description: Option<String>,
    pub http_method: String,
    pub endpoint_url: String,
    pub request_template: Option<String>,
    pub response_template: Option<String>,
    pub auth_type: Option<String>,
    pub auth_config: Option<JsonValue>,
    pub param_schema: Option<JsonValue>,
    pub headers: Option<JsonValue>,
    pub settings: Option<JsonValue>,
    pub enabled: bool,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CaptainScenario {
    pub id: i64,
    pub account_id: i64,
    pub assistant_id: i64,
    pub title: Option<String>,
    pub description: Option<String>,
    pub instruction: Option<String>,
    pub tools: Option<JsonValue>,
    pub enabled: bool,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CopilotThread {
    pub id: i64,
    pub title: String,
    pub user_id: i64,
    pub account_id: i64,
    pub conversation_id: Option<i64>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CopilotMessage {
    pub id: i64,
    pub copilot_thread_id: i64,
    pub user_id: Option<i64>,
    pub account_id: i64,
    pub message_type: i32,
    pub message: JsonValue,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}
