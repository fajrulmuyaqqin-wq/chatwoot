use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::NaiveDateTime;
use serde::Deserialize;
use serde_json::json;
use tracing::warn;

use crate::middleware::auth::CurrentUser;
use crate::models::notification::{
    notification_type_to_str, Notification, NotificationDataWrapper, NotificationIndexResponse,
    NotificationMetaResponse, NotificationResponse,
};
use crate::services::notification_dispatcher::{
    dispatch_notification_deleted, dispatch_notification_updated,
};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct NotificationQuery {
    pub page: Option<i64>,
    pub snoozed: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct SnoozePayload {
    pub snoozed_until: Option<String>,
}

/// GET /api/v1/accounts/:account_id/notifications
pub async fn list_notifications(
    Path(account_id): Path<i64>,
    Query(query): Query<NotificationQuery>,
    user_opt: Option<Extension<CurrentUser>>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let current_page = query.page.unwrap_or(1).max(1);
    let per_page: i64 = 15;
    let offset = (current_page - 1) * per_page;

    let user_id = if let Some(Extension(u)) = user_opt {
        u.id as i64
    } else {
        // Fallback ke agent pertama di account
        let uid: Option<i64> = sqlx::query_scalar(
            "SELECT user_id FROM account_users WHERE account_id = $1 ORDER BY id ASC LIMIT 1",
        )
        .bind(account_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);
        uid.unwrap_or(1)
    };

    let unread_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM notifications
        WHERE account_id = $1 AND user_id = $2 AND read_at IS NULL
          AND (snoozed_until IS NULL OR snoozed_until <= NOW())
        "#,
    )
    .bind(account_id)
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications WHERE account_id = $1 AND user_id = $2",
    )
    .bind(account_id)
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let notifs = match sqlx::query_as::<_, Notification>(
        r#"
        SELECT * FROM notifications
        WHERE account_id = $1 AND user_id = $2
        ORDER BY last_activity_at DESC NULLS LAST, id DESC
        LIMIT $3 OFFSET $4
        "#,
    )
    .bind(account_id)
    .bind(user_id)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to fetch notifications: {}", e);
            vec![]
        }
    };

    let mut payload = Vec::new();
    for n in notifs {
        let type_str = notification_type_to_str(n.notification_type).to_string();
        let title = match type_str.as_str() {
            "conversation_assignment" => format!("Conversation #{} was assigned to you", n.primary_actor_id),
            "assigned_conversation_new_message" => format!("New message in conversation #{}", n.primary_actor_id),
            "conversation_creation" => format!("New conversation #{} created", n.primary_actor_id),
            _ => format!("Notification for conversation #{}", n.primary_actor_id),
        };

        // Ambil summary conversation jika primary_actor_type == 'Conversation'
        let primary_actor = if n.primary_actor_type == "Conversation" {
            let conv_info = sqlx::query_as::<_, (i32, i32, i32, Option<i32>)>(
                "SELECT id, display_id, inbox_id, contact_id FROM conversations WHERE id = $1",
            )
            .bind(n.primary_actor_id as i32)
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);

            if let Some((_cid, did, iid, maybe_contact_id)) = conv_info {
                let mut sender_name = "User".to_string();
                if let Some(contact_id) = maybe_contact_id {
                    if let Ok(Some((Some(cname),))) = sqlx::query_as::<_, (Option<String>,)>(
                        "SELECT name FROM contacts WHERE id = $1",
                    )
                    .bind(contact_id)
                    .fetch_optional(&state.db)
                    .await
                    {
                        if !cname.trim().is_empty() {
                            sender_name = cname;
                        }
                    }
                }

                Some(json!({
                    "id": did,
                    "display_id": did,
                    "inbox_id": iid,
                    "account_id": account_id,
                    "meta": {
                        "sender": {
                            "name": sender_name,
                            "thumbnail": ""
                        }
                    }
                }))
            } else {
                None
            }
        } else {
            None
        };

        payload.push(NotificationResponse {
            id: n.id,
            notification_type: type_str,
            push_message_title: title.clone(),
            push_message_body: title,
            primary_actor_type: n.primary_actor_type,
            primary_actor_id: n.primary_actor_id,
            primary_actor,
            read_at: n.read_at,
            secondary_actor: None,
            user: Some(json!({ "id": n.user_id })),
            created_at: n.created_at.and_utc().timestamp(),
            last_activity_at: n.last_activity_at.map(|t| t.and_utc().timestamp()).unwrap_or(n.created_at.and_utc().timestamp()),
            snoozed_until: n.snoozed_until,
            meta: n.meta.unwrap_or_else(|| json!({})),
            account_id: n.account_id,
        });
    }

    Json(NotificationIndexResponse {
        data: NotificationDataWrapper {
            meta: NotificationMetaResponse {
                unread_count,
                count,
                current_page,
            },
            payload,
        },
    })
    .into_response()
}

/// POST /api/v1/accounts/:account_id/notifications/read_all
pub async fn read_all_notifications(
    Path(account_id): Path<i64>,
    user_opt: Option<Extension<CurrentUser>>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let user_id = if let Some(Extension(u)) = user_opt {
        u.id as i64
    } else {
        let uid: Option<i64> = sqlx::query_scalar(
            "SELECT user_id FROM account_users WHERE account_id = $1 ORDER BY id ASC LIMIT 1",
        )
        .bind(account_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);
        uid.unwrap_or(1)
    };

    let _ = sqlx::query(
        "UPDATE notifications SET read_at = NOW(), updated_at = NOW() WHERE account_id = $1 AND user_id = $2 AND read_at IS NULL",
    )
    .bind(account_id)
    .bind(user_id)
    .execute(&state.db)
    .await;

    StatusCode::OK.into_response()
}

/// PATCH /api/v1/accounts/:account_id/notifications/:id
pub async fn mark_notification_as_read(
    Path((account_id, id)): Path<(i64, i64)>,
    user_opt: Option<Extension<CurrentUser>>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let user_id = if let Some(Extension(u)) = user_opt {
        u.id as i64
    } else {
        let uid: Option<i64> = sqlx::query_scalar(
            "SELECT user_id FROM account_users WHERE account_id = $1 ORDER BY id ASC LIMIT 1",
        )
        .bind(account_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);
        uid.unwrap_or(1)
    };

    let notif = match sqlx::query_as::<_, Notification>(
        r#"
        UPDATE notifications
        SET read_at = NOW(), updated_at = NOW()
        WHERE id = $1 AND account_id = $2 AND user_id = $3
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(account_id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(n)) => n,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Notification not found" })),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    dispatch_notification_updated(&state, account_id, user_id, &notif).await;
    Json(notif).into_response()
}

/// POST /api/v1/accounts/:account_id/notifications/:id/unread
pub async fn mark_notification_as_unread(
    Path((account_id, id)): Path<(i64, i64)>,
    user_opt: Option<Extension<CurrentUser>>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let user_id = if let Some(Extension(u)) = user_opt {
        u.id as i64
    } else {
        let uid: Option<i64> = sqlx::query_scalar(
            "SELECT user_id FROM account_users WHERE account_id = $1 ORDER BY id ASC LIMIT 1",
        )
        .bind(account_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);
        uid.unwrap_or(1)
    };

    let notif = match sqlx::query_as::<_, Notification>(
        r#"
        UPDATE notifications
        SET read_at = NULL, updated_at = NOW()
        WHERE id = $1 AND account_id = $2 AND user_id = $3
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(account_id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(n)) => n,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Notification not found" })),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    dispatch_notification_updated(&state, account_id, user_id, &notif).await;
    Json(notif).into_response()
}

/// POST /api/v1/accounts/:account_id/notifications/:id/snooze
pub async fn snooze_notification(
    Path((account_id, id)): Path<(i64, i64)>,
    user_opt: Option<Extension<CurrentUser>>,
    State(state): State<AppState>,
    Json(payload): Json<SnoozePayload>,
) -> impl IntoResponse {
    let user_id = if let Some(Extension(u)) = user_opt {
        u.id as i64
    } else {
        let uid: Option<i64> = sqlx::query_scalar(
            "SELECT user_id FROM account_users WHERE account_id = $1 ORDER BY id ASC LIMIT 1",
        )
        .bind(account_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);
        uid.unwrap_or(1)
    };

    let snoozed_until_dt: Option<NaiveDateTime> = payload
        .snoozed_until
        .as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.naive_utc());

    let notif = match sqlx::query_as::<_, Notification>(
        r#"
        UPDATE notifications
        SET snoozed_until = $1, updated_at = NOW()
        WHERE id = $2 AND account_id = $3 AND user_id = $4
        RETURNING *
        "#,
    )
    .bind(snoozed_until_dt)
    .bind(id)
    .bind(account_id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(n)) => n,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Notification not found" })),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    dispatch_notification_updated(&state, account_id, user_id, &notif).await;
    Json(notif).into_response()
}

/// POST /api/v1/accounts/:account_id/notifications/unread_count
pub async fn get_unread_count(
    Path(account_id): Path<i64>,
    user_opt: Option<Extension<CurrentUser>>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let user_id = if let Some(Extension(u)) = user_opt {
        u.id as i64
    } else {
        let uid: Option<i64> = sqlx::query_scalar(
            "SELECT user_id FROM account_users WHERE account_id = $1 ORDER BY id ASC LIMIT 1",
        )
        .bind(account_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);
        uid.unwrap_or(1)
    };

    let count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM notifications
        WHERE account_id = $1 AND user_id = $2 AND read_at IS NULL
          AND (snoozed_until IS NULL OR snoozed_until <= NOW())
        "#,
    )
    .bind(account_id)
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    Json(count).into_response()
}

/// DELETE /api/v1/accounts/:account_id/notifications/:id
pub async fn delete_notification(
    Path((account_id, id)): Path<(i64, i64)>,
    user_opt: Option<Extension<CurrentUser>>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let user_id = if let Some(Extension(u)) = user_opt {
        u.id as i64
    } else {
        let uid: Option<i64> = sqlx::query_scalar(
            "SELECT user_id FROM account_users WHERE account_id = $1 ORDER BY id ASC LIMIT 1",
        )
        .bind(account_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);
        uid.unwrap_or(1)
    };

    let res = sqlx::query(
        "DELETE FROM notifications WHERE id = $1 AND account_id = $2 AND user_id = $3",
    )
    .bind(id)
    .bind(account_id)
    .bind(user_id)
    .execute(&state.db)
    .await;

    match res {
        Ok(r) if r.rows_affected() > 0 => {
            dispatch_notification_deleted(&state, account_id, user_id, id).await;
            StatusCode::OK.into_response()
        }
        Ok(_) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Notification not found" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// DELETE /api/v1/accounts/:account_id/notifications/destroy_all
pub async fn delete_all_notifications(
    Path(account_id): Path<i64>,
    user_opt: Option<Extension<CurrentUser>>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let user_id = if let Some(Extension(u)) = user_opt {
        u.id as i64
    } else {
        let uid: Option<i64> = sqlx::query_scalar(
            "SELECT user_id FROM account_users WHERE account_id = $1 ORDER BY id ASC LIMIT 1",
        )
        .bind(account_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);
        uid.unwrap_or(1)
    };

    let _ = sqlx::query("DELETE FROM notifications WHERE account_id = $1 AND user_id = $2")
        .bind(account_id)
        .bind(user_id)
        .execute(&state.db)
        .await;

    StatusCode::OK.into_response()
}
