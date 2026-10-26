use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::middleware::auth::CurrentUser;
use crate::models::user::{AuthResponseUser, User};
use crate::routes::auth::fetch_user_accounts_pub;
use crate::state::AppState;

#[derive(Debug, Deserialize, Default)]
pub struct ProfilePayload {
    pub profile: Option<ProfileInnerPayload>,
    pub account_id: Option<i64>,
    pub availability: Option<i32>,
    pub auto_offline: Option<bool>,
    pub ui_settings: Option<serde_json::Value>,
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ProfileInnerPayload {
    pub account_id: Option<i64>,
    pub availability: Option<i32>,
    pub auto_offline: Option<bool>,
    pub ui_settings: Option<serde_json::Value>,
    pub display_name: Option<String>,
}

impl ProfilePayload {
    pub fn get_account_id(&self) -> Option<i64> {
        self.profile
            .as_ref()
            .and_then(|p| p.account_id)
            .or(self.account_id)
    }
    pub fn get_availability(&self) -> Option<i32> {
        self.profile
            .as_ref()
            .and_then(|p| p.availability)
            .or(self.availability)
    }
    pub fn get_auto_offline(&self) -> Option<bool> {
        self.profile
            .as_ref()
            .and_then(|p| p.auto_offline)
            .or(self.auto_offline)
    }
    pub fn get_ui_settings(&self) -> Option<&serde_json::Value> {
        self.profile
            .as_ref()
            .and_then(|p| p.ui_settings.as_ref())
            .or(self.ui_settings.as_ref())
    }
    pub fn get_display_name(&self) -> Option<&String> {
        self.profile
            .as_ref()
            .and_then(|p| p.display_name.as_ref())
            .or(self.display_name.as_ref())
    }
}

pub async fn get_profile(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
) -> impl IntoResponse {
    let user_opt = sqlx::query_as::<_, User>(
        r#"
        SELECT id, provider, uid, encrypted_password, email, name, display_name,
               pubsub_token, tokens, ui_settings, custom_attributes, availability,
               sign_in_count, type, created_at, updated_at
        FROM users
        WHERE id = $1
        LIMIT 1
        "#,
    )
    .bind(current_user.id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    let user = match user_opt {
        Some(u) => u,
        None => return (StatusCode::NOT_FOUND, Json(json!({ "error": "User not found" }))),
    };

    let accounts_data = fetch_user_accounts_pub(&state.db, current_user.id as i64).await;
    let primary_account_id = accounts_data.first().map(|a| a.id);
    let primary_role = accounts_data
        .first()
        .map(|a| a.role.clone())
        .unwrap_or_else(|| "agent".to_string());

    let auth_user = AuthResponseUser {
        id: user.id,
        email: user.email.unwrap_or_else(|| current_user.email.clone()),
        name: user.name,
        display_name: user.display_name,
        provider: user.provider,
        uid: user.uid,
        pubsub_token: user.pubsub_token,
        account_id: primary_account_id,
        role: primary_role,
        ui_settings: user.ui_settings.unwrap_or_else(|| json!({})),
        accounts: accounts_data,
    };

    (StatusCode::OK, Json(json!(auth_user)))
}

pub async fn update_profile(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(payload): Json<ProfilePayload>,
) -> impl IntoResponse {
    if let Some(ui_settings) = payload.get_ui_settings() {
        let _ = sqlx::query(
            "UPDATE users SET ui_settings = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(ui_settings)
        .bind(current_user.id)
        .execute(&state.db)
        .await;
    }

    if let Some(display_name) = payload.get_display_name() {
        let _ = sqlx::query(
            "UPDATE users SET display_name = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(display_name)
        .bind(current_user.id)
        .execute(&state.db)
        .await;
    }

    get_profile(State(state), Extension(current_user)).await
}

pub async fn set_active_account(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(payload): Json<ProfilePayload>,
) -> impl IntoResponse {
    if let Some(acc_id) = payload.get_account_id() {
        let _ = sqlx::query(
            "UPDATE account_users SET active_at = NOW(), updated_at = NOW() WHERE user_id = $1 AND account_id = $2",
        )
        .bind(current_user.id as i64)
        .bind(acc_id)
        .execute(&state.db)
        .await;
    }

    (StatusCode::OK, Json(json!({ "success": true })))
}

pub async fn update_availability(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(payload): Json<ProfilePayload>,
) -> impl IntoResponse {
    if let (Some(acc_id), Some(avail)) = (payload.get_account_id(), payload.get_availability()) {
        let _ = sqlx::query(
            "UPDATE account_users SET availability = $1, updated_at = NOW() WHERE user_id = $2 AND account_id = $3",
        )
        .bind(avail)
        .bind(current_user.id as i64)
        .bind(acc_id)
        .execute(&state.db)
        .await;
    }

    get_profile(State(state), Extension(current_user)).await
}

pub async fn update_auto_offline(
    State(state): State<AppState>,
    Extension(current_user): Extension<CurrentUser>,
    Json(payload): Json<ProfilePayload>,
) -> impl IntoResponse {
    if let (Some(acc_id), Some(auto_off)) = (payload.get_account_id(), payload.get_auto_offline()) {
        let _ = sqlx::query(
            "UPDATE account_users SET auto_offline = $1, updated_at = NOW() WHERE user_id = $2 AND account_id = $3",
        )
        .bind(auto_off)
        .bind(current_user.id as i64)
        .bind(acc_id)
        .execute(&state.db)
        .await;
    }

    get_profile(State(state), Extension(current_user)).await
}

pub async fn dummy_ok() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({ "success": true })))
}
