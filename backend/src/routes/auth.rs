use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::Utc;
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use tracing::warn;

use crate::models::user::{
    AuthResponseAccount, AuthResponseUser, AuthSuccessResponse, User,
};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct SignInPayload {
    pub email: Option<String>,
    pub password: Option<String>,
}

pub async fn sign_in(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Option<Json<SignInPayload>>,
) -> impl IntoResponse {
    // Ambil email & password dari JSON payload atau fallback ke request headers
    let (email, password) = if let Some(Json(body)) = payload {
        (
            body.email.or_else(|| {
                headers
                    .get("email")
                    .and_then(|v| v.to_str().ok().map(|s| s.to_string()))
            }),
            body.password.or_else(|| {
                headers
                    .get("password")
                    .and_then(|v| v.to_str().ok().map(|s| s.to_string()))
            }),
        )
    } else {
        (
            headers
                .get("email")
                .and_then(|v| v.to_str().ok().map(|s| s.to_string())),
            headers
                .get("password")
                .and_then(|v| v.to_str().ok().map(|s| s.to_string())),
        )
    };

    let email = match email {
        Some(e) if !e.trim().is_empty() => e.trim().to_lowercase(),
        _ => {
            return (
                StatusCode::UNAUTHORIZED,
                HeaderMap::new(),
                Json(json!({ "errors": ["Email is required."] })),
            );
        }
    };

    let password = match password {
        Some(p) if !p.is_empty() => p,
        _ => {
            return (
                StatusCode::UNAUTHORIZED,
                HeaderMap::new(),
                Json(json!({ "errors": ["Password is required."] })),
            );
        }
    };

    // Cari user di database
    let user_opt = match sqlx::query_as::<_, User>(
        r#"
        SELECT id, provider, uid, encrypted_password, email, name, display_name,
               pubsub_token, tokens, ui_settings, custom_attributes, availability,
               sign_in_count, type, created_at, updated_at
        FROM users
        WHERE LOWER(email) = $1 OR uid = $1
        LIMIT 1
        "#,
    )
    .bind(&email)
    .fetch_optional(&state.db)
    .await
    {
        Ok(u) => u,
        Err(err) => {
            warn!("Database error saat query user {}: {}", email, err);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                HeaderMap::new(),
                Json(json!({ "errors": ["Database connection error."] })),
            );
        }
    };

    let user = match user_opt {
        Some(u) => u,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                HeaderMap::new(),
                Json(json!({ "errors": ["Invalid login credentials. Please try again."] })),
            );
        }
    };

    // Verifikasi password hash bcrypt
    let password_valid = bcrypt::verify(&password, &user.encrypted_password).unwrap_or(false);
    if !password_valid {
        return (
            StatusCode::UNAUTHORIZED,
            HeaderMap::new(),
            Json(json!({ "errors": ["Invalid login credentials. Please try again."] })),
        );
    }

    // Generate token Devise Token Auth
    let client_id: String = thread_rng()
        .sample_iter(&Alphanumeric)
        .take(24)
        .map(char::from)
        .collect();

    let raw_token: String = thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();

    let expiry = Utc::now().timestamp() + (60 * 24 * 3600); // 60 hari
    let token_hash = hash_token(&raw_token);

    // Update tokens jsonb di database users
    let mut tokens_map = user.tokens.unwrap_or_else(|| json!({}));
    if !tokens_map.is_object() {
        tokens_map = json!({});
    }

    tokens_map[client_id.clone()] = json!({
        "token": token_hash,
        "expiry": expiry,
        "updated_at": Utc::now().to_rfc3339()
    });

    let _ = sqlx::query(
        r#"
        UPDATE users
        SET tokens = $1, last_sign_in_at = NOW(), sign_in_count = sign_in_count + 1
        WHERE id = $2
        "#,
    )
    .bind(&tokens_map)
    .bind(user.id)
    .execute(&state.db)
    .await;

    // Ambil akun yang dimiliki user
    let accounts_data = fetch_user_accounts(&state.db, user.id as i64).await;
    let primary_account_id = accounts_data.first().map(|a| a.id);
    let primary_role = accounts_data
        .first()
        .map(|a| a.role.clone())
        .unwrap_or_else(|| "agent".to_string());

    let auth_user = AuthResponseUser {
        id: user.id,
        email: user.email.unwrap_or_else(|| email.clone()),
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

    let response_body = AuthSuccessResponse { data: auth_user };

    // Pasang 5 header Devise Token Auth
    let mut resp_headers = HeaderMap::new();
    if let Ok(v) = HeaderValue::from_str(&raw_token) {
        resp_headers.insert("access-token", v);
    }
    resp_headers.insert("token-type", HeaderValue::from_static("Bearer"));
    if let Ok(v) = HeaderValue::from_str(&client_id) {
        resp_headers.insert("client", v);
    }
    if let Ok(v) = HeaderValue::from_str(&expiry.to_string()) {
        resp_headers.insert("expiry", v);
    }
    if let Ok(v) = HeaderValue::from_str(&email) {
        resp_headers.insert("uid", v);
    }

    (
        StatusCode::OK,
        resp_headers,
        Json(serde_json::to_value(&response_body).unwrap_or(json!({})),
    ))
}

pub async fn validate_token(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let access_token = headers.get("access-token").and_then(|h| h.to_str().ok());
    let client_id = headers.get("client").and_then(|h| h.to_str().ok());
    let uid = headers.get("uid").and_then(|h| h.to_str().ok());

    let (token, client, uid_str) = match (access_token, client_id, uid) {
        (Some(t), Some(c), Some(u)) => (t, c, u.trim().to_lowercase()),
        _ => {
            return (
                StatusCode::UNAUTHORIZED,
                HeaderMap::new(),
                Json(json!({ "success": false, "errors": ["Missing auth headers"] })),
            );
        }
    };

    let user_opt = match sqlx::query_as::<_, User>(
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
        Ok(u) => u,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                HeaderMap::new(),
                Json(json!({ "success": false, "errors": ["Database error"] })),
            );
        }
    };

    let user = match user_opt {
        Some(u) => u,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                HeaderMap::new(),
                Json(json!({ "success": false, "errors": ["User not found"] })),
            );
        }
    };

    // Validasi token di tokens JSONB
    let tokens_map = user.tokens.clone().unwrap_or_else(|| json!({}));
    let client_info = tokens_map.get(client);

    let is_valid = if let Some(info) = client_info {
        let expiry = info.get("expiry").and_then(|e| e.as_i64()).unwrap_or(0);
        let stored_hash = info.get("token").and_then(|t| t.as_str()).unwrap_or("");
        let expected_hash = hash_token(token);

        expiry > Utc::now().timestamp()
            && (stored_hash == expected_hash || stored_hash == token)
    } else {
        false
    };

    if !is_valid {
        return (
            StatusCode::UNAUTHORIZED,
            HeaderMap::new(),
            Json(json!({ "success": false, "errors": ["Invalid token or session expired"] })),
        );
    }

    let accounts_data = fetch_user_accounts(&state.db, user.id as i64).await;
    let primary_account_id = accounts_data.first().map(|a| a.id);
    let primary_role = accounts_data
        .first()
        .map(|a| a.role.clone())
        .unwrap_or_else(|| "agent".to_string());

    let auth_user = AuthResponseUser {
        id: user.id,
        email: user.email.unwrap_or_else(|| uid_str.clone()),
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

    let mut resp_headers = HeaderMap::new();
    if let Some(h) = headers.get("access-token") {
        resp_headers.insert("access-token", h.clone());
    }
    resp_headers.insert("token-type", HeaderValue::from_static("Bearer"));
    if let Some(h) = headers.get("client") {
        resp_headers.insert("client", h.clone());
    }
    if let Some(h) = headers.get("expiry") {
        resp_headers.insert("expiry", h.clone());
    }
    if let Some(h) = headers.get("uid") {
        resp_headers.insert("uid", h.clone());
    }

    (
        StatusCode::OK,
        resp_headers,
        Json(json!({
            "payload": {
                "success": true,
                "data": auth_user
            },
            "data": auth_user
        })),
    )
}

pub async fn sign_out(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let client_id = headers.get("client").and_then(|h| h.to_str().ok());
    let uid = headers.get("uid").and_then(|h| h.to_str().ok());

    if let (Some(client), Some(uid_str)) = (client_id, uid) {
        let _ = sqlx::query(
            r#"
            UPDATE users
            SET tokens = tokens - $1
            WHERE (LOWER(email) = $2 OR uid = $2) AND tokens ? $1
            "#,
        )
        .bind(client)
        .bind(uid_str.trim().to_lowercase())
        .execute(&state.db)
        .await;
    }

    (StatusCode::OK, Json(json!({ "success": true })))
}

fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

pub async fn fetch_user_accounts_pub(db: &sqlx::PgPool, user_id: i64) -> Vec<AuthResponseAccount> {
    fetch_user_accounts(db, user_id).await
}

async fn fetch_user_accounts(db: &sqlx::PgPool, user_id: i64) -> Vec<AuthResponseAccount> {
    #[derive(sqlx::FromRow)]
    struct AccountRow {
        account_id: i64,
        name: String,
        status: Option<i32>,
        role: Option<i32>,
        active_at: Option<chrono::NaiveDateTime>,
        availability: i32,
        auto_offline: bool,
    }

    let rows = sqlx::query_as::<_, AccountRow>(
        r#"
        SELECT au.account_id, a.name, a.status, au.role, au.active_at,
               au.availability, au.auto_offline
        FROM account_users au
        JOIN accounts a ON a.id = au.account_id
        WHERE au.user_id = $1
        ORDER BY au.active_at DESC NULLS LAST
        "#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await
    .unwrap_or_default();

    rows.into_iter()
        .map(|r| {
            let role_str = match r.role {
                Some(1) => "administrator",
                _ => "agent",
            };
            let status_str = match r.status {
                Some(1) => "suspended",
                _ => "active",
            };
            let permissions = match r.role {
                Some(1) => vec!["administrator".to_string()],
                _ => vec!["agent".to_string()],
            };
            AuthResponseAccount {
                id: r.account_id,
                name: r.name,
                status: status_str.to_string(),
                role: role_str.to_string(),
                permissions,
                active_at: r.active_at,
                availability: r.availability,
                availability_status: "online".to_string(),
                auto_offline: r.auto_offline,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bcrypt_verification() {
        let password = "password123";
        let hashed = bcrypt::hash(password, 4).expect("Bcrypt hash failed");
        assert!(bcrypt::verify(password, &hashed).expect("Verify failed"));
        assert!(!bcrypt::verify("wrongpassword", &hashed).expect("Verify failed"));
    }

    #[test]
    fn test_token_hashing() {
        let token = "test_access_token_xyz_123";
        let hash1 = hash_token(token);
        let hash2 = hash_token(token);
        assert_eq!(hash1, hash2);
        assert_ne!(hash1, token);
    }
}
