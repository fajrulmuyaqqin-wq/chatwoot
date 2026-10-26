use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Inbox {
    pub id: i32,
    pub channel_id: i32,
    pub account_id: i32,
    pub name: String,
    pub channel_type: Option<String>,
    pub enable_auto_assignment: Option<bool>,
    pub greeting_enabled: Option<bool>,
    pub greeting_message: Option<String>,
    pub email_address: Option<String>,
    pub working_hours_enabled: Option<bool>,
    pub out_of_office_message: Option<String>,
    pub timezone: Option<String>,
    pub enable_email_collect: Option<bool>,
    pub csat_survey_enabled: Option<bool>,
    pub allow_messages_after_resolved: Option<bool>,
    pub auto_assignment_config: Option<serde_json::Value>,
    pub lock_to_single_conversation: bool,
    pub portal_id: Option<i64>,
    pub sender_name_type: i32,
    pub business_name: Option<String>,
    pub csat_config: Option<serde_json::Value>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// Format JSON inbox untuk frontend Chatwoot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboxResponse {
    pub id: i32,
    pub channel_id: i32,
    pub name: String,
    pub channel_type: String,
    pub avatar_url: String,
    pub greeting_enabled: bool,
    pub greeting_message: Option<String>,
    pub working_hours_enabled: bool,
    pub enable_email_collect: bool,
    pub csat_survey_enabled: bool,
    pub enable_auto_assignment: bool,
    pub allow_messages_after_resolved: bool,
    pub lock_to_single_conversation: bool,
    pub timezone: String,
}

impl From<Inbox> for InboxResponse {
    fn from(i: Inbox) -> Self {
        Self {
            id: i.id,
            channel_id: i.channel_id,
            name: i.name,
            channel_type: i.channel_type.unwrap_or_else(|| "Channel::Api".to_string()),
            avatar_url: "".to_string(),
            greeting_enabled: i.greeting_enabled.unwrap_or(false),
            greeting_message: i.greeting_message,
            working_hours_enabled: i.working_hours_enabled.unwrap_or(false),
            enable_email_collect: i.enable_email_collect.unwrap_or(true),
            csat_survey_enabled: i.csat_survey_enabled.unwrap_or(false),
            enable_auto_assignment: i.enable_auto_assignment.unwrap_or(true),
            allow_messages_after_resolved: i.allow_messages_after_resolved.unwrap_or(true),
            lock_to_single_conversation: i.lock_to_single_conversation,
            timezone: i.timezone.unwrap_or_else(|| "UTC".to_string()),
        }
    }
}
