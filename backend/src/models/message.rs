use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::models::attachment::AttachmentResponse;

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Message {
    pub id: i32,
    pub content: Option<String>,
    pub account_id: i32,
    pub inbox_id: i32,
    pub conversation_id: i32,
    pub message_type: i32, // 0 = incoming, 1 = outgoing, 2 = activity, 3 = template
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub private: bool,
    pub status: Option<i32>,
    pub source_id: Option<String>,
    pub content_type: i32,
    pub content_attributes: Option<serde_json::Value>,
    pub sender_type: Option<String>,
    pub sender_id: Option<i64>,
    pub external_source_ids: Option<serde_json::Value>,
    pub additional_attributes: Option<serde_json::Value>,
    pub processed_message_content: Option<String>,
    pub sentiment: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageSender {
    pub id: i64,
    pub name: String,
    pub avatar_url: String,
    pub r#type: String, // "user" or "contact"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageResponse {
    pub id: i32,
    pub content: Option<String>,
    pub account_id: i32,
    pub inbox_id: i32,
    pub conversation_id: i32, // display_id percakapan
    pub message_type: i32,
    pub content_type: String,
    pub status: String,
    pub content_attributes: serde_json::Value,
    pub created_at: i64,
    pub private: bool,
    pub source_id: Option<String>,
    pub sender: Option<MessageSender>,
    pub attachments: Vec<AttachmentResponse>,
}

impl Message {
    pub fn to_response(
        &self,
        conversation_display_id: i32,
        sender: Option<MessageSender>,
        attachments: Vec<AttachmentResponse>,
    ) -> MessageResponse {
        let status_str = match self.status {
            Some(1) => "delivered",
            Some(2) => "read",
            Some(3) => "failed",
            _ => "sent",
        };

        MessageResponse {
            id: self.id,
            content: self.content.clone(),
            account_id: self.account_id,
            inbox_id: self.inbox_id,
            conversation_id: conversation_display_id,
            message_type: self.message_type,
            content_type: "text".to_string(),
            status: status_str.to_string(),
            content_attributes: self
                .content_attributes
                .clone()
                .unwrap_or_else(|| serde_json::json!({})),
            created_at: self.created_at.and_utc().timestamp(),
            private: self.private,
            source_id: self.source_id.clone(),
            sender,
            attachments,
        }
    }
}
