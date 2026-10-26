use axum::{
    extract::{Extension, Path, Query, State},
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
pub struct Portal {
    pub id: i64,
    pub account_id: i32,
    pub name: String,
    pub slug: String,
    pub custom_domain: Option<String>,
    pub color: Option<String>,
    pub homepage_link: Option<String>,
    pub page_title: Option<String>,
    pub header_text: Option<String>,
    pub config: Option<serde_json::Value>,
    pub archived: Option<bool>,
    pub created_at: chrono::NaiveDateTime,
    pub updated_at: chrono::NaiveDateTime,
}

#[derive(Debug, Deserialize)]
pub struct CreatePortalPayload {
    pub name: Option<String>,
    pub slug: Option<String>,
    pub color: Option<String>,
    pub custom_domain: Option<String>,
    pub page_title: Option<String>,
    pub header_text: Option<String>,
    pub homepage_link: Option<String>,
    pub config: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ArticleListQuery {
    pub page_number: Option<i64>,
    pub locale: Option<String>,
    pub status: Option<String>,
    pub author_id: Option<i64>,
    pub category_slug: Option<String>,
    pub sort: Option<String>,
    pub query: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct CreateArticlePayload {
    pub title: Option<String>,
    pub content: Option<String>,
    pub description: Option<String>,
    pub status: Option<serde_json::Value>,
    pub author_id: Option<i64>,
    pub category_id: Option<i64>,
    pub locale: Option<String>,
    pub slug: Option<String>,
    pub meta: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Default)]
pub struct CategoryListQuery {
    pub locale: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct CreateCategoryPayload {
    pub name: Option<String>,
    pub description: Option<String>,
    pub slug: Option<String>,
    pub locale: Option<String>,
    pub icon: Option<String>,
    pub icon_color: Option<String>,
    pub position: Option<i32>,
    pub parent_category_id: Option<i64>,
    pub associated_category_id: Option<i64>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ReorderPayload {
    pub positions_hash: Option<serde_json::Value>,
}

fn portal_to_json(p: Portal) -> serde_json::Value {
    json!({
        "id": p.id,
        "account_id": p.account_id,
        "name": p.name,
        "slug": p.slug,
        "custom_domain": p.custom_domain,
        "color": p.color.unwrap_or_else(|| "#1f93ff".to_string()),
        "homepage_link": p.homepage_link,
        "page_title": p.page_title,
        "header_text": p.header_text,
        "archived": p.archived.unwrap_or(false),
        "config": p.config.unwrap_or_else(|| json!({
            "allowed_locales": ["en"],
            "default_locale": "en"
        })),
        "meta": {
            "all_articles_count": 0,
            "archived_articles_count": 0,
            "published_count": 0,
            "draft_articles_count": 0,
            "mine_articles_count": 0,
            "categories_count": 0
        }
    })
}

fn parse_status(val: Option<&serde_json::Value>) -> i32 {
    match val {
        Some(serde_json::Value::Number(n)) => n.as_i64().unwrap_or(1) as i32,
        Some(serde_json::Value::String(s)) => match s.as_str() {
            "draft" => 0,
            "published" => 1,
            "archived" => 2,
            _ => 1,
        },
        _ => 1,
    }
}

fn format_status_str(status: i32) -> &'static str {
    match status {
        0 => "draft",
        1 => "published",
        2 => "archived",
        _ => "published",
    }
}

// ======================== PORTALS ========================

pub async fn list_portals(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let portals = sqlx::query_as::<_, Portal>(
        r#"
        SELECT id, account_id, name, slug, custom_domain, color,
               homepage_link, page_title, header_text, config, archived,
               created_at, updated_at
        FROM portals
        WHERE account_id = $1
        ORDER BY id ASC
        "#,
    )
    .bind(account_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let count = portals.len();
    let payload: Vec<serde_json::Value> = portals.into_iter().map(portal_to_json).collect();

    (
        StatusCode::OK,
        Json(json!({
            "payload": payload,
            "meta": {
                "current_page": 1,
                "portals_count": count
            }
        })),
    )
}

pub async fn get_portal(
    Path((account_id, slug)): Path<(i32, String)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Agent).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let portal = sqlx::query_as::<_, Portal>(
        r#"
        SELECT id, account_id, name, slug, custom_domain, color,
               homepage_link, page_title, header_text, config, archived,
               created_at, updated_at
        FROM portals
        WHERE account_id = $1 AND slug = $2
        LIMIT 1
        "#,
    )
    .bind(account_id)
    .bind(&slug)
    .fetch_optional(&state.db)
    .await;

    match portal {
        Ok(Some(p)) => (
            StatusCode::OK,
            Json(json!({
                "payload": portal_to_json(p),
                "meta": {
                    "all_articles_count": 0
                }
            })),
        ),
        _ => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Portal not found" })),
        ),
    }
}

pub async fn create_portal(
    Path(account_id): Path<i32>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body): Json<CreatePortalPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let name = body.name.unwrap_or_else(|| "Help Center".to_string());
    let slug = body
        .slug
        .unwrap_or_else(|| name.to_lowercase().replace(' ', "-"));
    let config = body.config.unwrap_or_else(|| json!({"allowed_locales": ["en"]}));

    let portal = sqlx::query_as::<_, Portal>(
        r#"
        INSERT INTO portals (account_id, name, slug, color, custom_domain, page_title, header_text, homepage_link, config, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW(), NOW())
        RETURNING id, account_id, name, slug, custom_domain, color, homepage_link, page_title, header_text, config, archived, created_at, updated_at
        "#,
    )
    .bind(account_id)
    .bind(name)
    .bind(slug)
    .bind(body.color)
    .bind(body.custom_domain)
    .bind(body.page_title)
    .bind(body.header_text)
    .bind(body.homepage_link)
    .bind(config)
    .fetch_one(&state.db)
    .await;

    match portal {
        Ok(p) => (StatusCode::OK, Json(portal_to_json(p))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

pub async fn update_portal(
    Path((account_id, slug)): Path<(i32, String)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body): Json<CreatePortalPayload>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let portal = sqlx::query_as::<_, Portal>(
        r#"
        UPDATE portals 
        SET name = COALESCE($1, name),
            color = COALESCE($2, color),
            custom_domain = COALESCE($3, custom_domain),
            page_title = COALESCE($4, page_title),
            header_text = COALESCE($5, header_text),
            homepage_link = COALESCE($6, homepage_link),
            config = COALESCE($7, config),
            updated_at = NOW()
        WHERE account_id = $8 AND slug = $9
        RETURNING id, account_id, name, slug, custom_domain, color, homepage_link, page_title, header_text, config, archived, created_at, updated_at
        "#,
    )
    .bind(body.name)
    .bind(body.color)
    .bind(body.custom_domain)
    .bind(body.page_title)
    .bind(body.header_text)
    .bind(body.homepage_link)
    .bind(body.config)
    .bind(account_id)
    .bind(&slug)
    .fetch_optional(&state.db)
    .await;

    match portal {
        Ok(Some(p)) => (StatusCode::OK, Json(portal_to_json(p))),
        _ => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Portal not found" })),
        ),
    }
}

pub async fn delete_portal(
    Path((account_id, slug)): Path<(i32, String)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let _ = sqlx::query("DELETE FROM portals WHERE account_id = $1 AND slug = $2")
        .bind(account_id)
        .bind(&slug)
        .execute(&state.db)
        .await;

    (StatusCode::OK, Json(json!({ "success": true })))
}

pub async fn archive_portal(
    Path((account_id, slug)): Path<(i32, String)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    let _ = sqlx::query("UPDATE portals SET archived = NOT COALESCE(archived, false), updated_at = NOW() WHERE account_id = $1 AND slug = $2")
        .bind(account_id)
        .bind(&slug)
        .execute(&state.db)
        .await;

    (StatusCode::OK, Json(json!({ "success": true })))
}

pub async fn delete_portal_logo(
    Path((account_id, _slug)): Path<(i32, String)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    if let Err((code, msg)) =
        verify_account_access(&state.db, &current_user, account_id, AccountRole::Administrator).await
    {
        return (code, Json(json!({ "error": msg })));
    }

    (StatusCode::OK, Json(json!({ "success": true })))
}

// ======================== CATEGORIES ========================

pub async fn list_portal_categories(
    Path((account_id, portal_slug)): Path<(i32, String)>,
    Query(_query): Query<CategoryListQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let portal_id: Option<i64> = sqlx::query_scalar("SELECT id FROM portals WHERE account_id = $1 AND slug = $2")
        .bind(account_id)
        .bind(&portal_slug)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);

    let pid = match portal_id {
        Some(id) => id,
        None => return (StatusCode::NOT_FOUND, Json(json!({ "error": "Portal not found" }))),
    };

    let categories = sqlx::query!(
        r#"
        SELECT c.id, c.name, c.slug, c.locale, c.description, c.position,
               c.account_id, c.icon, c.icon_color,
               COUNT(a.id)::int8 as articles_count
        FROM categories c
        LEFT JOIN articles a ON a.category_id = c.id
        WHERE c.account_id = $1 AND c.portal_id = $2
        GROUP BY c.id, c.name, c.slug, c.locale, c.description, c.position, c.account_id, c.icon, c.icon_color
        ORDER BY c.position ASC, c.id ASC
        "#,
        account_id,
        pid as i32
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let payload: Vec<_> = categories
        .into_iter()
        .map(|c| {
            json!({
                "id": c.id,
                "name": c.name,
                "slug": c.slug,
                "locale": c.locale.unwrap_or_else(|| "en".to_string()),
                "description": c.description,
                "position": c.position.unwrap_or(0),
                "account_id": c.account_id,
                "icon": c.icon.unwrap_or_default(),
                "icon_color": c.icon_color.unwrap_or_default(),
                "related_categories": [],
                "meta": {
                    "articles_count": c.articles_count.unwrap_or(0)
                }
            })
        })
        .collect();

    (StatusCode::OK, Json(json!({ "payload": payload })))
}

pub async fn create_portal_category(
    Path((account_id, portal_slug)): Path<(i32, String)>,
    State(state): State<AppState>,
    Json(body): Json<CreateCategoryPayload>,
) -> impl IntoResponse {
    let portal_id: Option<i64> = sqlx::query_scalar("SELECT id FROM portals WHERE account_id = $1 AND slug = $2")
        .bind(account_id)
        .bind(&portal_slug)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);

    let pid = match portal_id {
        Some(id) => id,
        None => return (StatusCode::NOT_FOUND, Json(json!({ "error": "Portal not found" }))),
    };

    let name = body.name.unwrap_or_else(|| "General".to_string());
    let slug = body
        .slug
        .unwrap_or_else(|| name.to_lowercase().replace(' ', "-"));
    let locale = body.locale.unwrap_or_else(|| "en".to_string());
    let icon = body.icon.unwrap_or_default();
    let icon_color = body.icon_color.unwrap_or_default();
    let position = body.position.unwrap_or(0);

    let category = sqlx::query!(
        r#"
        INSERT INTO categories (account_id, portal_id, name, slug, description, locale, icon, icon_color, position, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW(), NOW())
        RETURNING id, name, slug, locale, description, position, account_id, icon, icon_color
        "#,
        account_id,
        pid as i32,
        name,
        slug,
        body.description,
        locale,
        icon,
        icon_color,
        position
    )
    .fetch_one(&state.db)
    .await;

    match category {
        Ok(c) => (
            StatusCode::OK,
            Json(json!({
                "payload": {
                    "id": c.id,
                    "name": c.name,
                    "slug": c.slug,
                    "locale": c.locale.unwrap_or_else(|| "en".to_string()),
                    "description": c.description,
                    "position": c.position.unwrap_or(0),
                    "account_id": c.account_id,
                    "icon": c.icon.unwrap_or_default(),
                    "icon_color": c.icon_color.unwrap_or_default(),
                    "related_categories": [],
                    "meta": { "articles_count": 0 }
                }
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

pub async fn get_portal_category(
    Path((account_id, portal_slug, category_id)): Path<(i32, String, i64)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let category = sqlx::query!(
        r#"
        SELECT c.id, c.name, c.slug, c.locale, c.description, c.position,
               c.account_id, c.icon, c.icon_color,
               COUNT(a.id)::int8 as articles_count
        FROM categories c
        JOIN portals p ON p.id = c.portal_id
        LEFT JOIN articles a ON a.category_id = c.id
        WHERE c.account_id = $1 AND p.slug = $2 AND c.id = $3
        GROUP BY c.id, c.name, c.slug, c.locale, c.description, c.position, c.account_id, c.icon, c.icon_color
        "#,
        account_id,
        portal_slug,
        category_id
    )
    .fetch_optional(&state.db)
    .await;

    match category {
        Ok(Some(c)) => (
            StatusCode::OK,
            Json(json!({
                "payload": {
                    "id": c.id,
                    "name": c.name,
                    "slug": c.slug,
                    "locale": c.locale.unwrap_or_else(|| "en".to_string()),
                    "description": c.description,
                    "position": c.position.unwrap_or(0),
                    "account_id": c.account_id,
                    "icon": c.icon.unwrap_or_default(),
                    "icon_color": c.icon_color.unwrap_or_default(),
                    "related_categories": [],
                    "meta": { "articles_count": c.articles_count.unwrap_or(0) }
                }
            })),
        ),
        _ => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Category not found" })),
        ),
    }
}

pub async fn update_portal_category(
    Path((account_id, _portal_slug, category_id)): Path<(i32, String, i64)>,
    State(state): State<AppState>,
    Json(body): Json<CreateCategoryPayload>,
) -> impl IntoResponse {
    let category = sqlx::query!(
        r#"
        UPDATE categories
        SET name = COALESCE($1, name),
            description = COALESCE($2, description),
            slug = COALESCE($3, slug),
            locale = COALESCE($4, locale),
            icon = COALESCE($5, icon),
            icon_color = COALESCE($6, icon_color),
            position = COALESCE($7, position),
            updated_at = NOW()
        WHERE account_id = $8 AND id = $9
        RETURNING id, name, slug, locale, description, position, account_id, icon, icon_color
        "#,
        body.name,
        body.description,
        body.slug,
        body.locale,
        body.icon,
        body.icon_color,
        body.position,
        account_id,
        category_id
    )
    .fetch_optional(&state.db)
    .await;

    match category {
        Ok(Some(c)) => (
            StatusCode::OK,
            Json(json!({
                "payload": {
                    "id": c.id,
                    "name": c.name,
                    "slug": c.slug,
                    "locale": c.locale.unwrap_or_else(|| "en".to_string()),
                    "description": c.description,
                    "position": c.position.unwrap_or(0),
                    "account_id": c.account_id,
                    "icon": c.icon.unwrap_or_default(),
                    "icon_color": c.icon_color.unwrap_or_default(),
                    "related_categories": [],
                    "meta": { "articles_count": 0 }
                }
            })),
        ),
        _ => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Category not found" })),
        ),
    }
}

pub async fn delete_portal_category(
    Path((account_id, _portal_slug, category_id)): Path<(i32, String, i64)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let _ = sqlx::query("DELETE FROM categories WHERE account_id = $1 AND id = $2")
        .bind(account_id)
        .bind(category_id)
        .execute(&state.db)
        .await;

    (StatusCode::OK, Json(json!({ "success": true })))
}

pub async fn reorder_portal_categories(
    Path((_account_id, _portal_slug)): Path<(i32, String)>,
    Json(_body): Json<ReorderPayload>,
) -> impl IntoResponse {
    (StatusCode::OK, Json(json!({ "success": true })))
}

// ======================== ARTICLES ========================

pub async fn list_portal_articles(
    Path((account_id, portal_slug)): Path<(i32, String)>,
    Query(query): Query<ArticleListQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let portal_id: Option<i64> = sqlx::query_scalar("SELECT id FROM portals WHERE account_id = $1 AND slug = $2")
        .bind(account_id)
        .bind(&portal_slug)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);

    let pid = match portal_id {
        Some(id) => id,
        None => return (StatusCode::NOT_FOUND, Json(json!({ "error": "Portal not found" }))),
    };

    let page = query.page_number.unwrap_or(1).max(1);
    let per_page = 25i64;
    let offset = (page - 1) * per_page;

    let articles = sqlx::query!(
        r#"
        SELECT a.id, a.title, a.content, a.description, a.status, a.views,
               a.slug, a.position, a.locale, a.category_id, a.author_id,
               a.draft_title, a.draft_content, a.created_at, a.updated_at,
               c.name as category_name, c.slug as category_slug,
               u.name as author_name, u.email as author_email
        FROM articles a
        LEFT JOIN categories c ON c.id = a.category_id
        LEFT JOIN users u ON u.id = a.author_id
        WHERE a.account_id = $1 AND a.portal_id = $2
        ORDER BY a.position ASC, a.updated_at DESC
        LIMIT $3 OFFSET $4
        "#,
        account_id,
        pid,
        per_page,
        offset
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let total_count: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*)::int8 FROM articles WHERE account_id = $1 AND portal_id = $2",
        account_id,
        pid
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(Some(0))
    .unwrap_or(0);

    let published_count: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*)::int8 FROM articles WHERE account_id = $1 AND portal_id = $2 AND status = 1",
        account_id,
        pid
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(Some(0))
    .unwrap_or(0);

    let draft_count: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*)::int8 FROM articles WHERE account_id = $1 AND portal_id = $2 AND status = 0",
        account_id,
        pid
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(Some(0))
    .unwrap_or(0);

    let payload: Vec<_> = articles
        .into_iter()
        .map(|a| {
            let status_num = a.status.unwrap_or(1);
            let category_json = a.category_id.map(|cid| {
                json!({
                    "id": cid,
                    "name": a.category_name,
                    "slug": a.category_slug
                })
            });

            let author_json = a.author_id.map(|aid| {
                json!({
                    "id": aid,
                    "name": a.author_name,
                    "email": a.author_email
                })
            });

            json!({
                "id": a.id,
                "slug": a.slug.unwrap_or_else(|| a.id.to_string()),
                "title": a.title,
                "content": a.content,
                "description": a.description,
                "status": format_status_str(status_num),
                "draft_title": a.draft_title,
                "draft_content": a.draft_content,
                "position": a.position.unwrap_or(0),
                "account_id": account_id,
                "updated_at": a.updated_at.and_utc().timestamp(),
                "category": category_json,
                "views": a.views.unwrap_or(0),
                "author": author_json,
                "associated_articles": []
            })
        })
        .collect();

    (
        StatusCode::OK,
        Json(json!({
            "payload": payload,
            "meta": {
                "all_articles_count": total_count,
                "archived_articles_count": 0,
                "articles_count": total_count,
                "current_page": page,
                "draft_articles_count": draft_count,
                "mine_articles_count": total_count,
                "published_count": published_count
            }
        })),
    )
}

pub async fn create_portal_article(
    Path((account_id, portal_slug)): Path<(i32, String)>,
    Extension(current_user): Extension<CurrentUser>,
    State(state): State<AppState>,
    Json(body): Json<CreateArticlePayload>,
) -> impl IntoResponse {
    let portal_id: Option<i64> = sqlx::query_scalar("SELECT id FROM portals WHERE account_id = $1 AND slug = $2")
        .bind(account_id)
        .bind(&portal_slug)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);

    let pid = match portal_id {
        Some(id) => id,
        None => return (StatusCode::NOT_FOUND, Json(json!({ "error": "Portal not found" }))),
    };

    let title = body.title.unwrap_or_else(|| "Untitled Article".to_string());
    let slug = body
        .slug
        .unwrap_or_else(|| title.to_lowercase().replace(' ', "-"));
    let status_num = parse_status(body.status.as_ref());
    let author_id = body.author_id.unwrap_or(current_user.id as i64);

    let article = sqlx::query!(
        r#"
        INSERT INTO articles (account_id, portal_id, title, content, description, status, author_id, category_id, slug, locale, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW(), NOW())
        RETURNING id, title, content, description, status, views, slug, position, locale, category_id, author_id, created_at, updated_at
        "#,
        account_id,
        pid,
        title,
        body.content,
        body.description,
        status_num,
        author_id,
        body.category_id,
        slug,
        body.locale.unwrap_or_else(|| "en".to_string())
    )
    .fetch_one(&state.db)
    .await;

    match article {
        Ok(a) => (
            StatusCode::OK,
            Json(json!({
                "payload": {
                    "id": a.id,
                    "slug": a.slug.unwrap_or_else(|| a.id.to_string()),
                    "title": a.title,
                    "content": a.content,
                    "description": a.description,
                    "status": format_status_str(a.status.unwrap_or(1)),
                    "views": a.views.unwrap_or(0),
                    "author_id": a.author_id,
                    "category_id": a.category_id,
                    "created_at": a.created_at.and_utc().timestamp(),
                    "updated_at": a.updated_at.and_utc().timestamp()
                }
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        ),
    }
}

pub async fn get_portal_article(
    Path((account_id, _portal_slug, article_id)): Path<(i32, String, i32)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let article = sqlx::query!(
        r#"
        SELECT a.id, a.title, a.content, a.description, a.status, a.views,
               a.slug, a.position, a.locale, a.category_id, a.author_id,
               a.draft_title, a.draft_content, a.created_at, a.updated_at,
               c.name as category_name, c.slug as category_slug,
               u.name as author_name, u.email as author_email
        FROM articles a
        LEFT JOIN categories c ON c.id = a.category_id
        LEFT JOIN users u ON u.id = a.author_id
        WHERE a.account_id = $1 AND a.id = $2
        "#,
        account_id,
        article_id
    )
    .fetch_optional(&state.db)
    .await;

    match article {
        Ok(Some(a)) => {
            let status_num = a.status.unwrap_or(1);
            let category_json = a.category_id.map(|cid| {
                json!({
                    "id": cid,
                    "name": a.category_name,
                    "slug": a.category_slug
                })
            });

            let author_json = a.author_id.map(|aid| {
                json!({
                    "id": aid,
                    "name": a.author_name,
                    "email": a.author_email
                })
            });

            (
                StatusCode::OK,
                Json(json!({
                    "payload": {
                        "id": a.id,
                        "slug": a.slug.unwrap_or_else(|| a.id.to_string()),
                        "title": a.title,
                        "content": a.content,
                        "description": a.description,
                        "status": format_status_str(status_num),
                        "draft_title": a.draft_title,
                        "draft_content": a.draft_content,
                        "position": a.position.unwrap_or(0),
                        "account_id": account_id,
                        "updated_at": a.updated_at.and_utc().timestamp(),
                        "category": category_json,
                        "views": a.views.unwrap_or(0),
                        "author": author_json,
                        "associated_articles": []
                    }
                })),
            )
        }
        _ => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Article not found" })),
        ),
    }
}

pub async fn update_portal_article(
    Path((account_id, _portal_slug, article_id)): Path<(i32, String, i32)>,
    State(state): State<AppState>,
    Json(body): Json<CreateArticlePayload>,
) -> impl IntoResponse {
    let status_num = body.status.as_ref().map(|s| parse_status(Some(s)));

    let article = sqlx::query!(
        r#"
        UPDATE articles
        SET title = COALESCE($1, title),
            content = COALESCE($2, content),
            description = COALESCE($3, description),
            status = COALESCE($4, status),
            author_id = COALESCE($5, author_id),
            category_id = COALESCE($6, category_id),
            slug = COALESCE($7, slug),
            locale = COALESCE($8, locale),
            updated_at = NOW()
        WHERE account_id = $9 AND id = $10
        RETURNING id, title, content, description, status, views, slug, position, locale, category_id, author_id, created_at, updated_at
        "#,
        body.title,
        body.content,
        body.description,
        status_num,
        body.author_id,
        body.category_id,
        body.slug,
        body.locale,
        account_id,
        article_id
    )
    .fetch_optional(&state.db)
    .await;

    match article {
        Ok(Some(a)) => (
            StatusCode::OK,
            Json(json!({
                "payload": {
                    "id": a.id,
                    "slug": a.slug.unwrap_or_else(|| a.id.to_string()),
                    "title": a.title,
                    "content": a.content,
                    "description": a.description,
                    "status": format_status_str(a.status.unwrap_or(1)),
                    "views": a.views.unwrap_or(0),
                    "author_id": a.author_id,
                    "category_id": a.category_id,
                    "created_at": a.created_at.and_utc().timestamp(),
                    "updated_at": a.updated_at.and_utc().timestamp()
                }
            })),
        ),
        _ => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Article not found" })),
        ),
    }
}

pub async fn delete_portal_article(
    Path((account_id, _portal_slug, article_id)): Path<(i32, String, i32)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let _ = sqlx::query("DELETE FROM articles WHERE account_id = $1 AND id = $2")
        .bind(account_id)
        .bind(article_id)
        .execute(&state.db)
        .await;

    (StatusCode::OK, Json(json!({ "success": true })))
}

pub async fn reorder_portal_articles(
    Path((_account_id, _portal_slug)): Path<(i32, String)>,
    Json(body): Json<ReorderPayload>,
) -> impl IntoResponse {
    let positions = body.positions_hash.unwrap_or_else(|| json!({}));
    (StatusCode::OK, Json(json!({ "positions": positions })))
}
