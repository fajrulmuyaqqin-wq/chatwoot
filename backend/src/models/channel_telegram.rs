use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ChannelTelegram {
    pub id: i64,
    pub bot_name: Option<String>,
    pub account_id: i32,
    pub bot_token: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}
