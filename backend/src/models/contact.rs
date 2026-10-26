use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Contact {
    pub id: i32,
    pub name: Option<String>,
    pub email: Option<String>,
    pub phone_number: Option<String>,
    pub account_id: i32,
    pub identifier: Option<String>,
    pub additional_attributes: Option<serde_json::Value>,
    pub custom_attributes: Option<serde_json::Value>,
    pub last_activity_at: Option<NaiveDateTime>,
    pub contact_type: Option<i32>,
    pub middle_name: Option<String>,
    pub last_name: Option<String>,
    pub location: Option<String>,
    pub country_code: Option<String>,
    pub blocked: bool,
    pub company_id: Option<i64>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ContactInbox {
    pub id: i32,
    pub contact_id: i32,
    pub inbox_id: i32,
    pub source_id: String,
    pub hmac_verified: Option<bool>,
    pub pubsub_token: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactResponse {
    pub id: i32,
    pub name: String,
    pub email: Option<String>,
    pub phone_number: Option<String>,
    pub identifier: Option<String>,
    pub thumbnail: String,
    pub additional_attributes: serde_json::Value,
    pub custom_attributes: serde_json::Value,
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub company_id: Option<i64>,
}

impl From<Contact> for ContactResponse {
    fn from(c: Contact) -> Self {
        Self {
            id: c.id,
            name: c.name.unwrap_or_else(|| "Customer".to_string()),
            email: c.email,
            phone_number: c.phone_number,
            identifier: c.identifier,
            thumbnail: "".to_string(),
            additional_attributes: c.additional_attributes.unwrap_or_else(|| serde_json::json!({})),
            custom_attributes: c.custom_attributes.unwrap_or_else(|| serde_json::json!({})),
            created_at: c.created_at.and_utc().timestamp(),
            company_id: c.company_id,
        }
    }
}
