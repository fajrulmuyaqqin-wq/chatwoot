use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CsatSurveyResponse {
    pub id: i64,
    pub account_id: i64,
    pub conversation_id: i64,
    pub message_id: i64,
    pub rating: i32,
    pub feedback_message: Option<String>,
    pub contact_id: i64,
    pub assigned_agent_id: Option<i64>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub csat_review_notes: Option<String>,
    pub review_notes_updated_at: Option<NaiveDateTime>,
    pub review_notes_updated_by_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CsatSurveyResponseData {
    pub id: i64,
    pub rating: i32,
    pub feedback_message: Option<String>,
    pub csat_review_notes: Option<String>,
    pub review_notes_updated_at: Option<i64>,
    pub review_notes_updated_by: Option<serde_json::Value>,
    pub account_id: i64,
    pub message_id: i64,
    pub contact: Option<serde_json::Value>,
    pub conversation_id: i32,
    pub assigned_agent: Option<serde_json::Value>,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct CsatMetricsResponse {
    pub total_count: i64,
    pub ratings_count: serde_json::Value,
    pub total_sent_messages_count: i64,
}

#[derive(Debug, Deserialize)]
pub struct CreateCsatPayload {
    pub csat_survey_response: Option<CsatFields>,
    pub message_id: Option<i64>,
    pub rating: Option<i32>,
    pub feedback_message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CsatFields {
    pub message_id: Option<i64>,
    pub rating: Option<i32>,
    pub feedback_message: Option<String>,
}
