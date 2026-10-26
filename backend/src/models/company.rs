use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::models::conversation::ConversationResponse;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Company {
    pub id: i64,
    pub name: String,
    pub domain: Option<String>,
    pub description: Option<String>,
    pub account_id: i64,
    pub contacts_count: i32,
    pub additional_attributes: serde_json::Value,
    pub custom_attributes: serde_json::Value,
    pub last_activity_at: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyResponse {
    pub id: i64,
    pub name: String,
    pub domain: Option<String>,
    pub description: Option<String>,
    pub contacts_count: i32,
    pub additional_attributes: serde_json::Value,
    pub custom_attributes: serde_json::Value,
    pub avatar_url: String,
    pub last_activity_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<Company> for CompanyResponse {
    fn from(c: Company) -> Self {
        Self {
            id: c.id,
            name: c.name,
            domain: c.domain,
            description: c.description,
            contacts_count: c.contacts_count,
            additional_attributes: c.additional_attributes,
            custom_attributes: c.custom_attributes,
            avatar_url: "".to_string(),
            last_activity_at: c.last_activity_at.map(|dt| dt.and_utc().timestamp()),
            created_at: c.created_at.and_utc().timestamp(),
            updated_at: c.updated_at.and_utc().timestamp(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyListMeta {
    pub total_count: i64,
    pub page: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyListResponse {
    pub meta: CompanyListMeta,
    pub payload: Vec<CompanyResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyContactResponse {
    pub id: i32,
    pub name: String,
    pub email: Option<String>,
    pub phone_number: Option<String>,
    pub identifier: Option<String>,
    pub thumbnail: String,
    pub additional_attributes: serde_json::Value,
    pub custom_attributes: serde_json::Value,
    pub created_at: i64,
    pub company_id: Option<i64>,
    pub linked_to_current_company: bool,
    pub company: Option<CompanyResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyContactListResponse {
    pub meta: CompanyListMeta,
    pub payload: Vec<CompanyContactResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyConversationListMeta {
    pub total_count: i64,
    pub all_count: i64,
    pub open_count: i64,
    pub page: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanyConversationListResponse {
    pub meta: CompanyConversationListMeta,
    pub payload: Vec<ConversationResponse>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CompanyFields {
    pub name: Option<String>,
    pub domain: Option<String>,
    pub description: Option<String>,
    pub additional_attributes: Option<serde_json::Value>,
    pub custom_attributes: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompanyPayloadWrapper {
    pub company: Option<CompanyFields>,
    #[serde(flatten)]
    pub direct: CompanyFields,
}

impl CompanyPayloadWrapper {
    pub fn into_fields(self) -> CompanyFields {
        if let Some(comp) = self.company {
            comp
        } else {
            self.direct
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AttachContactPayload {
    pub contact_id: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DestroyCustomAttributesPayload {
    pub custom_attributes: Vec<String>,
}
