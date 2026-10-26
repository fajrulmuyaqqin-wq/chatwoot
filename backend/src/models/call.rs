use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Call {
    pub id: i64,
    pub account_id: i64,
    pub inbox_id: i64,
    pub conversation_id: i64,
    pub contact_id: Option<i64>,
    pub message_id: Option<i64>,
    pub accepted_by_agent_id: Option<i64>,
    pub provider_call_id: String,
    pub provider: i32,  // 0 = twilio, 1 = whatsapp
    pub direction: i32, // 0 = incoming (inbound), 1 = outgoing (outbound)
    pub status: String, // ringing, in_progress, completed, no_answer, failed, rejected
    pub started_at: Option<NaiveDateTime>,
    pub duration_seconds: Option<i32>,
    pub end_reason: Option<String>,
    pub meta: serde_json::Value,
    pub ring_state: serde_json::Value,
    pub transcript: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

impl Call {
    pub fn provider_str(&self) -> &'static str {
        match self.provider {
            1 => "whatsapp",
            _ => "twilio",
        }
    }

    pub fn direction_label(&self) -> &'static str {
        match self.direction {
            1 => "outbound",
            _ => "inbound",
        }
    }

    pub fn display_status(&self) -> String {
        self.status.replace('_', "-")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallConversationInfo {
    pub id: i64,
    pub display_id: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallInboxInfo {
    pub id: i64,
    pub name: String,
    pub channel_type: String,
    pub medium: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallAgentInfo {
    pub id: i64,
    pub name: String,
    pub avatar: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallContactInfo {
    pub id: i64,
    pub name: String,
    pub phone_number: Option<String>,
    pub avatar: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallResponse {
    pub id: i64,
    pub call_id: String,
    pub provider: String,
    pub status: String,
    pub direction: String,
    pub duration_seconds: Option<i32>,
    pub end_reason: Option<String>,
    pub started_at: Option<i64>,
    pub created_at: i64,
    pub message_id: Option<i64>,
    pub recording_url: Option<String>,
    pub transcript: Option<String>,
    pub conversation: CallConversationInfo,
    pub inbox: CallInboxInfo,
    pub agent: Option<CallAgentInfo>,
    pub contact: Option<CallContactInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallListMeta {
    pub count: i64,
    pub current_page: i64,
    pub total_pages: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallListResponse {
    pub meta: CallListMeta,
    pub payload: Vec<CallResponse>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TriggerContactCallPayload {
    pub inbox_id: i64,
    pub conversation_id: Option<i32>,
}
