use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SlaPolicy {
    pub id: i32,
    pub name: String,
    pub first_response_time_threshold: Option<f64>,
    pub next_response_time_threshold: Option<f64>,
    pub only_during_business_hours: bool,
    pub account_id: i64,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub description: Option<String>,
    pub resolution_time_threshold: Option<f64>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AppliedSla {
    pub id: i32,
    pub account_id: i64,
    pub sla_policy_id: i64,
    pub conversation_id: i64,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub sla_status: i32, // 0 = active / hit, 1 = missed / breached
    pub completed_at: Option<NaiveDateTime>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SlaEvent {
    pub id: i32,
    pub applied_sla_id: i64,
    pub conversation_id: i64,
    pub account_id: i64,
    pub sla_policy_id: i64,
    pub inbox_id: i64,
    pub event_type: Option<i32>,
    pub meta: Option<serde_json::Value>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}
