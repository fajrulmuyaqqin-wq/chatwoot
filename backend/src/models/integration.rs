use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct IntegrationHook {
    pub id: i64,
    pub status: Option<i32>, // 0 = disabled, 1 = enabled
    pub inbox_id: Option<i32>,
    pub account_id: i32,
    pub app_id: String,
    pub hook_type: Option<i32>, // 0 = account, 1 = inbox
    pub reference_id: Option<String>,
    pub access_token: Option<String>,
    pub settings: Option<serde_json::Value>,
    pub created_at: Option<NaiveDateTime>,
    pub updated_at: Option<NaiveDateTime>,
}
