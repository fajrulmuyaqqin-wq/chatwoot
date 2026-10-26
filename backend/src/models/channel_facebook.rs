use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ChannelFacebookPage {
    pub id: i32,
    pub page_id: String,
    pub user_access_token: String,
    pub page_access_token: String,
    pub account_id: i32,
    pub instagram_id: Option<String>,
    pub provider_name: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}
