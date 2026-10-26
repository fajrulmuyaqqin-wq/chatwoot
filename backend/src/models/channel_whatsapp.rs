use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ChannelWhatsapp {
    pub id: i32,
    pub account_id: i32,
    pub business_management_token: Option<String>,
    pub phone_number: String,
    pub provider: Option<String>,
    pub provider_config: Option<serde_json::Value>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub message_templates: Option<serde_json::Value>,
    pub message_templates_last_updated: Option<NaiveDateTime>,
    pub phone_number_health: Option<serde_json::Value>,
    pub phone_number_health_checked_at: Option<NaiveDateTime>,
    pub phone_number_health_error: Option<String>,
}

#[allow(dead_code)]
impl ChannelWhatsapp {
    pub fn phone_number_id(&self) -> Option<&str> {
        self.provider_config
            .as_ref()?
            .get("phone_number_id")?
            .as_str()
    }

    pub fn api_key(&self) -> Option<&str> {
        self.provider_config
            .as_ref()?
            .get("api_key")?
            .as_str()
    }

    pub fn webhook_verify_token(&self) -> Option<&str> {
        self.provider_config
            .as_ref()?
            .get("webhook_verify_token")?
            .as_str()
    }

    pub fn app_secret(&self) -> Option<&str> {
        self.provider_config
            .as_ref()?
            .get("app_secret")?
            .as_str()
    }
}
