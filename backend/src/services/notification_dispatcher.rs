use crate::models::notification::{notification_type_to_str, Notification};
use crate::services::event_dispatcher;
use crate::state::AppState;
use serde_json::{json, Value};
use tracing::info;

pub async fn create_and_dispatch_notification(
    state: &AppState,
    account_id: i64,
    user_id: i64,
    notification_type: i32,
    primary_actor_id: i64,
    primary_actor_type: &str,
    secondary_actor_id: Option<i64>,
    secondary_actor_type: Option<&str>,
    meta: Option<Value>,
) -> Result<Notification, String> {
    let meta_val = meta.unwrap_or_else(|| json!({}));

    let notif = sqlx::query_as::<_, Notification>(
        r#"
        INSERT INTO notifications (
            account_id, user_id, notification_type, primary_actor_type, primary_actor_id,
            secondary_actor_type, secondary_actor_id, meta, created_at, updated_at, last_activity_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW(), NOW(), NOW())
        RETURNING *
        "#,
    )
    .bind(account_id)
    .bind(user_id)
    .bind(notification_type)
    .bind(primary_actor_type)
    .bind(primary_actor_id)
    .bind(secondary_actor_type)
    .bind(secondary_actor_id)
    .bind(&meta_val)
    .fetch_one(&state.db)
    .await
    .map_err(|e| format!("Failed to insert notification: {}", e))?;

    // Hitung unread count dan total count untuk user
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

    let total_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM notifications
        WHERE account_id = $1 AND user_id = $2
        "#,
    )
    .bind(account_id)
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    // Ambil ringkasan primary_actor (Conversation)
    let conv_summary = sqlx::query_as::<_, (i32, i32, i32, Option<String>)>(
        "SELECT id, display_id, inbox_id, additional_attributes::text FROM conversations WHERE id = $1",
    )
    .bind(primary_actor_id as i32)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    let primary_actor_json = conv_summary.map(|(cid, did, iid, _)| {
        json!({
            "id": cid,
            "display_id": did,
            "inbox_id": iid,
            "account_id": account_id
        })
    });

    let type_str = notification_type_to_str(notif.notification_type).to_string();

    let title = match type_str.as_str() {
        "conversation_assignment" => format!("Conversation #{} was assigned to you", primary_actor_id),
        "assigned_conversation_new_message" => format!("New message in conversation #{}", primary_actor_id),
        "conversation_creation" => format!("New conversation #{} created", primary_actor_id),
        _ => format!("Notification for conversation #{}", primary_actor_id),
    };

    let push_data = json!({
        "id": notif.id,
        "notification_type": type_str,
        "push_message_title": title,
        "push_message_body": title,
        "primary_actor_type": notif.primary_actor_type,
        "primary_actor_id": notif.primary_actor_id,
        "primary_actor": primary_actor_json,
        "read_at": notif.read_at,
        "secondary_actor": Value::Null,
        "user": json!({ "id": user_id }),
        "created_at": notif.created_at.and_utc().timestamp(),
        "last_activity_at": notif.last_activity_at.map(|t| t.and_utc().timestamp()).unwrap_or(notif.created_at.and_utc().timestamp()),
        "snoozed_until": notif.snoozed_until,
        "meta": notif.meta.clone().unwrap_or_else(|| json!({})),
        "account_id": account_id
    });

    // Broadcast event ke ActionCable
    event_dispatcher::dispatch_event(
        state,
        account_id as i32,
        "notification.created",
        json!({
            "notification": push_data,
            "unread_count": unread_count,
            "count": total_count
        }),
    );

    info!(
        "Dispatched notification id={} to user_id={} for account_id={}",
        notif.id, user_id, account_id
    );

    Ok(notif)
}

pub async fn dispatch_notification_updated(
    state: &AppState,
    account_id: i64,
    user_id: i64,
    notif: &Notification,
) {
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

    let total_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM notifications
        WHERE account_id = $1 AND user_id = $2
        "#,
    )
    .bind(account_id)
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let type_str = notification_type_to_str(notif.notification_type).to_string();

    let push_data = json!({
        "id": notif.id,
        "notification_type": type_str,
        "primary_actor_type": notif.primary_actor_type,
        "primary_actor_id": notif.primary_actor_id,
        "read_at": notif.read_at,
        "created_at": notif.created_at.and_utc().timestamp(),
        "snoozed_until": notif.snoozed_until,
        "meta": notif.meta.clone().unwrap_or_else(|| json!({})),
        "account_id": account_id
    });

    event_dispatcher::dispatch_event(
        state,
        account_id as i32,
        "notification.updated",
        json!({
            "notification": push_data,
            "unread_count": unread_count,
            "count": total_count
        }),
    );
}

pub async fn dispatch_notification_deleted(
    state: &AppState,
    account_id: i64,
    user_id: i64,
    notif_id: i64,
) {
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

    let total_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM notifications
        WHERE account_id = $1 AND user_id = $2
        "#,
    )
    .bind(account_id)
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    event_dispatcher::dispatch_event(
        state,
        account_id as i32,
        "notification.deleted",
        json!({
            "notification": { "id": notif_id },
            "unread_count": unread_count,
            "count": total_count
        }),
    );
}
