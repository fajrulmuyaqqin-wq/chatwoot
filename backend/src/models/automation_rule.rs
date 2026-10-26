use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AutomationRule {
    pub id: i64,
    pub account_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub event_name: String,
    pub conditions: serde_json::Value,
    pub actions: serde_json::Value,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub active: bool,
    pub execution_delay: Option<i32>,
    pub monitor_id: Option<i64>,
    pub monitor_event_activated_at: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationRuleResponse {
    pub id: i64,
    pub account_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub event_name: String,
    pub conditions: serde_json::Value,
    pub actions: serde_json::Value,
    pub created_on: i64,
    pub active: bool,
    pub execution_delay: Option<i32>,
    pub monitor_id: Option<i64>,
}

impl From<AutomationRule> for AutomationRuleResponse {
    fn from(r: AutomationRule) -> Self {
        Self {
            id: r.id,
            account_id: r.account_id,
            name: r.name,
            description: r.description,
            event_name: r.event_name,
            conditions: r.conditions,
            actions: r.actions,
            created_on: r.created_at.and_utc().timestamp(),
            active: r.active,
            execution_delay: r.execution_delay,
            monitor_id: r.monitor_id,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateAutomationRulePayload {
    pub name: String,
    pub description: Option<String>,
    pub event_name: String,
    pub conditions: Option<serde_json::Value>,
    pub actions: Option<serde_json::Value>,
    pub active: Option<bool>,
    pub execution_delay: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAutomationRulePayload {
    pub name: Option<String>,
    pub description: Option<String>,
    pub event_name: Option<String>,
    pub conditions: Option<serde_json::Value>,
    pub actions: Option<serde_json::Value>,
    pub active: Option<bool>,
    pub execution_delay: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct AutomationRuleListResponse {
    pub payload: Vec<AutomationRuleResponse>,
}
