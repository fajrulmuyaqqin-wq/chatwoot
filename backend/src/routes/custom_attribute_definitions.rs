use crate::models::custom_attribute_definition::{
    CustomAttributeDefinition, CustomAttributePayload,
};
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use tracing::warn;

#[derive(Debug, Deserialize)]
pub struct CustomAttrQuery {
    pub attribute_model: Option<String>,
}

/// GET /api/v1/accounts/:account_id/custom_attribute_definitions
pub async fn list_custom_attribute_definitions(
    Path(account_id): Path<i32>,
    Query(query): Query<CustomAttrQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let model_code: Option<i32> = query.attribute_model.as_deref().map(|m| match m {
        "conversation_attribute" => 0,
        "contact_attribute" => 1,
        val => val.parse::<i32>().unwrap_or(0),
    });

    let items = if let Some(m) = model_code {
        sqlx::query_as::<_, CustomAttributeDefinition>(
            r#"
            SELECT * FROM custom_attribute_definitions
            WHERE account_id = $1 AND attribute_model = $2
            ORDER BY id ASC
            "#,
        )
        .bind(account_id as i64)
        .bind(m)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default()
    } else {
        sqlx::query_as::<_, CustomAttributeDefinition>(
            r#"
            SELECT * FROM custom_attribute_definitions
            WHERE account_id = $1
            ORDER BY id ASC
            "#,
        )
        .bind(account_id as i64)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default()
    };

    (StatusCode::OK, Json(json!(items)))
}

/// POST /api/v1/accounts/:account_id/custom_attribute_definitions
pub async fn create_custom_attribute_definition(
    Path(account_id): Path<i32>,
    State(state): State<AppState>,
    Json(body): Json<CustomAttributePayload>,
) -> impl IntoResponse {
    let result = sqlx::query_as::<_, CustomAttributeDefinition>(
        r#"
        INSERT INTO custom_attribute_definitions (
            attribute_display_name, attribute_key, attribute_display_type,
            attribute_model, account_id, attribute_description,
            attribute_values, regex_pattern, regex_cue,
            created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW(), NOW())
        RETURNING *
        "#,
    )
    .bind(&body.attribute_display_name)
    .bind(&body.attribute_key)
    .bind(body.attribute_display_type.unwrap_or(0))
    .bind(body.attribute_model.unwrap_or(0))
    .bind(account_id as i64)
    .bind(body.attribute_description)
    .bind(body.attribute_values.unwrap_or(json!([])))
    .bind(body.regex_pattern)
    .bind(body.regex_cue)
    .fetch_one(&state.db)
    .await;

    match result {
        Ok(item) => (StatusCode::OK, Json(json!(item))),
        Err(e) => {
            warn!("Failed to create custom attribute definition: {}", e);
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": format!("Could not create attribute: {}", e) })),
            )
        }
    }
}

/// GET /api/v1/accounts/:account_id/custom_attribute_definitions/:id
pub async fn get_custom_attribute_definition(
    Path((account_id, id)): Path<(i32, i64)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let item = sqlx::query_as::<_, CustomAttributeDefinition>(
        "SELECT * FROM custom_attribute_definitions WHERE id = $1 AND account_id = $2",
    )
    .bind(id)
    .bind(account_id as i64)
    .fetch_optional(&state.db)
    .await;

    match item {
        Ok(Some(i)) => (StatusCode::OK, Json(json!(i))),
        _ => (StatusCode::NOT_FOUND, Json(json!({ "error": "Attribute not found" }))),
    }
}

/// PATCH /api/v1/accounts/:account_id/custom_attribute_definitions/:id
pub async fn update_custom_attribute_definition(
    Path((account_id, id)): Path<(i32, i64)>,
    State(state): State<AppState>,
    Json(body): Json<CustomAttributePayload>,
) -> impl IntoResponse {
    let result = sqlx::query_as::<_, CustomAttributeDefinition>(
        r#"
        UPDATE custom_attribute_definitions
        SET attribute_display_name = $1,
            attribute_description = COALESCE($2, attribute_description),
            attribute_values = COALESCE($3, attribute_values),
            regex_pattern = COALESCE($4, regex_pattern),
            regex_cue = COALESCE($5, regex_cue),
            updated_at = NOW()
        WHERE id = $6 AND account_id = $7
        RETURNING *
        "#,
    )
    .bind(&body.attribute_display_name)
    .bind(body.attribute_description)
    .bind(body.attribute_values)
    .bind(body.regex_pattern)
    .bind(body.regex_cue)
    .bind(id)
    .bind(account_id as i64)
    .fetch_optional(&state.db)
    .await;

    match result {
        Ok(Some(item)) => (StatusCode::OK, Json(json!(item))),
        Ok(None) => (StatusCode::NOT_FOUND, Json(json!({ "error": "Attribute not found" }))),
        Err(e) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

/// DELETE /api/v1/accounts/:account_id/custom_attribute_definitions/:id
pub async fn delete_custom_attribute_definition(
    Path((account_id, id)): Path<(i32, i64)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let _ = sqlx::query(
        "DELETE FROM custom_attribute_definitions WHERE id = $1 AND account_id = $2",
    )
    .bind(id)
    .bind(account_id as i64)
    .execute(&state.db)
    .await;

    (StatusCode::OK, Json(json!({ "status": "ok" })))
}
