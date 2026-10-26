use crate::middleware::auth::CurrentUser;
use crate::middleware::rbac::{verify_account_access, AccountRole};
use crate::models::user::Account;
use crate::state::AppState;
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use tracing::warn;

fn locale_code_to_int(val: &serde_json::Value) -> Option<i32> {
    if let Some(i) = val.as_i64() {
        return Some(i as i32);
    }
    if let Some(s) = val.as_str() {
        return match s.to_lowercase().as_str() {
            "en" => Some(0),
            "ar" => Some(1),
            "nl" => Some(2),
            "fr" => Some(3),
            "de" => Some(4),
            "hi" => Some(5),
            "it" => Some(6),
            "ja" => Some(7),
            "ko" => Some(8),
            "pt" => Some(9),
            "ru" => Some(10),
            "zh" => Some(11),
            "es" => Some(12),
            "id" => Some(26),
            _ => Some(0),
        };
    }
    None
}

fn locale_int_to_code(code: Option<i32>) -> &'static str {
    match code {
        Some(0) => "en",
        Some(1) => "ar",
        Some(2) => "nl",
        Some(3) => "fr",
        Some(4) => "de",
        Some(5) => "hi",
        Some(6) => "it",
        Some(7) => "ja",
        Some(8) => "ko",
        Some(9) => "pt",
        Some(10) => "ru",
        Some(11) => "zh",
        Some(12) => "es",
        Some(26) => "id",
        _ => "en",
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateAccountPayload {
    pub name: Option<String>,
    pub account_name: Option<String>,
    pub locale: Option<serde_json::Value>,
    pub domain: Option<String>,
    pub support_email: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAccountPayload {
    pub name: Option<String>,
    pub locale: Option<serde_json::Value>,
    pub domain: Option<String>,
    pub support_email: Option<String>,
    pub settings: Option<serde_json::Value>,
    pub custom_attributes: Option<serde_json::Value>,
}

/// POST /api/v1/accounts (Membuat tenant akun baru dan mengaitkan pemanggil sebagai Administrator)
pub async fn create_account(
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body): Json<CreateAccountPayload>,
) -> impl IntoResponse {
    let acc_name = body
        .name
        .or(body.account_name)
        .unwrap_or_else(|| format!("{}'s Workspace", current_user.email));

    // 1. Buat record akun di tabel `accounts`
    let new_account = sqlx::query_as::<_, Account>(
        r#"
        INSERT INTO accounts (name, locale, domain, support_email, created_at, updated_at)
        VALUES ($1, $2, $3, $4, NOW(), NOW())
        RETURNING *
        "#,
    )
    .bind(&acc_name)
    .bind(body.locale.as_ref().and_then(locale_code_to_int).unwrap_or(0))
    .bind(body.domain)
    .bind(body.support_email)
    .fetch_one(&state.db)
    .await;

    let account = match new_account {
        Ok(a) => a,
        Err(e) => {
            warn!("Failed to create tenant account: {}", e);
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": format!("Could not create account: {}", e) })),
            );
        }
    };

    // 2. Kaitkan user pemanggil sebagai Administrator (role = 1)
    let _ = sqlx::query!(
        r#"
        INSERT INTO account_users (account_id, user_id, role, auto_offline, availability, created_at, updated_at)
        VALUES ($1, $2, 1, false, 0, NOW(), NOW())
        ON CONFLICT (account_id, user_id) DO UPDATE SET role = 1
        "#,
        account.id as i64,
        current_user.id as i64
    )
    .execute(&state.db)
    .await;

    // 3. Buat default Support Inbox agar tenant baru langsung siap pakai
    let _ = sqlx::query!(
        r#"
        INSERT INTO inboxes (name, channel_type, account_id, created_at, updated_at)
        VALUES ('General Support', 'Channel::Api', $1, NOW(), NOW())
        "#,
        account.id
    )
    .execute(&state.db)
    .await;

    (
        StatusCode::OK,
        Json(json!({
            "id": account.id,
            "name": account.name,
            "role": "administrator",
            "status": "active",
            "locale": account.locale,
            "domain": account.domain,
            "support_email": account.support_email
        })),
    )
}

/// GET /api/v1/accounts/:account_id
pub async fn get_account(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) = verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await {
        return (code, Json(json!({ "error": msg })));
    }

    let acc = sqlx::query_as::<_, Account>(
        "SELECT * FROM accounts WHERE id = $1",
    )
    .bind(account_id)
    .fetch_optional(&state.db)
    .await;

    match acc {
        Ok(Some(a)) => {
            let status_str = match a.status {
                Some(1) => "suspended",
                _ => "active",
            };
            (
                StatusCode::OK,
                Json(json!({
                    "id": a.id,
                    "name": a.name,
                    "status": status_str,
                    "locale": locale_int_to_code(a.locale),
                    "domain": a.domain,
                    "support_email": a.support_email,
                    "settings": a.settings.unwrap_or(json!({})),
                    "custom_attributes": a.custom_attributes.unwrap_or(json!({})),
                    "latest_chatwoot_version": "4.1.0",
                    "features": {
                        "inbound_emails": true,
                        "channel_email": true,
                        "channel_facebook": true,
                        "channel_twitter": true,
                        "channel_website": true,
                        "channel_instagram": true,
                        "channel_telegram": true,
                        "campaigns": true,
                        "campaign_analytics": true,
                        "sla": true,
                        "captain_integration": true,
                        "captain_integration_v2": true,
                        "custom_tools": true,
                        "custom_roles": true,
                        "audit_logs": true,
                        "help_center": true,
                        "companies": true,
                        "advanced_search": true,
                        "macros": true,
                        "canned_responses": true,
                        "labels": true,
                        "automations": true,
                        "agent_management": true,
                        "team_management": true,
                        "inbox_management": true,
                        "api_and_webhooks": true,
                        "saml": true,
                        "auto_resolve_conversations": true,
                        "conversation_required_attributes": true,
                        "advanced_assignment": true,
                        "assignment_v2": true,
                        "conversation_monitors": true,
                        "reports": true,
                        "custom_attributes": true,
                        "data_import": true,
                        "agent_bots": true,
                        "voice_recorder": true,
                        "channel_voice": true,
                        "auto_translate": true
                    },
                    "cache_keys": {
                        "label": 0,
                        "inbox": 0,
                        "team": 0
                    }
                })),
            )
        }
        _ => (StatusCode::NOT_FOUND, Json(json!({ "error": "Account not found" }))),
    }
}

/// PATCH /api/v1/accounts/:account_id
pub async fn update_account(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(raw_body): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err((code, msg)) = verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await {
        return (code, Json(json!({ "error": msg })));
    }

    let existing = match sqlx::query_as::<_, Account>(
        "SELECT * FROM accounts WHERE id = $1 LIMIT 1"
    )
    .bind(account_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(a)) => a,
        Ok(None) => return (StatusCode::NOT_FOUND, Json(json!({ "error": "Account not found" }))),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))),
    };

    let name = raw_body.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or(existing.name);
    let locale_int = raw_body.get("locale").and_then(locale_code_to_int).or(existing.locale);
    let domain = if raw_body.get("domain").is_some() {
        raw_body.get("domain").and_then(|v| v.as_str()).map(|s| s.to_string())
    } else {
        existing.domain
    };
    let support_email = if raw_body.get("support_email").is_some() {
        raw_body.get("support_email").and_then(|v| v.as_str()).map(|s| s.to_string())
    } else {
        existing.support_email
    };

    let mut settings = existing.settings.unwrap_or_else(|| json!({}));
    if let Some(obj) = settings.as_object_mut() {
        if let Some(s_obj) = raw_body.get("settings").and_then(|s| s.as_object()) {
            for (k, v) in s_obj {
                obj.insert(k.clone(), v.clone());
            }
        }
        for key in &[
            "auto_resolve_after",
            "auto_resolve_message",
            "auto_resolve_ignore_waiting",
            "audio_transcriptions",
            "auto_resolve_label",
            "auto_translate",
            "translation_provider",
            "enforce_mfa",
        ] {
            if let Some(val) = raw_body.get(*key) {
                obj.insert((*key).to_string(), val.clone());
            }
        }
    }

    let mut custom_attributes = existing.custom_attributes.unwrap_or_else(|| json!({}));
    if let Some(obj) = custom_attributes.as_object_mut() {
        if let Some(ca_obj) = raw_body.get("custom_attributes").and_then(|ca| ca.as_object()) {
            for (k, v) in ca_obj {
                obj.insert(k.clone(), v.clone());
            }
        }
    }

    let result = sqlx::query_as::<_, Account>(
        r#"
        UPDATE accounts
        SET name = $1,
            locale = $2,
            domain = $3,
            support_email = $4,
            settings = $5,
            custom_attributes = $6,
            updated_at = NOW()
        WHERE id = $7
        RETURNING *
        "#,
    )
    .bind(name)
    .bind(locale_int)
    .bind(domain)
    .bind(support_email)
    .bind(settings)
    .bind(custom_attributes)
    .bind(account_id)
    .fetch_optional(&state.db)
    .await;

    match result {
        Ok(Some(a)) => (
            StatusCode::OK,
            Json(json!({
                "id": a.id,
                "name": a.name,
                "locale": locale_int_to_code(a.locale),
                "domain": a.domain,
                "support_email": a.support_email,
                "settings": a.settings.unwrap_or(json!({})),
                "custom_attributes": a.custom_attributes.unwrap_or(json!({}))
            })),
        ),
        Ok(None) => (StatusCode::NOT_FOUND, Json(json!({ "error": "Account not found" }))),
        Err(e) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

/// GET /api/v1/accounts/:account_id/cache_keys
pub async fn get_cache_keys(
    Path(_account_id): Path<i32>,
    State(_state): State<AppState>,
) -> impl IntoResponse {
    let now = chrono::Utc::now().timestamp();
    (
        StatusCode::OK,
        Json(json!({
            "cache_keys": {
                "label": now,
                "inbox": now,
                "team": now,
                "canned_response": now
            }
        })),
    )
}

