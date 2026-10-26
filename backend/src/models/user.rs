use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: i32,
    pub provider: String,
    pub uid: String,
    pub encrypted_password: String,
    pub email: Option<String>,
    pub name: String,
    pub display_name: Option<String>,
    pub pubsub_token: Option<String>,
    pub tokens: Option<serde_json::Value>,
    pub ui_settings: Option<serde_json::Value>,
    pub custom_attributes: Option<serde_json::Value>,
    pub availability: Option<i32>,
    pub sign_in_count: i32,
    pub r#type: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[allow(dead_code)]
impl User {
    pub fn is_super_admin(&self) -> bool {
        self.r#type.as_deref() == Some("SuperAdmin")
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Account {
    pub id: i32,
    pub name: String,
    pub status: Option<i32>,
    pub locale: Option<i32>,
    pub domain: Option<String>,
    pub support_email: Option<String>,
    pub settings: Option<serde_json::Value>,
    pub custom_attributes: Option<serde_json::Value>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AccountUser {
    pub id: i64,
    pub account_id: Option<i64>,
    pub user_id: Option<i64>,
    pub role: Option<i32>, // 0 = agent, 1 = administrator
    pub availability: i32,
    pub auto_offline: bool,
    pub active_at: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[allow(dead_code)]
impl AccountUser {
    pub fn role_name(&self) -> &'static str {
        match self.role {
            Some(1) => "administrator",
            _ => "agent",
        }
    }
}

/// Representasi data user yang dikembalikan ke Vue frontend pada /auth/sign_in dan /auth/validate_token
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResponseAccount {
    pub id: i64,
    pub name: String,
    pub status: String,
    pub role: String,
    pub permissions: Vec<String>,
    pub active_at: Option<NaiveDateTime>,
    pub availability: i32,
    pub availability_status: String,
    pub auto_offline: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResponseUser {
    pub id: i32,
    pub email: String,
    pub name: String,
    pub display_name: Option<String>,
    pub provider: String,
    pub uid: String,
    pub pubsub_token: Option<String>,
    pub account_id: Option<i64>,
    pub role: String,
    pub ui_settings: serde_json::Value,
    pub accounts: Vec<AuthResponseAccount>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthSuccessResponse {
    pub data: AuthResponseUser,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_response_serialization() {
        let response = AuthSuccessResponse {
            data: AuthResponseUser {
                id: 1,
                email: "admin@example.com".to_string(),
                name: "Administrator".to_string(),
                display_name: Some("Admin".to_string()),
                provider: "email".to_string(),
                uid: "admin@example.com".to_string(),
                pubsub_token: Some("sample_pubsub_token_123".to_string()),
                account_id: Some(1),
                role: "administrator".to_string(),
                ui_settings: serde_json::json!({ "theme": "dark" }),
                accounts: vec![AuthResponseAccount {
                    id: 1,
                    name: "Default Account".to_string(),
                    status: "active".to_string(),
                    role: "administrator".to_string(),
                    permissions: vec!["administrator".to_string()],
                    active_at: None,
                    availability: 0,
                    availability_status: "online".to_string(),
                    auto_offline: true,
                }],
            },
        };

        let json = serde_json::to_string(&response).expect("Harus dapat diserialisasi ke JSON");
        assert!(json.contains("\"pubsub_token\":\"sample_pubsub_token_123\""));
        assert!(json.contains("\"role\":\"administrator\""));
        assert!(json.contains("\"accounts\":["));
    }
}
