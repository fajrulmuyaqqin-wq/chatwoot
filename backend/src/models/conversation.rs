use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::models::contact::ContactResponse;
use crate::models::message::MessageResponse;

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Conversation {
    pub id: i32,
    pub account_id: i32,
    pub inbox_id: i32,
    pub status: i32, // 0 = open, 1 = resolved, 2 = pending, 3 = snoozed
    pub assignee_id: Option<i32>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub contact_id: Option<i64>,
    pub display_id: i32,
    pub contact_last_seen_at: Option<NaiveDateTime>,
    pub agent_last_seen_at: Option<NaiveDateTime>,
    pub additional_attributes: Option<serde_json::Value>,
    pub contact_inbox_id: Option<i64>,
    pub uuid: Uuid,
    pub identifier: Option<String>,
    pub last_activity_at: NaiveDateTime,
    pub team_id: Option<i64>,
    pub campaign_id: Option<i64>,
    pub snoozed_until: Option<NaiveDateTime>,
    pub custom_attributes: Option<serde_json::Value>,
    pub assignee_last_seen_at: Option<NaiveDateTime>,
    pub first_reply_created_at: Option<NaiveDateTime>,
    pub priority: Option<i32>,
    pub sla_policy_id: Option<i64>,
    pub waiting_since: Option<NaiveDateTime>,
    pub cached_label_list: Option<String>,
    pub assignee_agent_bot_id: Option<i64>,
    pub ai_assignee_type: Option<String>,
    pub status_changed_at: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMeta {
    pub sender: Option<ContactResponse>,
    pub channel: String,
    pub assignee: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationResponse {
    pub id: i32, // display_id
    pub account_id: i32,
    pub inbox_id: i32,
    pub status: String, // "open", "resolved", "pending", "snoozed"
    pub assignee_id: Option<i32>,
    pub created_at: i64,
    pub updated_at: f64,
    pub last_activity_at: i64,
    pub meta: ConversationMeta,
    pub messages: Vec<MessageResponse>,
    pub unread_count: i64,
    pub priority: Option<i32>,
    pub team_id: Option<i64>,
    pub snoozed_until: Option<i64>,
    pub custom_attributes: Option<serde_json::Value>,
}

impl Conversation {
    pub fn status_string(&self) -> &'static str {
        match self.status {
            0 => "open",
            1 => "resolved",
            2 => "pending",
            3 => "snoozed",
            _ => "open",
        }
    }

    pub fn to_response(
        &self,
        sender: Option<ContactResponse>,
        channel: String,
        assignee: Option<serde_json::Value>,
        last_message: Option<MessageResponse>,
        unread_count: i64,
    ) -> ConversationResponse {
        ConversationResponse {
            id: self.display_id,
            account_id: self.account_id,
            inbox_id: self.inbox_id,
            status: self.status_string().to_string(),
            assignee_id: self.assignee_id,
            created_at: self.created_at.and_utc().timestamp(),
            updated_at: self.updated_at.and_utc().timestamp() as f64,
            last_activity_at: self.last_activity_at.and_utc().timestamp(),
            meta: ConversationMeta {
                sender,
                channel,
                assignee,
            },
            messages: last_message.into_iter().collect(),
            unread_count,
            priority: self.priority,
            team_id: self.team_id,
            snoozed_until: self.snoozed_until.map(|s| s.and_utc().timestamp()),
            custom_attributes: self.custom_attributes.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationCounts {
    pub mine_count: i64,
    pub unassigned_count: i64,
    pub assigned_count: i64,
    pub all_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationListPayload {
    pub meta: ConversationCounts,
    pub payload: Vec<ConversationResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationListResponse {
    pub data: ConversationListPayload,
}
