use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;

use crate::middleware::auth::CurrentUser;
use crate::middleware::rbac::{verify_account_access, AccountRole};
use crate::state::AppState;

/// GET /enterprise/api/v1/accounts/:account_id/limits
pub async fn get_account_limits(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let agent_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM account_users WHERE account_id = $1")
        .bind(account_id as i64)
        .fetch_one(&state.db)
        .await
        .unwrap_or(1);

    let conversation_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM conversations WHERE account_id = $1")
        .bind(account_id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    let inbox_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM inboxes WHERE account_id = $1")
        .bind(account_id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    (
        StatusCode::OK,
        Json(json!({
            "id": account_id,
            "limits": {
                "conversation": {
                    "allowed": 100000,
                    "consumed": conversation_count
                },
                "non_web_inboxes": {
                    "allowed": 100,
                    "consumed": inbox_count
                },
                "agents": {
                    "allowed": 100,
                    "consumed": agent_count
                }
            }
        })),
    )
}
