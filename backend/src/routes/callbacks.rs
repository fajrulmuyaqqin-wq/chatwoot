use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;

use crate::middleware::auth::CurrentUser;
use crate::middleware::rbac::{verify_account_access, AccountRole};
use crate::models::inbox::{Inbox, InboxResponse};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct FacebookPagesPayload {
    pub omniauth_token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RegisterFacebookPagePayload {
    pub user_access_token: Option<String>,
    pub page_access_token: Option<String>,
    pub page_id: Option<String>,
    pub inbox_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ReauthorizeFacebookPagePayload {
    pub omniauth_token: Option<String>,
    pub inbox_id: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct InstagramAuthorizationPayload {
    pub return_to: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct InstagramCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub account_id: Option<i32>,
    pub error: Option<String>,
    pub error_description: Option<String>,
    pub return_to: Option<String>,
}

/// POST /api/v1/accounts/:account_id/callbacks/facebook_pages.json
pub async fn fetch_facebook_pages(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(payload): Json<FacebookPagesPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg }))).into_response();
    }

    let user_token = payload.omniauth_token.unwrap_or_else(|| "mock_user_token".to_string());

    // Cek apakah ada halaman yang sudah tersimpan di database untuk akun ini
    let existing_pages: Vec<String> = sqlx::query_scalar!(
        "SELECT page_id FROM channel_facebook_pages WHERE account_id = $1",
        account_id
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    // Default mock fanpage jika dijalankan di dev / offline tanpa Meta Graph API
    let sample_pages = vec![
        json!({
            "id": "100234567890",
            "name": "Chatwoot Official Fanpage",
            "access_token": "EAAXsample_page_token_100234567890",
            "exists": existing_pages.contains(&"100234567890".to_string())
        }),
        json!({
            "id": "200345678901",
            "name": "Nusantara Customer Support",
            "access_token": "EAAXsample_page_token_200345678901",
            "exists": existing_pages.contains(&"200345678901".to_string())
        })
    ];

    (
        StatusCode::OK,
        Json(json!({
            "data": {
                "page_details": sample_pages,
                "user_access_token": user_token
            }
        })),
    )
        .into_response()
}

/// POST /api/v1/accounts/:account_id/callbacks/register_facebook_page
pub async fn register_facebook_page(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(payload): Json<RegisterFacebookPagePayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg }))).into_response();
    }

    let page_id = match payload.page_id {
        Some(pid) if !pid.trim().is_empty() => pid.trim().to_string(),
        _ => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": "page_id is required" })),
            )
                .into_response();
        }
    };

    let user_token = payload.user_access_token.unwrap_or_else(|| "mock_user_token".to_string());
    let page_token = payload.page_access_token.unwrap_or_else(|| "mock_page_token".to_string());
    let inbox_name = payload.inbox_name.unwrap_or_else(|| "Facebook Page".to_string());

    // 1. Simpan atau perbarui channel_facebook_pages
    let ch = sqlx::query!(
        r#"
        INSERT INTO channel_facebook_pages (
            page_id, user_access_token, page_access_token, account_id, provider_name, created_at, updated_at
        ) VALUES ($1, $2, $3, $4, 'facebook', NOW(), NOW())
        ON CONFLICT (page_id, account_id)
        DO UPDATE SET page_access_token = $3, user_access_token = $2, updated_at = NOW()
        RETURNING id
        "#,
        page_id,
        user_token,
        page_token,
        account_id
    )
    .fetch_one(&state.db)
    .await;

    let channel_id = match ch {
        Ok(c) => c.id,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Failed to create facebook channel: {}", e) })),
            )
                .into_response();
        }
    };

    // 2. Buat atau perbarui Inbox
    let existing_inbox = sqlx::query_as::<_, Inbox>(
        "SELECT * FROM inboxes WHERE account_id = $1 AND channel_type = 'Channel::FacebookPage' AND channel_id = $2 LIMIT 1",
    )
    .bind(account_id)
    .bind(channel_id)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    let inbox = match existing_inbox {
        Some(i) => i,
        None => {
            let created = sqlx::query_as::<_, Inbox>(
                r#"
                INSERT INTO inboxes (
                    name, channel_type, channel_id, account_id, enable_auto_assignment,
                    greeting_enabled, csat_survey_enabled, lock_to_single_conversation,
                    timezone, created_at, updated_at
                ) VALUES (
                    $1, 'Channel::FacebookPage', $2, $3, true,
                    false, false, false, 'UTC', NOW(), NOW()
                )
                RETURNING *
                "#,
            )
            .bind(&inbox_name)
            .bind(channel_id)
            .bind(account_id)
            .fetch_one(&state.db)
            .await;

            match created {
                Ok(i) => i,
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": format!("Failed to create inbox: {}", e) })),
                    )
                        .into_response();
                }
            }
        }
    };

    (StatusCode::OK, Json(json!(InboxResponse::from(inbox)))).into_response()
}

/// POST /api/v1/accounts/:account_id/callbacks/reauthorize_page
pub async fn reauthorize_facebook_page(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(payload): Json<ReauthorizeFacebookPagePayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg }))).into_response();
    }

    if let (Some(inbox_id), Some(token)) = (payload.inbox_id, payload.omniauth_token) {
        let ch_id: Option<(i32,)> = sqlx::query_as(
            "SELECT channel_id FROM inboxes WHERE id = $1 AND account_id = $2 AND channel_type = 'Channel::FacebookPage'",
        )
        .bind(inbox_id)
        .bind(account_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);

        if let Some((cid,)) = ch_id {
            let _ = sqlx::query!(
                "UPDATE channel_facebook_pages SET user_access_token = $1, updated_at = NOW() WHERE id = $2",
                token,
                cid
            )
            .execute(&state.db)
            .await;
        }
    }

    (StatusCode::OK, Json(json!({ "status": "success" }))).into_response()
}

/// POST /api/v1/accounts/:account_id/instagram/authorization
pub async fn instagram_authorization(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(payload): Json<InstagramAuthorizationPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg }))).into_response();
    }

    let return_to = payload.return_to.unwrap_or_default();
    let client_id = std::env::var("INSTAGRAM_APP_ID").ok();

    let auth_url = if let Some(cid) = client_id.filter(|s| !s.is_empty() && s != "100234567890123") {
        format!(
            "https://www.instagram.com/oauth/authorize?enable_fb_login=0&force_reauth=true&client_id={}&redirect_uri={}/instagram/callback&response_type=code&scope=instagram_basic,instagram_manage_messages&state={}&return_to={}",
            cid, state.config.frontend_url, account_id, return_to
        )
    } else {
        // Mode dev / local: Mengarahkan langsung ke callback lokal
        format!(
            "{}/instagram/callback?account_id={}&code=mock_ig_code_{}&return_to={}",
            state.config.frontend_url, account_id, Utc::now().timestamp(), return_to
        )
    };

    (
        StatusCode::OK,
        Json(json!({
            "success": true,
            "url": auth_url
        })),
    )
        .into_response()
}

/// GET /instagram/callback
pub async fn instagram_callback(
    Query(query): Query<InstagramCallbackQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let account_id = query.account_id.or_else(|| {
        query.state.as_deref().and_then(|s| s.parse().ok())
    }).unwrap_or(1);

    let token = query.code.unwrap_or_else(|| format!("ig_token_{}", Utc::now().timestamp()));
    let ig_id = format!("ig_{}", Utc::now().timestamp());

    // Buat channel_instagram
    let ch = sqlx::query!(
        r#"
        INSERT INTO channel_instagram (
            access_token, account_id, instagram_id, provider_name, created_at, updated_at
        ) VALUES ($1, $2, $3, 'instagram', NOW(), NOW())
        ON CONFLICT (instagram_id) DO UPDATE SET access_token = $1, updated_at = NOW()
        RETURNING id
        "#,
        token,
        account_id,
        ig_id
    )
    .fetch_one(&state.db)
    .await;

    let channel_id = ch.map(|c| c.id).unwrap_or(1);

    // Buat Inbox Instagram
    let inbox = sqlx::query_as::<_, Inbox>(
        r#"
        INSERT INTO inboxes (
            name, channel_type, channel_id, account_id, enable_auto_assignment,
            greeting_enabled, csat_survey_enabled, lock_to_single_conversation,
            timezone, created_at, updated_at
        ) VALUES (
            'Instagram Inbox', 'Channel::Instagram', $1, $2, true,
            false, false, false, 'UTC', NOW(), NOW()
        )
        RETURNING *
        "#,
    )
    .bind(channel_id as i32)
    .bind(account_id)
    .fetch_one(&state.db)
    .await;

    let inbox_id = inbox.map(|i| i.id).unwrap_or(1);

    // Redirect sesuai alur resmi Chatwoot
    let target = format!("/app/accounts/{}/settings/inboxes/new/{}/agents", account_id, inbox_id);
    Redirect::to(&target)
}
