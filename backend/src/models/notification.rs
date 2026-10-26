use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Notification {
    pub id: i64,
    pub account_id: i64,
    pub user_id: i64,
    pub notification_type: i32,
    pub primary_actor_type: String,
    pub primary_actor_id: i64,
    pub secondary_actor_type: Option<String>,
    pub secondary_actor_id: Option<i64>,
    pub read_at: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub snoozed_until: Option<NaiveDateTime>,
    pub last_activity_at: Option<NaiveDateTime>,
    pub meta: Option<serde_json::Value>,
}

pub fn notification_type_to_str(t: i32) -> &'static str {
    match t {
        1 => "conversation_creation",
        2 => "conversation_assignment",
        3 => "assigned_conversation_new_message",
        4 => "conversation_mention",
        5 => "participating_conversation_new_message",
        6 => "sla_missed_first_response",
        7 => "sla_missed_next_response",
        8 => "sla_missed_resolution",
        9 => "voice_call_missed",
        _ => "conversation_creation",
    }
}

pub fn notification_type_from_str(s: &str) -> i32 {
    match s {
        "conversation_creation" => 1,
        "conversation_assignment" => 2,
        "assigned_conversation_new_message" => 3,
        "conversation_mention" => 4,
        "participating_conversation_new_message" => 5,
        "sla_missed_first_response" => 6,
        "sla_missed_next_response" => 7,
        "sla_missed_resolution" => 8,
        "voice_call_missed" => 9,
        _ => 1,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationResponse {
    pub id: i64,
    pub notification_type: String,
    pub push_message_title: String,
    pub push_message_body: String,
    pub primary_actor_type: String,
    pub primary_actor_id: i64,
    pub primary_actor: Option<serde_json::Value>,
    pub read_at: Option<NaiveDateTime>,
    pub secondary_actor: Option<serde_json::Value>,
    pub user: Option<serde_json::Value>,
    pub created_at: i64,
    pub last_activity_at: i64,
    pub snoozed_until: Option<NaiveDateTime>,
    pub meta: serde_json::Value,
    pub account_id: i64,
}

#[derive(Debug, Serialize)]
pub struct NotificationMetaResponse {
    pub unread_count: i64,
    pub count: i64,
    pub current_page: i64,
}

#[derive(Debug, Serialize)]
pub struct NotificationDataWrapper {
    pub meta: NotificationMetaResponse,
    pub payload: Vec<NotificationResponse>,
}

#[derive(Debug, Serialize)]
pub struct NotificationIndexResponse {
    pub data: NotificationDataWrapper,
}
