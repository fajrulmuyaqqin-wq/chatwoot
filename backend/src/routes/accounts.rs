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

    // Ambil daftar akun pengguna terkini
    let user_accounts = sqlx::query_as::<_, (i32, String, Option<i32>, i32)>(
        r#"
        SELECT a.id, a.name, a.locale, au.role
        FROM account_users au
        JOIN accounts a ON a.id = au.account_id
        WHERE au.user_id = $1
        ORDER BY a.id ASC
        "#,
    )
    .bind(current_user.id as i64)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let accounts_json: Vec<_> = user_accounts
        .into_iter()
        .map(|(aid, aname, alocale, arole)| {
            json!({
                "id": aid,
                "name": aname,
                "role": if arole == 1 { "administrator" } else { "agent" },
                "locale": locale_int_to_code(alocale),
                "availability": "online",
                "availability_status": "online"
            })
        })
        .collect();

    let user_row = sqlx::query!(
        "SELECT name, display_name FROM users WHERE id = $1",
        current_user.id
    )
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    let user_name = user_row.as_ref().map(|u| u.name.clone()).unwrap_or_else(|| current_user.email.clone());
    let display_name = user_row.as_ref().and_then(|u| u.display_name.clone());
    let available_name = display_name
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| user_name.clone());

    (
        StatusCode::OK,
        Json(json!({
            "data": {
                "id": current_user.id,
                "email": current_user.email,
                "name": user_name,
                "display_name": display_name,
                "available_name": available_name,
                "avatar_url": serde_json::Value::Null,
                "account_id": account.id,
                "role": "administrator",
                "accounts": accounts_json
            },
            "id": account.id,
            "account_id": account.id,
            "name": account.name,
            "role": "administrator",
            "status": "active",
            "locale": locale_int_to_code(account.locale),
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

            let custom_attrs = a.custom_attributes.clone().unwrap_or(json!({}));
            let tier_str = custom_attrs
                .get("plan_name")
                .or_else(|| custom_attrs.get("tier"))
                .and_then(|v| v.as_str());

            let current_tier = tier_str.and_then(crate::services::tier::AccountTier::from_str);
            let effective_tier = current_tier.unwrap_or(crate::services::tier::AccountTier::Free);

            let mut final_custom_attrs = custom_attrs;
            if let Some(obj) = final_custom_attrs.as_object_mut() {
                obj.insert("plan_name".to_string(), json!(effective_tier.as_str()));
                obj.insert("tier".to_string(), json!(effective_tier.as_str()));
                obj.insert("tier_name".to_string(), json!(effective_tier.display_name()));
                obj.insert("badge".to_string(), json!(effective_tier.badge()));
                obj.insert("is_plan_registered".to_string(), json!(current_tier.is_some()));
            }

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
                    "custom_attributes": final_custom_attrs,
                    "limits": effective_tier.limits(),
                    "latest_chatwoot_version": "4.1.0",
                    "features": effective_tier.features(),
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

#[derive(Debug, Deserialize)]
pub struct SetTierPayload {
    pub tier: String,
}

/// GET /api/v1/accounts/:account_id/tier
pub async fn get_tier(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) = verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await {
        return (code, Json(json!({ "error": msg }))).into_response();
    }

    let acc = sqlx::query_as::<_, Account>("SELECT * FROM accounts WHERE id = $1")
        .bind(account_id)
        .fetch_optional(&state.db)
        .await;

    match acc {
        Ok(Some(a)) => {
            let custom_attrs = a.custom_attributes.unwrap_or(json!({}));
            let tier_str = custom_attrs
                .get("plan_name")
                .or_else(|| custom_attrs.get("tier"))
                .and_then(|v| v.as_str());

            let current_tier = tier_str.and_then(crate::services::tier::AccountTier::from_str);
            let is_registered = current_tier.is_some();
            let effective_tier = current_tier.unwrap_or(crate::services::tier::AccountTier::Free);

            (
                StatusCode::OK,
                Json(json!({
                    "account_id": a.id,
                    "account_name": a.name,
                    "is_registered": is_registered,
                    "current_tier": effective_tier.as_str(),
                    "tier_name": effective_tier.display_name(),
                    "badge": effective_tier.badge(),
                    "price_monthly": effective_tier.price_monthly(),
                    "limits": effective_tier.limits(),
                    "features": effective_tier.features(),
                    "available_tiers": crate::services::tier::AccountTier::all_tiers_catalog()
                })),
            ).into_response()
        }
        _ => (StatusCode::NOT_FOUND, Json(json!({ "error": "Account not found" }))).into_response(),
    }
}

/// POST /api/v1/accounts/:account_id/tier (Mendaftarkan atau mengubah paket tier akun)
pub async fn set_tier(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(payload): Json<SetTierPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) = verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await {
        return (code, Json(json!({ "error": msg }))).into_response();
    }

    let target_tier = match crate::services::tier::AccountTier::from_str(&payload.tier) {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": "Invalid tier. Supported: free, small, max, enterprise" })),
            ).into_response();
        }
    };

    let limits = target_tier.limits();
    let features = target_tier.features();

    let existing_acc = match sqlx::query_as::<_, Account>("SELECT * FROM accounts WHERE id = $1")
        .bind(account_id)
        .fetch_optional(&state.db)
        .await
    {
        Ok(Some(a)) => a,
        _ => return (StatusCode::NOT_FOUND, Json(json!({ "error": "Account not found" }))).into_response(),
    };

    let mut custom_attrs = existing_acc.custom_attributes.unwrap_or(json!({}));
    if let Some(obj) = custom_attrs.as_object_mut() {
        obj.insert("plan_name".to_string(), json!(target_tier.as_str()));
        obj.insert("tier".to_string(), json!(target_tier.as_str()));
        obj.insert("tier_name".to_string(), json!(target_tier.display_name()));
        obj.insert("badge".to_string(), json!(target_tier.badge()));
        obj.insert("is_plan_registered".to_string(), json!(true));
        obj.insert("tier_registered_at".to_string(), json!(chrono::Utc::now().to_rfc3339()));
        obj.insert("subscribed_quantity".to_string(), json!(target_tier.agent_limit()));
    }

    let updated = sqlx::query_as::<_, Account>(
        r#"
        UPDATE accounts
        SET custom_attributes = $1,
            limits = $2,
            updated_at = NOW()
        WHERE id = $3
        RETURNING *
        "#
    )
    .bind(&custom_attrs)
    .bind(&limits)
    .bind(account_id)
    .fetch_one(&state.db)
    .await;

    match updated {
        Ok(a) => {
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
                    "custom_attributes": custom_attrs,
                    "limits": limits,
                    "features": features,
                    "current_tier": target_tier.as_str(),
                    "tier_name": target_tier.display_name(),
                    "badge": target_tier.badge(),
                    "message": format!("Tier successfully set to {}", target_tier.display_name())
                })),
            ).into_response()
        }
        Err(e) => {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Failed to update tier: {}", e) })),
            ).into_response()
        }
    }
}

/// GET /enterprise/api/v1/accounts/:account_id/subscription
/// POST /enterprise/api/v1/accounts/:account_id/subscription
pub async fn get_subscription(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) = verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await {
        return (code, Json(json!({ "error": msg }))).into_response();
    }

    let acc = sqlx::query_as::<_, Account>("SELECT * FROM accounts WHERE id = $1")
        .bind(account_id)
        .fetch_optional(&state.db)
        .await;

    match acc {
        Ok(Some(a)) => {
            let custom_attrs = a.custom_attributes.unwrap_or(json!({}));
            let tier_str = custom_attrs
                .get("plan_name")
                .or_else(|| custom_attrs.get("tier"))
                .and_then(|v| v.as_str())
                .unwrap_or("free");

            let tier = crate::services::tier::AccountTier::from_str(tier_str)
                .unwrap_or(crate::services::tier::AccountTier::Free);

            (
                StatusCode::OK,
                Json(json!({
                    "state": "active",
                    "plan_name": tier.as_str(),
                    "tier": tier.as_str(),
                    "tier_name": tier.display_name(),
                    "badge": tier.badge(),
                    "limits": tier.limits(),
                    "features": tier.features(),
                    "currency_selection_required": false
                })),
            ).into_response()
        }
        _ => (StatusCode::NOT_FOUND, Json(json!({ "error": "Account not found" }))).into_response(),
    }
}

/// GET /enterprise/api/v1/accounts/:account_id/limits
pub async fn get_limits(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) = verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await {
        return (code, Json(json!({ "error": msg }))).into_response();
    }

    let acc = sqlx::query_as::<_, Account>("SELECT * FROM accounts WHERE id = $1")
        .bind(account_id)
        .fetch_optional(&state.db)
        .await;

    match acc {
        Ok(Some(a)) => {
            let custom_attrs = a.custom_attributes.unwrap_or(json!({}));
            let tier_str = custom_attrs
                .get("plan_name")
                .or_else(|| custom_attrs.get("tier"))
                .and_then(|v| v.as_str())
                .unwrap_or("free");

            let tier = crate::services::tier::AccountTier::from_str(tier_str)
                .unwrap_or(crate::services::tier::AccountTier::Free);

            (
                StatusCode::OK,
                Json(json!({
                    "limits": tier.limits()
                })),
            ).into_response()
        }
        _ => (StatusCode::NOT_FOUND, Json(json!({ "error": "Account not found" }))).into_response(),
    }
}

/// GET /enterprise/api/v1/accounts/:account_id/billing_summary
pub async fn get_billing_summary(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) = verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await {
        return (code, Json(json!({ "error": msg }))).into_response();
    }

    let acc = sqlx::query_as::<_, Account>("SELECT * FROM accounts WHERE id = $1")
        .bind(account_id)
        .fetch_optional(&state.db)
        .await;

    match acc {
        Ok(Some(a)) => {
            let custom_attrs = a.custom_attributes.unwrap_or(json!({}));
            let tier_str = custom_attrs
                .get("plan_name")
                .or_else(|| custom_attrs.get("tier"))
                .and_then(|v| v.as_str())
                .unwrap_or("free");

            let tier = crate::services::tier::AccountTier::from_str(tier_str)
                .unwrap_or(crate::services::tier::AccountTier::Free);

            (
                StatusCode::OK,
                Json(json!({
                    "plan_name": tier.display_name(),
                    "tier": tier.as_str(),
                    "badge": tier.badge(),
                    "price_monthly": tier.price_monthly(),
                    "currency": "IDR",
                    "status": "active"
                })),
            ).into_response()
        }
        _ => (StatusCode::NOT_FOUND, Json(json!({ "error": "Account not found" }))).into_response(),
    }
}

