use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use tracing::warn;

use crate::models::conversation::Conversation;
use crate::models::productivity::Label;
use crate::services::event_dispatcher;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct LabelPayload {
    pub label: Option<LabelFields>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub color: Option<String>,
    pub show_on_sidebar: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct LabelFields {
    pub title: Option<String>,
    pub description: Option<String>,
    pub color: Option<String>,
    pub show_on_sidebar: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ConversationLabelsPayload {
    pub labels: Option<Vec<String>>,
}

/// GET /api/v1/accounts/:account_id/labels
pub async fn list_labels(
    Path(account_id): Path<i32>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let result = sqlx::query_as::<_, Label>(
        "SELECT * FROM labels WHERE account_id = $1 ORDER BY title ASC",
    )
    .bind(account_id as i64)
    .fetch_all(&state.db)
    .await;

    match result {
        Ok(list) => (StatusCode::OK, Json(json!({ "payload": list }))),
        Err(e) => {
            warn!("Failed to fetch labels: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Database error" })),
            )
        }
    }
}

/// POST /api/v1/accounts/:account_id/labels
pub async fn create_label(
    Path(account_id): Path<i32>,
    State(state): State<AppState>,
    Json(body): Json<LabelPayload>,
) -> impl IntoResponse {
    let title = body
        .label
        .as_ref()
        .and_then(|l| l.title.clone())
        .or(body.title)
        .unwrap_or_default()
        .to_lowercase();

    let description = body
        .label
        .as_ref()
        .and_then(|l| l.description.clone())
        .or(body.description);

    let color = body
        .label
        .as_ref()
        .and_then(|l| l.color.clone())
        .or(body.color)
        .unwrap_or_else(|| "#1f93ff".to_string());

    let show_on_sidebar = body
        .label
        .as_ref()
        .and_then(|l| l.show_on_sidebar)
        .or(body.show_on_sidebar)
        .unwrap_or(true);

    if title.trim().is_empty() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "title is required" })),
        );
    }

    let result = sqlx::query_as::<_, Label>(
        r#"
        INSERT INTO labels (account_id, title, description, color, show_on_sidebar, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, NOW(), NOW())
        RETURNING *
        "#,
    )
    .bind(account_id as i64)
    .bind(&title)
    .bind(description)
    .bind(&color)
    .bind(show_on_sidebar)
    .fetch_one(&state.db)
    .await;

    match result {
        Ok(created) => (StatusCode::OK, Json(json!({ "payload": created }))),
        Err(e) => {
            warn!("Failed to create label: {}", e);
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": format!("Could not create label: {}", e) })),
            )
        }
    }
}

/// PUT/PATCH /api/v1/accounts/:account_id/labels/:id
pub async fn update_label(
    Path((account_id, id)): Path<(i32, i64)>,
    State(state): State<AppState>,
    Json(body): Json<LabelPayload>,
) -> impl IntoResponse {
    let title = body
        .label
        .as_ref()
        .and_then(|l| l.title.clone())
        .or(body.title)
        .map(|t| t.to_lowercase());

    let description = body
        .label
        .as_ref()
        .and_then(|l| l.description.clone())
        .or(body.description);

    let color = body
        .label
        .as_ref()
        .and_then(|l| l.color.clone())
        .or(body.color);

    let show_on_sidebar = body
        .label
        .as_ref()
        .and_then(|l| l.show_on_sidebar)
        .or(body.show_on_sidebar);

    let result = sqlx::query_as::<_, Label>(
        r#"
        UPDATE labels
        SET title = COALESCE($1, title),
            description = COALESCE($2, description),
            color = COALESCE($3, color),
            show_on_sidebar = COALESCE($4, show_on_sidebar),
            updated_at = NOW()
        WHERE id = $5 AND account_id = $6
        RETURNING *
        "#,
    )
    .bind(title)
    .bind(description)
    .bind(color)
    .bind(show_on_sidebar)
    .bind(id)
    .bind(account_id as i64)
    .fetch_optional(&state.db)
    .await;

    match result {
        Ok(Some(updated)) => (StatusCode::OK, Json(json!({ "payload": updated }))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Label not found" })),
        ),
        Err(e) => {
            warn!("Failed to update label: {}", e);
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": format!("Could not update label: {}", e) })),
            )
        }
    }
}

/// DELETE /api/v1/accounts/:account_id/labels/:id
pub async fn delete_label(
    Path((account_id, id)): Path<(i32, i64)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let res = sqlx::query("DELETE FROM labels WHERE id = $1 AND account_id = $2")
        .bind(id)
        .bind(account_id as i64)
        .execute(&state.db)
        .await;

    match res {
        Ok(_) => (StatusCode::OK, Json(json!({ "status": "ok" }))),
        Err(e) => {
            warn!("Failed to delete label: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to delete" })),
            )
        }
    }
}

/// GET /api/v1/accounts/:account_id/conversations/:display_id/labels
pub async fn get_conversation_labels(
    Path((account_id, display_id)): Path<(i32, i32)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let conv = sqlx::query_as::<_, Conversation>(
        "SELECT * FROM conversations WHERE account_id = $1 AND display_id = $2",
    )
    .bind(account_id)
    .bind(display_id)
    .fetch_optional(&state.db)
    .await;

    let conv = match conv {
        Ok(Some(c)) => c,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Conversation not found" })),
            );
        }
    };

    let tags = sqlx::query_scalar::<_, String>(
        r#"
        SELECT t.name FROM tags t
        JOIN taggings tg ON t.id = tg.tag_id
        WHERE tg.taggable_type = 'Conversation' AND tg.taggable_id = $1
        ORDER BY t.name ASC
        "#,
    )
    .bind(conv.id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    (StatusCode::OK, Json(json!({ "payload": tags })))
}

/// POST /api/v1/accounts/:account_id/conversations/:display_id/labels
pub async fn set_conversation_labels(
    Path((account_id, display_id)): Path<(i32, i32)>,
    State(state): State<AppState>,
    Json(body): Json<ConversationLabelsPayload>,
) -> impl IntoResponse {
    let conv = sqlx::query_as::<_, Conversation>(
        "SELECT * FROM conversations WHERE account_id = $1 AND display_id = $2",
    )
    .bind(account_id)
    .bind(display_id)
    .fetch_optional(&state.db)
    .await;

    let conv = match conv {
        Ok(Some(c)) => c,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Conversation not found" })),
            );
        }
    };

    let new_labels = body.labels.unwrap_or_default();

    // Hapus tagging lama untuk percakapan ini
    let _ = sqlx::query(
        "DELETE FROM taggings WHERE taggable_type = 'Conversation' AND taggable_id = $1",
    )
    .bind(conv.id)
    .execute(&state.db)
    .await;

    for label_name in &new_labels {
        let clean_name = label_name.trim().to_lowercase();
        if clean_name.is_empty() {
            continue;
        }

        // Cari atau buat tag
        let tag_id = match sqlx::query_scalar::<_, i32>(
            r#"
            INSERT INTO tags (name, taggings_count)
            VALUES ($1, 1)
            ON CONFLICT (name) DO UPDATE
            SET taggings_count = tags.taggings_count + 1
            RETURNING id
            "#,
        )
        .bind(&clean_name)
        .fetch_one(&state.db)
        .await
        {
            Ok(id) => id,
            Err(_) => continue,
        };

        // Buat relasi di taggings
        let _ = sqlx::query(
            r#"
            INSERT INTO taggings (tag_id, taggable_type, taggable_id, context, created_at)
            VALUES ($1, 'Conversation', $2, 'labels', NOW())
            "#,
        )
        .bind(tag_id)
        .bind(conv.id)
        .execute(&state.db)
        .await;
    }

    // Broadcast ke ActionCable agar label langsung muncul di antarmuka Vue 3 agen!
    let updated_conv_val = json!({
        "id": conv.id,
        "display_id": conv.display_id,
        "labels": new_labels
    });
    event_dispatcher::dispatch_event(&state, account_id, "conversation.updated", updated_conv_val);

    (StatusCode::OK, Json(json!({ "payload": new_labels })))
}
