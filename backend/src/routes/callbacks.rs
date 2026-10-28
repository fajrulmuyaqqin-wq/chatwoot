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

#[derive(Debug, Deserialize)]
pub struct WhatsAppAuthorizationPayload {
    pub code: String,
    pub business_id: Option<String>,
    pub waba_id: String,
    pub phone_number_id: Option<String>,
    pub is_coexistence: Option<bool>,
    pub inbox_id: Option<i32>,
}

/// POST /api/v1/accounts/:account_id/whatsapp/authorization
/// Menangani initial authorization maupun reauthorization WhatsApp via Meta Embedded Signup
pub async fn whatsapp_authorization(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(payload): Json<WhatsAppAuthorizationPayload>,
) -> impl IntoResponse {
    // 1. Verifikasi hak akses admin pada akun terkait
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "success": false, "error": msg }))).into_response();
    }

    // 2. Validasi parameter input
    if payload.code.trim().is_empty() || payload.waba_id.trim().is_empty() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({
                "success": false,
                "error": "Required parameters are missing: code, waba_id"
            })),
        ).into_response();
    }

    // 3. Tukar authorization code dengan access token
    let app_id = state.config.whatsapp_app_id.as_deref().unwrap_or_default();
    let app_secret = state.config.whatsapp_app_secret.as_deref().unwrap_or_default();
    let api_version = &state.config.whatsapp_api_version;
    let base_url = &state.config.whatsapp_cloud_base_url;

    let access_token = match crate::services::meta_whatsapp::exchange_code_for_token(
        app_id,
        app_secret,
        &payload.code,
        api_version,
        base_url,
    ).await {
        Ok(token) => token,
        Err(err) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "success": false, "error": err })),
            ).into_response();
        }
    };

    // 4. Ambil informasi nomor telepon dari Meta Graph API
    let phone_info = match crate::services::meta_whatsapp::fetch_phone_number_details(
        &access_token,
        &payload.waba_id,
        payload.phone_number_id.as_deref(),
        api_version,
        base_url,
    ).await {
        Ok(info) => info,
        Err(err) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "success": false, "error": err })),
            ).into_response();
        }
    };

    // 5. Daftarkan WABA ke Webhook App (subscribed_apps)
    if let Err(err) = crate::services::meta_whatsapp::subscribe_waba_to_app(
        &access_token,
        &payload.waba_id,
        api_version,
        base_url,
    ).await {
        tracing::warn!("Failed to subscribe WABA to app webhook: {}", err);
    }

    // 6. Transaksi Database untuk Channel & Inbox
    let mut tx = match state.db.begin().await {
        Ok(t) => t,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "success": false, "error": format!("Database transaction error: {}", e) })),
            ).into_response();
        }
    };

    // Provider config JSON
    let provider_config = json!({
        "api_key": access_token,
        "phone_number_id": phone_info.phone_number_id,
        "business_account_id": payload.waba_id,
        "business_id": payload.business_id,
        "is_coexistence": payload.is_coexistence.unwrap_or(false),
        "source": "embedded_signup"
    });

    // Periksa apakah ini alur reauthorization (inbox_id disediakan)
    if let Some(inbox_id) = payload.inbox_id {
        let existing_inbox = sqlx::query_as::<_, Inbox>(
            "SELECT * FROM inboxes WHERE id = $1 AND account_id = $2"
        )
        .bind(inbox_id)
        .bind(account_id)
        .fetch_optional(&mut *tx)
        .await;

        let inbox = match existing_inbox {
            Ok(Some(i)) => i,
            Ok(None) => {
                let _ = tx.rollback().await;
                return (
                    StatusCode::NOT_FOUND,
                    Json(json!({ "success": false, "error": "Target inbox for reauthorization not found" })),
                ).into_response();
            }
            Err(e) => {
                let _ = tx.rollback().await;
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "success": false, "error": e.to_string() })),
                ).into_response();
            }
        };

        // Update channel_whatsapp terkait
        let update_channel = sqlx::query(
            r#"
            UPDATE channel_whatsapp
            SET provider_config = $1,
                business_management_token = $2,
                updated_at = NOW()
            WHERE id = $3
            "#
        )
        .bind(&provider_config)
        .bind(&access_token)
        .bind(inbox.channel_id)
        .execute(&mut *tx)
        .await;

        if let Err(e) = update_channel {
            let _ = tx.rollback().await;
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "success": false, "error": format!("Failed to update channel: {}", e) })),
            ).into_response();
        }

        if let Err(e) = tx.commit().await {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "success": false, "error": format!("Failed to commit transaction: {}", e) })),
            ).into_response();
        }

        return (
            StatusCode::OK,
            Json(json!({
                "success": true,
                "id": inbox.id,
                "name": inbox.name,
                "channel_type": "whatsapp",
                "channel_id": inbox.channel_id,
                "phone_number": phone_info.phone_number,
                "message": "Inbox reauthorized successfully"
            })),
        ).into_response();
    }

    // Alur Initial Authorization / Pembuatan Channel & Inbox Baru
    // Cek apakah nomor telepon sudah terdaftar di channel_whatsapp
    let existing_channel = sqlx::query_as::<_, crate::models::channel_whatsapp::ChannelWhatsapp>(
        "SELECT * FROM channel_whatsapp WHERE phone_number = $1"
    )
    .bind(&phone_info.phone_number)
    .fetch_optional(&mut *tx)
    .await;

    let (channel_id, inbox_id, inbox_name) = match existing_channel {
        Ok(Some(ch)) => {
            // Jika nomor ini terdaftar di akun lain, tolak dengan 422
            if ch.account_id != account_id {
                let _ = tx.rollback().await;
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(json!({
                        "success": false,
                        "error": format!("Phone number {} is already registered under another account", phone_info.phone_number)
                    })),
                ).into_response();
            }

            // Jika di akun yang sama, perbarui token dan ambil inbox yang sudah ada
            let _ = sqlx::query(
                r#"
                UPDATE channel_whatsapp
                SET provider_config = $1,
                    business_management_token = $2,
                    updated_at = NOW()
                WHERE id = $3
                "#
            )
            .bind(&provider_config)
            .bind(&access_token)
            .bind(ch.id)
            .execute(&mut *tx)
            .await;

            let existing_inbox = sqlx::query_as::<_, Inbox>(
                "SELECT * FROM inboxes WHERE channel_id = $1 AND account_id = $2"
            )
            .bind(ch.id)
            .bind(account_id)
            .fetch_optional(&mut *tx)
            .await;

            match existing_inbox {
                Ok(Some(ib)) => (ch.id, ib.id, ib.name),
                _ => {
                    // Jika belum ada inbox record, buat inbox baru
                    let name = format!("{} WhatsApp", phone_info.display_name);
                    let new_inbox = sqlx::query!(
                        r#"
                        INSERT INTO inboxes (
                            channel_id, account_id, name, channel_type,
                            created_at, updated_at
                        ) VALUES ($1, $2, $3, 'Channel::Whatsapp', NOW(), NOW())
                        RETURNING id
                        "#,
                        ch.id,
                        account_id,
                        name
                    )
                    .fetch_one(&mut *tx)
                    .await;

                    match new_inbox {
                        Ok(r) => (ch.id, r.id, name),
                        Err(e) => {
                            let _ = tx.rollback().await;
                            return (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(json!({ "success": false, "error": e.to_string() })),
                            ).into_response();
                        }
                    }
                }
            }
        }
        Ok(None) => {
            // Masukkan channel_whatsapp baru
            let new_ch = sqlx::query!(
                r#"
                INSERT INTO channel_whatsapp (
                    account_id, phone_number, provider, provider_config,
                    business_management_token, created_at, updated_at
                ) VALUES ($1, $2, 'whatsapp_cloud', $3, $4, NOW(), NOW())
                RETURNING id
                "#,
                account_id,
                phone_info.phone_number,
                provider_config,
                access_token
            )
            .fetch_one(&mut *tx)
            .await;

            let ch_id = match new_ch {
                Ok(r) => r.id,
                Err(e) => {
                    let _ = tx.rollback().await;
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "success": false, "error": format!("Failed to create channel: {}", e) })),
                    ).into_response();
                }
            };

            // Masukkan inboxes baru
            let name = format!("{} WhatsApp", phone_info.display_name);
            let new_inbox = sqlx::query!(
                r#"
                INSERT INTO inboxes (
                    channel_id, account_id, name, channel_type,
                    created_at, updated_at
                ) VALUES ($1, $2, $3, 'Channel::Whatsapp', NOW(), NOW())
                RETURNING id
                "#,
                ch_id,
                account_id,
                name
            )
            .fetch_one(&mut *tx)
            .await;

            let ib_id = match new_inbox {
                Ok(r) => r.id,
                Err(e) => {
                    let _ = tx.rollback().await;
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "success": false, "error": format!("Failed to create inbox: {}", e) })),
                    ).into_response();
                }
            };

            (ch_id, ib_id, name)
        }
        Err(e) => {
            let _ = tx.rollback().await;
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "success": false, "error": e.to_string() })),
            ).into_response();
        }
    };

    // Tambahkan user saat ini sebagai anggota inbox awal (inbox_members)
    let _ = sqlx::query(
        r#"
        INSERT INTO inbox_members (inbox_id, user_id, created_at, updated_at)
        VALUES ($1, $2, NOW(), NOW())
        ON CONFLICT DO NOTHING
        "#
    )
    .bind(inbox_id)
    .bind(current_user.id)
    .execute(&mut *tx)
    .await;

    if let Err(e) = tx.commit().await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "error": format!("Failed to commit transaction: {}", e) })),
        ).into_response();
    }

    (
        StatusCode::OK,
        Json(json!({
            "success": true,
            "id": inbox_id,
            "name": inbox_name,
            "channel_type": "whatsapp",
            "channel_id": channel_id,
            "phone_number": phone_info.phone_number
        })),
    ).into_response()
}
