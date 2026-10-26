use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Campaign {
    pub id: i64,
    pub display_id: i32,
    pub title: String,
    pub description: Option<String>,
    pub message: String,
    pub sender_id: Option<i32>,
    pub enabled: Option<bool>,
    pub account_id: i64,
    pub inbox_id: i64,
    pub trigger_rules: Option<serde_json::Value>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub campaign_type: i32, // 0 = ongoing, 1 = one_off
    pub campaign_status: i32, // 0 = active, 1 = completed, 2 = processing
    pub audience: Option<serde_json::Value>,
    pub scheduled_at: Option<NaiveDateTime>,
    pub trigger_only_during_business_hours: Option<bool>,
    pub template_params: Option<serde_json::Value>,
    pub started_at: Option<NaiveDateTime>,
    pub completed_at: Option<NaiveDateTime>,
    pub whatsapp_cursor: Option<i64>,
}

pub fn campaign_type_to_str(t: i32) -> &'static str {
    match t {
        1 => "one_off",
        _ => "ongoing",
    }
}

pub fn campaign_type_from_val(v: &serde_json::Value) -> i32 {
    match v {
        serde_json::Value::Number(n) => n.as_i64().unwrap_or(0) as i32,
        serde_json::Value::String(s) => match s.as_str() {
            "one_off" => 1,
            _ => 0,
        },
        _ => 0,
    }
}

pub fn campaign_status_to_str(s: i32) -> &'static str {
    match s {
        1 => "completed",
        2 => "processing",
        _ => "active",
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CampaignResponse {
    pub id: i32,
    pub title: String,
    pub description: Option<String>,
    pub account_id: i64,
    pub inbox: Option<serde_json::Value>,
    pub sender: Option<serde_json::Value>,
    pub message: String,
    pub template_params: Option<serde_json::Value>,
    pub campaign_status: String,
    pub enabled: bool,
    pub campaign_type: String,
    pub scheduled_at: Option<i64>,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub audience: Option<serde_json::Value>,
    pub trigger_rules: Option<serde_json::Value>,
    pub trigger_only_during_business_hours: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Deserialize)]
pub struct CampaignPayloadWrapper {
    pub campaign: Option<CreateCampaignFields>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub message: Option<String>,
    pub inbox_id: Option<i64>,
    pub sender_id: Option<i32>,
    pub campaign_type: Option<serde_json::Value>,
    pub enabled: Option<bool>,
    pub trigger_only_during_business_hours: Option<bool>,
    pub scheduled_at: Option<String>,
    pub audience: Option<serde_json::Value>,
    pub trigger_rules: Option<serde_json::Value>,
    pub template_params: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct CreateCampaignFields {
    pub title: Option<String>,
    pub description: Option<String>,
    pub message: Option<String>,
    pub inbox_id: Option<i64>,
    pub sender_id: Option<i32>,
    pub campaign_type: Option<serde_json::Value>,
    pub enabled: Option<bool>,
    pub trigger_only_during_business_hours: Option<bool>,
    pub scheduled_at: Option<String>,
    pub audience: Option<serde_json::Value>,
    pub trigger_rules: Option<serde_json::Value>,
    pub template_params: Option<serde_json::Value>,
}
