use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct Webhook {
    pub id: i64,
    pub account_id: Option<i32>,
    pub inbox_id: Option<i32>,
    pub url: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub webhook_type: Option<i32>,
    pub subscriptions: Option<serde_json::Value>,
    pub name: Option<String>,
    pub secret: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateWebhookPayload {
    pub inbox_id: Option<i32>,
    pub name: Option<String>,
    pub url: String,
    pub subscriptions: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWebhookPayload {
    pub inbox_id: Option<i32>,
    pub name: Option<String>,
    pub url: Option<String>,
    pub subscriptions: Option<Vec<String>>,
}
