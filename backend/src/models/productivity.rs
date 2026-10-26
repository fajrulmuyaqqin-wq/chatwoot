use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CannedResponse {
    pub id: i32,
    pub account_id: i32,
    pub short_code: String,
    pub content: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Label {
    pub id: i64,
    pub title: String,
    pub description: Option<String>,
    pub color: String,
    pub show_on_sidebar: Option<bool>,
    pub account_id: Option<i64>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Macro {
    pub id: i64,
    pub account_id: i64,
    pub name: String,
    pub visibility: i32,
    pub created_by_id: Option<i64>,
    pub updated_by_id: Option<i64>,
    pub actions: serde_json::Value,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}
