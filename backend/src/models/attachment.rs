use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Attachment {
    pub id: i32,
    pub file_type: Option<i32>,
    pub external_url: Option<String>,
    pub coordinates_lat: Option<f64>,
    pub coordinates_long: Option<f64>,
    pub message_id: i32,
    pub account_id: i32,
    pub fallback_title: Option<String>,
    pub extension: Option<String>,
    pub meta: Option<serde_json::Value>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentResponse {
    pub id: i32,
    pub message_id: i32,
    pub file_type: String,
    pub account_id: i32,
    pub data_url: String,
    pub thumb_url: String,
}

impl From<Attachment> for AttachmentResponse {
    fn from(a: Attachment) -> Self {
        let url = a.external_url.unwrap_or_default();
        let file_type_str = match a.file_type {
            Some(0) => "image",
            Some(1) => "audio",
            Some(2) => "video",
            Some(4) => "location",
            _ => "file",
        };

        Self {
            id: a.id,
            message_id: a.message_id,
            file_type: file_type_str.to_string(),
            account_id: a.account_id,
            data_url: url.clone(),
            thumb_url: url,
        }
    }
}
