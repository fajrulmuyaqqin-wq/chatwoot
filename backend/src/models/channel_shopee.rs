use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ChannelShopee {
    pub id: i64,
    pub account_id: i32,
    pub shop_id: i64,
    pub partner_id: i64,
    pub partner_key: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub shop_name: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}
