use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Audit {
    pub id: i32,
    pub auditable_id: Option<i64>,
    pub auditable_type: Option<String>,
    pub associated_id: Option<i64>,
    pub associated_type: Option<String>,
    pub user_id: Option<i64>,
    pub user_type: Option<String>,
    pub username: Option<String>,
    pub action: Option<String>,
    pub audited_changes: Option<serde_json::Value>,
    pub version: i32,
    pub comment: Option<String>,
    pub remote_address: Option<String>,
    pub request_uuid: Option<String>,
    pub created_at: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditResponse {
    pub id: i32,
    pub action: String,
    pub user_id: Option<i64>,
    pub username: Option<String>,
    pub remote_address: Option<String>,
    pub auditable_type: Option<String>,
    pub auditable_id: Option<i64>,
    pub created_at: i64,
}

impl From<Audit> for AuditResponse {
    fn from(a: Audit) -> Self {
        Self {
            id: a.id,
            action: a.action.unwrap_or_else(|| "unknown".to_string()),
            user_id: a.user_id,
            username: a.username,
            remote_address: a.remote_address,
            auditable_type: a.auditable_type,
            auditable_id: a.auditable_id,
            created_at: a.created_at.map(|t| t.and_utc().timestamp()).unwrap_or(0),
        }
    }
}
