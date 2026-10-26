use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ChannelInstagram {
    pub id: i64,
    pub access_token: String,
    pub expires_at: NaiveDateTime,
    pub account_id: i32,
    pub instagram_id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub provider_name: Option<String>,
}
