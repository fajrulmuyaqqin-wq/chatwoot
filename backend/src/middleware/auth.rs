use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
    Json,
};
use chrono::Utc;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::models::user::User;
use crate::state::AppState;

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct CurrentUser {
    pub id: i32,
    pub email: String,
    pub uid: String,
    pub pubsub_token: Option<String>,
    pub user_type: Option<String>,
}

#[allow(dead_code)]
impl CurrentUser {
    pub fn is_super_admin(&self) -> bool {
        self.user_type.as_deref() == Some("SuperAdmin")
    }
}

pub async fn require_auth(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut request: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let access_token = headers.get("access-token").and_then(|h| h.to_str().ok());
    let client_id = headers.get("client").and_then(|h| h.to_str().ok());
    let uid = headers.get("uid").and_then(|h| h.to_str().ok());

    let (token, client, uid_str) = match (access_token, client_id, uid) {
        (Some(t), Some(c), Some(u)) => (t, c, u.trim().to_lowercase()),
        _ => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({ "errors": ["You need to sign in or sign up before continuing."] })),
            ));
        }
    };

    let user = match sqlx::query_as::<_, User>(
        r#"
        SELECT id, provider, uid, encrypted_password, email, name, display_name,
               pubsub_token, tokens, ui_settings, custom_attributes, availability,
               sign_in_count, type, created_at, updated_at
        FROM users
        WHERE LOWER(email) = $1 OR uid = $1
        LIMIT 1
        "#,
    )
    .bind(&uid_str)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(u)) => u,
        _ => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({ "errors": ["User not found or invalid credentials."] })),
            ));
        }
    };

    // Validasi token hash dan expiry
    let tokens_map = user.tokens.clone().unwrap_or_else(|| json!({}));
    let is_valid = if let Some(info) = tokens_map.get(client) {
        let expiry = info.get("expiry").and_then(|e| e.as_i64()).unwrap_or(0);
        let stored_hash = info.get("token").and_then(|t| t.as_str()).unwrap_or("");
        let expected_hash = hash_token(token);

        expiry > Utc::now().timestamp()
            && (stored_hash == expected_hash || stored_hash == token)
    } else {
        false
    };

    if !is_valid {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "errors": ["Session expired or invalid token."] })),
        ));
    }

    let current_user = CurrentUser {
        id: user.id,
        email: user.email.unwrap_or_else(|| uid_str.clone()),
        uid: user.uid,
        pubsub_token: user.pubsub_token,
        user_type: user.r#type,
    };

    request.extensions_mut().insert(current_user);

    Ok(next.run(request).await)
}

fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}
