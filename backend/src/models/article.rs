use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Article {
    pub id: i32,
    pub account_id: i32,
    pub portal_id: Option<i64>,
    pub category_id: Option<i64>,
    pub folder_id: Option<i64>,
    pub author_id: Option<i64>,
    pub title: String,
    pub description: Option<String>,
    pub content: Option<String>,
    pub status: Option<i32>,
    pub views: Option<i32>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArticleResponse {
    pub id: i32,
    pub title: String,
    pub description: Option<String>,
    pub content: Option<String>,
    pub status: i32,
    pub views: i32,
    pub created_at: i64,
}

impl From<Article> for ArticleResponse {
    fn from(a: Article) -> Self {
        Self {
            id: a.id,
            title: a.title,
            description: a.description,
            content: a.content,
            status: a.status.unwrap_or(1),
            views: a.views.unwrap_or(0),
            created_at: a.created_at.and_utc().timestamp(),
        }
    }
}
