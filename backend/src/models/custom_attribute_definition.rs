use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct CustomAttributeDefinition {
    pub id: i64,
    pub attribute_display_name: Option<String>,
    pub attribute_key: Option<String>,
    pub attribute_display_type: Option<i32>,
    pub default_value: Option<i32>,
    pub attribute_model: Option<i32>,
    pub account_id: Option<i64>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub attribute_description: Option<String>,
    pub attribute_values: Option<serde_json::Value>,
    pub regex_pattern: Option<String>,
    pub regex_cue: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CustomAttributePayload {
    pub attribute_display_name: String,
    pub attribute_key: String,
    pub attribute_display_type: Option<i32>,
    pub attribute_model: Option<i32>,
    pub attribute_description: Option<String>,
    pub attribute_values: Option<serde_json::Value>,
    pub regex_pattern: Option<String>,
    pub regex_cue: Option<String>,
}
