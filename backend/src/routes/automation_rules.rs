use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use tracing::warn;

use crate::models::automation_rule::{
    AutomationRule, AutomationRuleListResponse, AutomationRuleResponse,
    CreateAutomationRulePayload, UpdateAutomationRulePayload,
};
use crate::state::AppState;

/// GET /api/v1/accounts/:account_id/automation_rules
pub async fn list_automation_rules(
    Path(account_id): Path<i64>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let rules = match sqlx::query_as::<_, AutomationRule>(
        "SELECT * FROM automation_rules WHERE account_id = $1 ORDER BY id DESC",
    )
    .bind(account_id)
    .fetch_all(&state.db)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to fetch automation rules for account {}: {}", account_id, e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Database error fetching automation rules" })),
            )
                .into_response();
        }
    };

    let responses: Vec<AutomationRuleResponse> = rules.into_iter().map(Into::into).collect();
    Json(AutomationRuleListResponse { payload: responses }).into_response()
}

/// POST /api/v1/accounts/:account_id/automation_rules
pub async fn create_automation_rule(
    Path(account_id): Path<i64>,
    State(state): State<AppState>,
    Json(payload): Json<CreateAutomationRulePayload>,
) -> impl IntoResponse {
    let conditions = payload.conditions.unwrap_or_else(|| json!([]));
    let actions = payload.actions.unwrap_or_else(|| json!([]));
    let active = payload.active.unwrap_or(true);

    let rule = match sqlx::query_as::<_, AutomationRule>(
        r#"
        INSERT INTO automation_rules (
            account_id, name, description, event_name, conditions, actions,
            active, execution_delay, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW(), NOW())
        RETURNING *
        "#,
    )
    .bind(account_id)
    .bind(&payload.name)
    .bind(&payload.description)
    .bind(&payload.event_name)
    .bind(&conditions)
    .bind(&actions)
    .bind(active)
    .bind(payload.execution_delay)
    .fetch_one(&state.db)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to create automation rule for account {}: {}", account_id, e);
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": format!("Could not create automation rule: {}", e) })),
            )
                .into_response();
        }
    };

    let resp: AutomationRuleResponse = rule.into();
    (StatusCode::OK, Json(resp)).into_response()
}

/// GET /api/v1/accounts/:account_id/automation_rules/:id
pub async fn get_automation_rule(
    Path((account_id, id)): Path<(i64, i64)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let rule = match sqlx::query_as::<_, AutomationRule>(
        "SELECT * FROM automation_rules WHERE id = $1 AND account_id = $2",
    )
    .bind(id)
    .bind(account_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(r)) => r,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Automation rule not found" })),
            )
                .into_response();
        }
        Err(e) => {
            warn!("DB error fetching automation rule {}: {}", id, e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Database error" })),
            )
                .into_response();
        }
    };

    let resp: AutomationRuleResponse = rule.into();
    Json(resp).into_response()
}

/// PATCH /api/v1/accounts/:account_id/automation_rules/:id
pub async fn update_automation_rule(
    Path((account_id, id)): Path<(i64, i64)>,
    State(state): State<AppState>,
    Json(payload): Json<UpdateAutomationRulePayload>,
) -> impl IntoResponse {
    let existing = match sqlx::query_as::<_, AutomationRule>(
        "SELECT * FROM automation_rules WHERE id = $1 AND account_id = $2",
    )
    .bind(id)
    .bind(account_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(r)) => r,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Automation rule not found" })),
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

    let name = payload.name.unwrap_or(existing.name);
    let description = payload.description.or(existing.description);
    let event_name = payload.event_name.unwrap_or(existing.event_name);
    let conditions = payload.conditions.unwrap_or(existing.conditions);
    let actions = payload.actions.unwrap_or(existing.actions);
    let active = payload.active.unwrap_or(existing.active);
    let execution_delay = payload.execution_delay.or(existing.execution_delay);

    let updated = match sqlx::query_as::<_, AutomationRule>(
        r#"
        UPDATE automation_rules
        SET name = $1, description = $2, event_name = $3, conditions = $4,
            actions = $5, active = $6, execution_delay = $7, updated_at = NOW()
        WHERE id = $8 AND account_id = $9
        RETURNING *
        "#,
    )
    .bind(&name)
    .bind(&description)
    .bind(&event_name)
    .bind(&conditions)
    .bind(&actions)
    .bind(active)
    .bind(execution_delay)
    .bind(id)
    .bind(account_id)
    .fetch_one(&state.db)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let resp: AutomationRuleResponse = updated.into();
    Json(resp).into_response()
}

/// POST /api/v1/accounts/:account_id/automation_rules/:id/clone
pub async fn clone_automation_rule(
    Path((account_id, id)): Path<(i64, i64)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let existing = match sqlx::query_as::<_, AutomationRule>(
        "SELECT * FROM automation_rules WHERE id = $1 AND account_id = $2",
    )
    .bind(id)
    .bind(account_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(r)) => r,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Automation rule not found" })),
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

    let cloned_name = format!("{} (Copy)", existing.name);

    let cloned = match sqlx::query_as::<_, AutomationRule>(
        r#"
        INSERT INTO automation_rules (
            account_id, name, description, event_name, conditions, actions,
            active, execution_delay, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW(), NOW())
        RETURNING *
        "#,
    )
    .bind(account_id)
    .bind(&cloned_name)
    .bind(&existing.description)
    .bind(&existing.event_name)
    .bind(&existing.conditions)
    .bind(&existing.actions)
    .bind(existing.active)
    .bind(existing.execution_delay)
    .fetch_one(&state.db)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let resp: AutomationRuleResponse = cloned.into();
    Json(resp).into_response()
}

/// DELETE /api/v1/accounts/:account_id/automation_rules/:id
pub async fn delete_automation_rule(
    Path((account_id, id)): Path<(i64, i64)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let res = sqlx::query("DELETE FROM automation_rules WHERE id = $1 AND account_id = $2")
        .bind(id)
        .bind(account_id)
        .execute(&state.db)
        .await;

    match res {
        Ok(r) if r.rows_affected() > 0 => StatusCode::OK.into_response(),
        Ok(_) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Automation rule not found" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}
