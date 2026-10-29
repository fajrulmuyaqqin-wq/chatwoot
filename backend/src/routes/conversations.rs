use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::middleware::auth::CurrentUser;
use crate::models::contact::{Contact, ContactResponse};
use crate::models::conversation::{
    Conversation, ConversationCounts, ConversationListPayload, ConversationListResponse,
};
use crate::models::message::{Message, MessageResponse};
use crate::services::event_dispatcher;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ConversationQuery {
    pub status: Option<String>,
    pub assignee_type: Option<String>,
    pub page: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ToggleStatusPayload {
    pub status: Option<String>,
}

pub async fn list_conversations(
    Path(account_id): Path<i32>,
    Query(query): Query<ConversationQuery>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let page = query.page.unwrap_or(1).max(1);
    let per_page: i64 = 25;
    let offset = (page - 1) * per_page;

    let status_filter = query.status.as_deref().unwrap_or("open");
    let assignee_type = query.assignee_type.as_deref().unwrap_or("all");

    // Periksa izin RBAC granular jika user memiliki custom role
    let user_perms = crate::middleware::rbac::get_account_user_permissions(&state.db, account_id, current_user.id).await.ok();
    let has_custom_role = user_perms.as_ref().map(|p| !p.is_administrator() && p.custom_role_id.is_some()).unwrap_or(false);
    let perms = user_perms.unwrap_or(crate::middleware::rbac::AccountUserPermissions {
        role: 0,
        custom_role_id: None,
        custom_role_name: None,
        permissions: vec![],
    });

    let can_manage_all = !has_custom_role || perms.has_permission("conversation_manage");
    let can_manage_unassigned = has_custom_role && perms.has_permission("conversation_unassigned_manage");
    let can_manage_participating = has_custom_role && perms.has_permission("conversation_participating_manage");

    if has_custom_role && !can_manage_all && !can_manage_unassigned && !can_manage_participating {
        return (StatusCode::OK, Json(ConversationListResponse {
            data: ConversationListPayload {
                meta: ConversationCounts {
                    mine_count: 0,
                    unassigned_count: 0,
                    assigned_count: 0,
                    all_count: 0,
                },
                payload: vec![],
            },
        }));
    }

    // Hitung counts untuk meta dengan granular permission filter
    let all_count: (i64,) = if !can_manage_all {
        if can_manage_unassigned {
            sqlx::query_as(
                "SELECT count(*) FROM conversations WHERE account_id = $1 AND status = 0 AND (assignee_id IS NULL OR assignee_id = $2)",
            )
            .bind(account_id)
            .bind(current_user.id)
            .fetch_one(&state.db)
            .await
            .unwrap_or((0,))
        } else {
            sqlx::query_as(
                r#"
                SELECT count(*) FROM conversations
                WHERE account_id = $1 AND status = 0
                  AND (assignee_id = $2 OR id IN (SELECT conversation_id FROM conversation_participants WHERE account_id = $1 AND user_id = $2))
                "#,
            )
            .bind(account_id)
            .bind(current_user.id)
            .fetch_one(&state.db)
            .await
            .unwrap_or((0,))
        }
    } else {
        sqlx::query_as(
            "SELECT count(*) FROM conversations WHERE account_id = $1 AND status = 0",
        )
        .bind(account_id)
        .fetch_one(&state.db)
        .await
        .unwrap_or((0,))
    };

    let unassigned_count: (i64,) = if !can_manage_all && !can_manage_unassigned {
        (0,)
    } else {
        sqlx::query_as(
            "SELECT count(*) FROM conversations WHERE account_id = $1 AND status = 0 AND assignee_id IS NULL",
        )
        .bind(account_id)
        .fetch_one(&state.db)
        .await
        .unwrap_or((0,))
    };

    let assigned_count: (i64,) = if !can_manage_all {
        if can_manage_unassigned {
            sqlx::query_as(
                "SELECT count(*) FROM conversations WHERE account_id = $1 AND status = 0 AND assignee_id = $2",
            )
            .bind(account_id)
            .bind(current_user.id)
            .fetch_one(&state.db)
            .await
            .unwrap_or((0,))
        } else {
            sqlx::query_as(
                r#"
                SELECT count(*) FROM conversations
                WHERE account_id = $1 AND status = 0
                  AND (assignee_id = $2 OR id IN (SELECT conversation_id FROM conversation_participants WHERE account_id = $1 AND user_id = $2))
                "#,
            )
            .bind(account_id)
            .bind(current_user.id)
            .fetch_one(&state.db)
            .await
            .unwrap_or((0,))
        }
    } else {
        sqlx::query_as(
            "SELECT count(*) FROM conversations WHERE account_id = $1 AND status = 0 AND assignee_id IS NOT NULL",
        )
        .bind(account_id)
        .fetch_one(&state.db)
        .await
        .unwrap_or((0,))
    };

    let mine_count: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM conversations WHERE account_id = $1 AND status = 0 AND assignee_id = $2",
    )
    .bind(account_id)
    .bind(current_user.id)
    .fetch_one(&state.db)
    .await
    .unwrap_or((0,));

    let status_int = match status_filter {
        "resolved" => 1,
        "pending" => 2,
        "snoozed" => 3,
        _ => 0, // default "open"
    };

    let conversations = if !can_manage_all {
        if can_manage_unassigned {
            match assignee_type {
                "unassigned" => {
                    sqlx::query_as::<_, Conversation>(
                        r#"
                        SELECT * FROM conversations
                        WHERE account_id = $1 AND status = $2 AND assignee_id IS NULL
                        ORDER BY last_activity_at DESC
                        LIMIT $3 OFFSET $4
                        "#,
                    )
                    .bind(account_id)
                    .bind(status_int)
                    .bind(per_page)
                    .bind(offset)
                    .fetch_all(&state.db)
                    .await
                    .unwrap_or_default()
                }
                "me" => {
                    sqlx::query_as::<_, Conversation>(
                        r#"
                        SELECT * FROM conversations
                        WHERE account_id = $1 AND status = $2 AND assignee_id = $3
                        ORDER BY last_activity_at DESC
                        LIMIT $4 OFFSET $5
                        "#,
                    )
                    .bind(account_id)
                    .bind(status_int)
                    .bind(current_user.id)
                    .bind(per_page)
                    .bind(offset)
                    .fetch_all(&state.db)
                    .await
                    .unwrap_or_default()
                }
                _ => {
                    sqlx::query_as::<_, Conversation>(
                        r#"
                        SELECT * FROM conversations
                        WHERE account_id = $1 AND status = $2
                          AND (assignee_id IS NULL OR assignee_id = $3)
                        ORDER BY last_activity_at DESC
                        LIMIT $4 OFFSET $5
                        "#,
                    )
                    .bind(account_id)
                    .bind(status_int)
                    .bind(current_user.id)
                    .bind(per_page)
                    .bind(offset)
                    .fetch_all(&state.db)
                    .await
                    .unwrap_or_default()
                }
            }
        } else {
            // can_manage_participating
            match assignee_type {
                "unassigned" => vec![],
                "me" => {
                    sqlx::query_as::<_, Conversation>(
                        r#"
                        SELECT * FROM conversations
                        WHERE account_id = $1 AND status = $2 AND assignee_id = $3
                        ORDER BY last_activity_at DESC
                        LIMIT $4 OFFSET $5
                        "#,
                    )
                    .bind(account_id)
                    .bind(status_int)
                    .bind(current_user.id)
                    .bind(per_page)
                    .bind(offset)
                    .fetch_all(&state.db)
                    .await
                    .unwrap_or_default()
                }
                _ => {
                    sqlx::query_as::<_, Conversation>(
                        r#"
                        SELECT * FROM conversations
                        WHERE account_id = $1 AND status = $2
                          AND (assignee_id = $3 OR id IN (SELECT conversation_id FROM conversation_participants WHERE account_id = $1 AND user_id = $3))
                        ORDER BY last_activity_at DESC
                        LIMIT $4 OFFSET $5
                        "#,
                    )
                    .bind(account_id)
                    .bind(status_int)
                    .bind(current_user.id)
                    .bind(per_page)
                    .bind(offset)
                    .fetch_all(&state.db)
                    .await
                    .unwrap_or_default()
                }
            }
        }
    } else if status_filter == "all" {
        sqlx::query_as::<_, Conversation>(
            r#"
            SELECT * FROM conversations
            WHERE account_id = $1
            ORDER BY last_activity_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(account_id)
        .bind(per_page)
        .bind(offset)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default()
    } else {
        match assignee_type {
            "unassigned" => {
                sqlx::query_as::<_, Conversation>(
                    r#"
                    SELECT * FROM conversations
                    WHERE account_id = $1 AND status = $2 AND assignee_id IS NULL
                    ORDER BY last_activity_at DESC
                    LIMIT $3 OFFSET $4
                    "#,
                )
                .bind(account_id)
                .bind(status_int)
                .bind(per_page)
                .bind(offset)
                .fetch_all(&state.db)
                .await
                .unwrap_or_default()
            }
            "me" => {
                sqlx::query_as::<_, Conversation>(
                    r#"
                    SELECT * FROM conversations
                    WHERE account_id = $1 AND status = $2 AND assignee_id = $3
                    ORDER BY last_activity_at DESC
                    LIMIT $4 OFFSET $5
                    "#,
                )
                .bind(account_id)
                .bind(status_int)
                .bind(current_user.id)
                .bind(per_page)
                .bind(offset)
                .fetch_all(&state.db)
                .await
                .unwrap_or_default()
            }
            _ => {
                sqlx::query_as::<_, Conversation>(
                    r#"
                    SELECT * FROM conversations
                    WHERE account_id = $1 AND status = $2
                    ORDER BY last_activity_at DESC
                    LIMIT $3 OFFSET $4
                    "#,
                )
                .bind(account_id)
                .bind(status_int)
                .bind(per_page)
                .bind(offset)
                .fetch_all(&state.db)
                .await
                .unwrap_or_default()
            }
        }
    };

    let mut payload = Vec::new();
    for conv in conversations {
        let sender = if let Some(contact_id) = conv.contact_id {
            match sqlx::query_as::<_, Contact>("SELECT * FROM contacts WHERE id = $1")
                .bind(contact_id as i32)
                .fetch_optional(&state.db)
                .await
            {
                Ok(c) => c.map(ContactResponse::from),
                Err(e) => {
                    tracing::error!("Error querying contact id {}: {:?}", contact_id, e);
                    None
                }
            }
        } else {
            None
        };

        let channel_type = sqlx::query_scalar::<_, Option<String>>(
            "SELECT channel_type FROM inboxes WHERE id = $1",
        )
        .bind(conv.inbox_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .flatten()
        .unwrap_or_else(|| "Channel::Api".to_string());

        let last_msg: Option<MessageResponse> = sqlx::query_as::<_, Message>(
            "SELECT * FROM messages WHERE conversation_id = $1 ORDER BY created_at DESC LIMIT 1",
        )
        .bind(conv.id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .map(|m| m.to_response(conv.display_id, None, vec![]));

        payload.push(conv.to_response(sender, channel_type, None, last_msg, 0));
    }

    let response = ConversationListResponse {
        data: ConversationListPayload {
            meta: ConversationCounts {
                mine_count: mine_count.0,
                unassigned_count: unassigned_count.0,
                assigned_count: assigned_count.0,
                all_count: all_count.0,
            },
            payload,
        },
    };

    (StatusCode::OK, Json(response))
}

pub async fn get_conversation(
    Path((account_id, display_id)): Path<(i32, i32)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let conv = match sqlx::query_as::<_, Conversation>(
        "SELECT * FROM conversations WHERE account_id = $1 AND display_id = $2",
    )
    .bind(account_id)
    .bind(display_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(c)) => c,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Conversation not found" })),
            );
        }
    };

    // Granular permission check jika user memiliki custom role
    if let Ok(perms) = crate::middleware::rbac::get_account_user_permissions(&state.db, account_id, current_user.id).await {
        if !perms.is_administrator() && perms.custom_role_id.is_some() {
            let can_all = perms.has_permission("conversation_manage");
            let can_unassigned = perms.has_permission("conversation_unassigned_manage");
            let can_participating = perms.has_permission("conversation_participating_manage");

            let has_access = if can_all {
                true
            } else if can_unassigned {
                conv.assignee_id.is_none() || conv.assignee_id == Some(current_user.id as i32)
            } else if can_participating {
                if conv.assignee_id == Some(current_user.id as i32) {
                    true
                } else {
                    let is_part: bool = sqlx::query_scalar::<_, bool>(
                        "SELECT EXISTS(SELECT 1 FROM conversation_participants WHERE account_id = $1 AND conversation_id = $2 AND user_id = $3)",
                    )
                    .bind(account_id as i64)
                    .bind(conv.id as i64)
                    .bind(current_user.id as i64)
                    .fetch_one(&state.db)
                    .await
                    .unwrap_or(false);
                    is_part
                }
            } else {
                false
            };

            if !has_access {
                return (
                    StatusCode::FORBIDDEN,
                    Json(json!({ "error": "Access denied by custom role permissions" })),
                );
            }
        }
    }

    let sender = if let Some(contact_id) = conv.contact_id {
        sqlx::query_as::<_, Contact>("SELECT * FROM contacts WHERE id = $1")
            .bind(contact_id as i32)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .map(ContactResponse::from)
    } else {
        None
    };

    let channel_type = sqlx::query_scalar::<_, Option<String>>(
        "SELECT channel_type FROM inboxes WHERE id = $1",
    )
    .bind(conv.inbox_id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten()
    .flatten()
    .unwrap_or_else(|| "Channel::Api".to_string());

    let last_msg: Option<MessageResponse> = sqlx::query_as::<_, Message>(
        "SELECT * FROM messages WHERE conversation_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(conv.id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten()
    .map(|m| m.to_response(conv.display_id, None, vec![]));

    let resp = conv.to_response(sender, channel_type, None, last_msg, 0);

    (
        StatusCode::OK,
        Json(serde_json::to_value(&resp).unwrap_or_default()),
    )
}

pub async fn toggle_status(
    Path((account_id, display_id)): Path<(i32, i32)>,
    State(state): State<AppState>,
    body: Option<Json<ToggleStatusPayload>>,
) -> impl IntoResponse {
    let conv = match sqlx::query_as::<_, Conversation>(
        "SELECT * FROM conversations WHERE account_id = $1 AND display_id = $2",
    )
    .bind(account_id)
    .bind(display_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(c)) => c,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Conversation not found" })),
            );
        }
    };

    let new_status = if let Some(Json(b)) = body {
        match b.status.as_deref() {
            Some("resolved") => 1,
            Some("open") => 0,
            Some("pending") => 2,
            Some("snoozed") => 3,
            _ => if conv.status == 0 { 1 } else { 0 },
        }
    } else if conv.status == 0 {
        1 // Open -> Resolved
    } else {
        0 // Resolved -> Open
    };

    let status_str = match new_status {
        1 => "resolved",
        2 => "pending",
        3 => "snoozed",
        _ => "open",
    };

    let _ = sqlx::query(
        "UPDATE conversations SET status = $1, status_changed_at = NOW(), updated_at = NOW() WHERE id = $2",
    )
    .bind(new_status)
    .bind(conv.id)
    .execute(&state.db)
    .await;

    // Broadcast status change ke seluruh agen via ActionCable
    let broadcast_payload = json!({
        "id": conv.display_id,
        "account_id": conv.account_id,
        "status": status_str,
        "current_status": status_str,
        "updated_at": chrono::Utc::now().timestamp()
    });

    event_dispatcher::dispatch_event(
        &state,
        account_id,
        "conversation.status_changed",
        broadcast_payload.clone(),
    );
    event_dispatcher::dispatch_event(
        &state,
        account_id,
        "conversation.updated",
        broadcast_payload.clone(),
    );

    let db_wh = state.db.clone();
    let wh_payload = broadcast_payload;
    tokio::spawn(async move {
        crate::services::webhook_dispatcher::dispatch_webhook_event(
            &db_wh,
            account_id as i64,
            "conversation_status_changed",
            wh_payload,
        )
        .await;
    });

    (
        StatusCode::OK,
        Json(json!({
            "payload": {
                "success": true,
                "conversation_id": conv.display_id,
                "current_status": status_str,
                "snoozed_until": null
            }
        })),
    )
}

#[derive(Debug, Deserialize)]
pub struct AssignmentPayload {
    pub assignee_id: Option<serde_json::Value>,
    pub team_id: Option<serde_json::Value>,
}

pub async fn assign_conversation(
    Path((account_id, display_id)): Path<(i32, i32)>,
    State(state): State<AppState>,
    Json(payload): Json<AssignmentPayload>,
) -> impl IntoResponse {
    let conv = match sqlx::query_as::<_, Conversation>(
        "SELECT * FROM conversations WHERE account_id = $1 AND display_id = $2",
    )
    .bind(account_id)
    .bind(display_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(c)) => c,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Conversation not found" })),
            )
                .into_response();
        }
    };

    if let Some(ref assignee_val) = payload.assignee_id {
        let new_assignee: Option<i32> = match assignee_val {
            serde_json::Value::Number(n) => n.as_i64().map(|i| i as i32),
            serde_json::Value::String(s) => s.parse::<i32>().ok(),
            _ => None,
        };

        let _ = sqlx::query("UPDATE conversations SET assignee_id = $1, updated_at = NOW() WHERE id = $2")
            .bind(new_assignee)
            .bind(conv.id)
            .execute(&state.db)
            .await;

        if let Some(uid) = new_assignee {
            let st = state.clone();
            let cid = conv.id;
            tokio::spawn(async move {
                let _ = crate::services::notification_dispatcher::create_and_dispatch_notification(
                    &st,
                    account_id as i64,
                    uid as i64,
                    2, // conversation_assignment
                    cid as i64,
                    "Conversation",
                    None,
                    None,
                    None,
                )
                .await;
            });
        }

        let broadcast_payload = json!({
            "id": conv.display_id,
            "account_id": conv.account_id,
            "assignee_id": new_assignee,
            "updated_at": chrono::Utc::now().timestamp()
        });

        event_dispatcher::dispatch_event(
            &state,
            account_id,
            "conversation.updated",
            broadcast_payload.clone(),
        );

        return (
            StatusCode::OK,
            Json(json!({
                "id": new_assignee,
                "conversation_id": conv.display_id
            })),
        )
            .into_response();
    }

    if let Some(ref team_val) = payload.team_id {
        let new_team: Option<i32> = match team_val {
            serde_json::Value::Number(n) => n.as_i64().map(|i| i as i32),
            serde_json::Value::String(s) => s.parse::<i32>().ok(),
            _ => None,
        };

        let _ = sqlx::query("UPDATE conversations SET team_id = $1, updated_at = NOW() WHERE id = $2")
            .bind(new_team)
            .bind(conv.id)
            .execute(&state.db)
            .await;

        let broadcast_payload = json!({
            "id": conv.display_id,
            "account_id": conv.account_id,
            "team_id": new_team,
            "updated_at": chrono::Utc::now().timestamp()
        });

        event_dispatcher::dispatch_event(
            &state,
            account_id,
            "conversation.updated",
            broadcast_payload,
        );

        return (
            StatusCode::OK,
            Json(json!({
                "id": new_team,
                "conversation_id": conv.display_id
            })),
        )
            .into_response();
    }

    (StatusCode::OK, Json(json!(null))).into_response()
}
