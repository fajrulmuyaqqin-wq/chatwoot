use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ChannelEmail {
    pub id: i32,
    pub account_id: i32,
    pub email: String,
    pub forward_to_email: String,
    pub imap_enabled: Option<bool>,
    pub imap_address: Option<String>,
    pub imap_port: Option<i32>,
    pub imap_login: Option<String>,
    pub imap_password: Option<String>,
    pub imap_enable_ssl: Option<bool>,
    pub smtp_enabled: Option<bool>,
    pub smtp_address: Option<String>,
    pub smtp_port: Option<i32>,
    pub smtp_login: Option<String>,
    pub smtp_password: Option<String>,
    pub smtp_domain: Option<String>,
    pub smtp_enable_starttls_auto: Option<bool>,
    pub smtp_authentication: Option<String>,
    pub smtp_openssl_verify_mode: Option<String>,
    pub smtp_enable_ssl_tls: Option<bool>,
    pub provider_config: Option<serde_json::Value>,
    pub provider: Option<String>,
    pub imap_authentication: Option<String>,
    pub verified_for_sending: bool,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}
