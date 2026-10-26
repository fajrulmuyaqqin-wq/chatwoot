use crate::middleware::auth::CurrentUser;
use crate::middleware::rbac::require_super_admin;
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

#[derive(Debug, Deserialize)]
pub struct SuperAdminCreateAccountPayload {
    pub name: String,
    pub locale: Option<i32>,
    pub domain: Option<String>,
    pub support_email: Option<String>,
    pub admin_email: Option<String>,
    pub admin_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SuperAdminCreateUserPayload {
    pub name: String,
    pub email: String,
    pub password: Option<String>,
    pub super_admin: Option<bool>,
}

/// GET /super_admin/api/v1/accounts
pub async fn list_all_accounts(
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) = require_super_admin(&current_user) {
        return (code, Json(json!({ "error": msg })));
    }

    let accounts = sqlx::query!(
        r#"
        SELECT 
            a.id, a.name, a.locale, a.domain, a.support_email, a.created_at,
            COUNT(DISTINCT au.user_id) as users_count,
            COUNT(DISTINCT c.id) as conversations_count
        FROM accounts a
        LEFT JOIN account_users au ON au.account_id = a.id
        LEFT JOIN conversations c ON c.account_id = a.id
        GROUP BY a.id, a.name, a.locale, a.domain, a.support_email, a.created_at
        ORDER BY a.id ASC
        "#
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let list: Vec<_> = accounts
        .into_iter()
        .map(|a| {
            json!({
                "id": a.id,
                "name": a.name,
                "locale": a.locale,
                "domain": a.domain,
                "support_email": a.support_email,
                "created_at": a.created_at.and_utc().timestamp(),
                "users_count": a.users_count.unwrap_or(0),
                "conversations_count": a.conversations_count.unwrap_or(0)
            })
        })
        .collect();

    (StatusCode::OK, Json(json!(list)))
}

/// POST /super_admin/api/v1/accounts
pub async fn super_admin_create_account(
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body): Json<SuperAdminCreateAccountPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) = require_super_admin(&current_user) {
        return (code, Json(json!({ "error": msg })));
    }

    let account = match sqlx::query_as::<_, Account>(
        r#"
        INSERT INTO accounts (name, locale, domain, support_email, created_at, updated_at)
        VALUES ($1, $2, $3, $4, NOW(), NOW())
        RETURNING *
        "#,
    )
    .bind(&body.name)
    .bind(body.locale.unwrap_or(0))
    .bind(body.domain)
    .bind(body.support_email)
    .fetch_one(&state.db)
    .await
    {
        Ok(a) => a,
        Err(e) => {
            warn!("SuperAdmin failed to create account: {}", e);
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": format!("Could not provision account: {}", e) })),
            );
        }
    };

    // Jika Super Admin menyertakan admin_email, tautkan sebagai Administrator akun ini
    if let Some(admin_email) = body.admin_email {
        let clean_email = admin_email.trim().to_lowercase();
        let user_id = if let Ok(Some(u)) = sqlx::query!(
            "SELECT id FROM users WHERE LOWER(email) = $1",
            clean_email
        )
        .fetch_optional(&state.db)
        .await
        {
            u.id
        } else {
            let admin_name = body.admin_name.unwrap_or_else(|| "Account Admin".to_string());
            let hashed_pw = bcrypt::hash("password123", bcrypt::DEFAULT_COST).unwrap_or_default();
            sqlx::query_scalar!(
                r#"
                INSERT INTO users (name, email, uid, provider, encrypted_password, created_at, updated_at, confirmed_at, sign_in_count)
                VALUES ($1, $2, $2, 'email', $3, NOW(), NOW(), NOW(), 0)
                RETURNING id
                "#,
                admin_name,
                clean_email,
                hashed_pw
            )
            .fetch_one(&state.db)
            .await
            .unwrap_or(1)
        };

        let _ = sqlx::query!(
            r#"
            INSERT INTO account_users (account_id, user_id, role, auto_offline, availability, created_at, updated_at)
            VALUES ($1, $2, 1, false, 0, NOW(), NOW())
            ON CONFLICT (account_id, user_id) DO UPDATE SET role = 1
            "#,
            account.id as i64,
            user_id as i64
        )
        .execute(&state.db)
        .await;
    }

    (StatusCode::OK, Json(json!(account)))
}

/// GET /super_admin/api/v1/accounts/:id
pub async fn super_admin_get_account(
    Path(id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) = require_super_admin(&current_user) {
        return (code, Json(json!({ "error": msg })));
    }

    let acc = sqlx::query_as::<_, Account>(
        "SELECT * FROM accounts WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await;

    match acc {
        Ok(Some(a)) => (StatusCode::OK, Json(json!(a))),
        _ => (StatusCode::NOT_FOUND, Json(json!({ "error": "Account not found" }))),
    }
}

/// POST /super_admin/api/v1/accounts/:id/seed
pub async fn super_admin_seed_account(
    Path(id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) = require_super_admin(&current_user) {
        return (code, Json(json!({ "error": msg })));
    }

    // 1. Buat Inbox Default
    let inbox_id = sqlx::query_scalar!(
        r#"
        INSERT INTO inboxes (name, channel_type, account_id, created_at, updated_at)
        VALUES ('Primary WhatsApp Support', 'Channel::Whatsapp', $1, NOW(), NOW())
        RETURNING id
        "#,
        id
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(1);

    // 2. Buat Canned Response Bawaan
    let _ = sqlx::query!(
        r#"
        INSERT INTO canned_responses (account_id, short_code, content, created_at, updated_at)
        VALUES ($1, 'salam', 'Halo! Terima kasih telah menghubungi kami. Ada yang bisa kami bantu hari ini?', NOW(), NOW())
        ON CONFLICT DO NOTHING
        "#,
        id
    )
    .execute(&state.db)
    .await;

    // 3. Buat Label Bawaan
    let _ = sqlx::query!(
        r#"
        INSERT INTO labels (account_id, title, color, show_on_sidebar, created_at, updated_at)
        VALUES ($1, 'prioritas', '#e11d48', true, NOW(), NOW())
        ON CONFLICT DO NOTHING
        "#,
        id as i64
    )
    .execute(&state.db)
    .await;

    // 4. Buat Kontak & Percakapan Contoh
    let contact_id = sqlx::query_scalar!(
        r#"
        INSERT INTO contacts (account_id, name, phone_number, email, created_at, updated_at)
        VALUES ($1, 'Budi Santoso', '+6281299988877', 'budi.santoso@example.id', NOW(), NOW())
        RETURNING id
        "#,
        id
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(1);

    let _ = sqlx::query!(
        r#"
        INSERT INTO conversations (account_id, inbox_id, contact_id, status, display_id, created_at, updated_at)
        VALUES ($1, $2, $3, 0, 1, NOW(), NOW())
        ON CONFLICT (account_id, display_id) DO NOTHING
        "#,
        id,
        inbox_id,
        contact_id as i64
    )
    .execute(&state.db)
    .await;

    (
        StatusCode::OK,
        Json(json!({
            "status": "seeded",
            "message": "Account seeded with default inboxes, canned responses, and sample conversation successfully."
        })),
    )
}

/// GET /super_admin/api/v1/users
pub async fn list_all_users(
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) = require_super_admin(&current_user) {
        return (code, Json(json!({ "error": msg })));
    }

    let users = sqlx::query!(
        r#"
        SELECT id, name, email, type, created_at
        FROM users
        ORDER BY id ASC
        "#
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let list: Vec<_> = users
        .into_iter()
        .map(|u| {
            json!({
                "id": u.id,
                "name": u.name,
                "email": u.email,
                "type": u.r#type,
                "is_super_admin": u.r#type.as_deref() == Some("SuperAdmin"),
                "created_at": u.created_at.and_utc().timestamp()
            })
        })
        .collect();

    (StatusCode::OK, Json(json!(list)))
}

/// POST /super_admin/api/v1/users
pub async fn super_admin_create_user(
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body): Json<SuperAdminCreateUserPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) = require_super_admin(&current_user) {
        return (code, Json(json!({ "error": msg })));
    }

    let clean_email = body.email.trim().to_lowercase();
    let pw = body.password.as_deref().unwrap_or("password123");
    let hashed_pw = bcrypt::hash(pw, bcrypt::DEFAULT_COST).unwrap_or_default();
    let user_type = if body.super_admin == Some(true) {
        Some("SuperAdmin".to_string())
    } else {
        None
    };

    let result = sqlx::query!(
        r#"
        INSERT INTO users (name, email, uid, provider, encrypted_password, type, created_at, updated_at, confirmed_at, sign_in_count)
        VALUES ($1, $2, $2, 'email', $3, $4, NOW(), NOW(), NOW(), 0)
        RETURNING id, name, email, type
        "#,
        body.name,
        clean_email,
        hashed_pw,
        user_type
    )
    .fetch_one(&state.db)
    .await;

    match result {
        Ok(u) => (
            StatusCode::OK,
            Json(json!({
                "id": u.id,
                "name": u.name,
                "email": u.email,
                "type": u.r#type,
                "is_super_admin": u.r#type.as_deref() == Some("SuperAdmin")
            })),
        ),
        Err(e) => {
            warn!("Failed to create user in super admin: {}", e);
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": format!("Could not create user: {}", e) })),
            )
        }
    }
}
