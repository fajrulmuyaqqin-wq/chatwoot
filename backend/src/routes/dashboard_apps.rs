use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::middleware::auth::CurrentUser;
use crate::middleware::rbac::{verify_account_access, AccountRole};
use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct DashboardApp {
    pub id: i64,
    pub title: String,
    pub content: Option<serde_json::Value>,
    pub account_id: i64,
}

pub async fn list_dashboard_apps(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let apps = sqlx::query_as::<_, DashboardApp>(
        "SELECT id, title, content, account_id FROM dashboard_apps WHERE account_id = $1 ORDER BY id ASC",
    )
    .bind(account_id as i64)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    (StatusCode::OK, Json(json!(apps)))
}
