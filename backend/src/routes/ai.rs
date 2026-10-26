use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::models::article::{Article, ArticleResponse};
use crate::services::ai_rag;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateArticlePayload {
    pub title: String,
    pub description: Option<String>,
    pub content: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ReplySuggestionPayload {
    pub message: String,
}

/// GET /api/v1/accounts/:account_id/articles
pub async fn list_articles(
    Path(account_id): Path<i32>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let articles = sqlx::query_as::<_, Article>(
        "SELECT * FROM articles WHERE account_id = $1 ORDER BY id DESC",
    )
    .bind(account_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let payload: Vec<ArticleResponse> = articles.into_iter().map(ArticleResponse::from).collect();
    (StatusCode::OK, Json(json!({ "payload": payload })))
}

/// POST /api/v1/accounts/:account_id/articles
pub async fn create_article(
    Path(account_id): Path<i32>,
    State(state): State<AppState>,
    Json(payload): Json<CreateArticlePayload>,
) -> impl IntoResponse {
    let article = match sqlx::query_as::<_, Article>(
        r#"
        INSERT INTO articles (
            account_id, title, description, content, status, views, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, 1, 0, NOW(), NOW())
        RETURNING *
        "#,
    )
    .bind(account_id)
    .bind(&payload.title)
    .bind(&payload.description)
    .bind(&payload.content)
    .fetch_one(&state.db)
    .await
    {
        Ok(a) => a,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Failed to create article: {}", e) })),
            );
        }
    };

    (StatusCode::CREATED, Json(json!(ArticleResponse::from(article))))
}

/// POST /api/v1/accounts/:account_id/ai/reply_suggestions
pub async fn get_reply_suggestion(
    Path(account_id): Path<i32>,
    State(state): State<AppState>,
    Json(payload): Json<ReplySuggestionPayload>,
) -> impl IntoResponse {
    let llm_provider = crate::services::llm::get_llm_provider(None, None, None, &state.config);
    match ai_rag::generate_reply_suggestion(
        &state.db,
        account_id,
        &payload.message,
        llm_provider,
    )
    .await
    {
        Ok(reply) => (StatusCode::OK, Json(json!({ "reply_suggestion": reply }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e })),
        ),
    }
}
