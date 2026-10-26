use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgPool, Row};

use crate::models::attachment::{Attachment, AttachmentResponse};
use crate::models::message::{Message, MessageResponse, MessageSender};
use crate::state::AppState;

const PER_PAGE: i64 = 15;

#[derive(Debug, Deserialize, Default)]
pub struct SearchQuery {
    pub q: Option<String>,
    pub page: Option<i64>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub from: Option<String>,
    pub inbox_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchContactItem {
    pub id: i32,
    pub name: Option<String>,
    pub email: Option<String>,
    pub phone_number: Option<String>,
    pub identifier: Option<String>,
    pub thumbnail: String,
    pub additional_attributes: serde_json::Value,
    pub custom_attributes: serde_json::Value,
    pub last_activity_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchInboxItem {
    pub id: i32,
    pub channel_id: Option<i32>,
    pub name: String,
    pub channel_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchAgentItem {
    pub id: i64,
    pub available_name: Option<String>,
    pub email: Option<String>,
    pub name: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConversationItem {
    pub id: i32, // display_id
    pub account_id: i32,
    pub created_at: i64,
    pub message: Option<MessageResponse>,
    pub contact: Option<SearchContactItem>,
    pub inbox: Option<SearchInboxItem>,
    pub agent: Option<SearchAgentItem>,
    pub additional_attributes: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchArticleItem {
    pub id: i32,
    pub title: String,
    pub description: Option<String>,
    pub locale: String,
    pub content: Option<String>,
    pub slug: String,
    pub portal_slug: String,
    pub account_id: i32,
    pub category: Option<String>,
    pub category_name: Option<String>,
    pub status: String,
    pub updated_at: i64,
}

fn parse_time_bound(val: Option<&str>) -> Option<NaiveDateTime> {
    val.and_then(|v| v.parse::<i64>().ok())
        .and_then(|sec| chrono::DateTime::from_timestamp(sec, 0).map(|dt| dt.naive_utc()))
}

/// Helper untuk mencari contacts
pub async fn filter_contacts(
    pool: &PgPool,
    account_id: i32,
    query: &SearchQuery,
) -> Vec<SearchContactItem> {
    let q_term = query.q.as_deref().unwrap_or("").trim();
    let search_pattern = format!("%{}%", q_term);
    let page = query.page.unwrap_or(1).max(1);
    let offset = (page - 1) * PER_PAGE;
    let since_dt = parse_time_bound(query.since.as_deref());
    let until_dt = parse_time_bound(query.until.as_deref());

    let rows = match sqlx::query(
        r#"
        SELECT id, name, email, phone_number, identifier, additional_attributes, custom_attributes,
               extract(epoch from last_activity_at)::bigint as last_activity_at
        FROM contacts
        WHERE account_id = $1
          AND (
            $2 = '%%'
            OR name ILIKE $2
            OR email ILIKE $2
            OR phone_number ILIKE $2
            OR identifier ILIKE $2
          )
          AND ($3::timestamp IS NULL OR last_activity_at >= $3)
          AND ($4::timestamp IS NULL OR last_activity_at <= $4)
        ORDER BY last_activity_at DESC NULLS LAST, id DESC
        LIMIT $5 OFFSET $6
        "#,
    )
    .bind(account_id)
    .bind(&search_pattern)
    .bind(since_dt)
    .bind(until_dt)
    .bind(PER_PAGE)
    .bind(offset)
    .fetch_all(pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Failed to query search contacts: {:?}", e);
            return Vec::new();
        }
    };

    rows.into_iter()
        .map(|row| {
            let id: i32 = row.try_get("id").unwrap_or(0);
            let name: Option<String> = row.try_get("name").ok();
            let email: Option<String> = row.try_get("email").ok();
            let phone_number: Option<String> = row.try_get("phone_number").ok();
            let identifier: Option<String> = row.try_get("identifier").ok();
            let additional_attributes: serde_json::Value = row
                .try_get("additional_attributes")
                .unwrap_or_else(|_| json!({}));
            let custom_attributes: serde_json::Value = row
                .try_get("custom_attributes")
                .unwrap_or_else(|_| json!({}));
            let last_activity_at: Option<i64> = row.try_get("last_activity_at").ok();

            SearchContactItem {
                id,
                name,
                email,
                phone_number,
                identifier,
                thumbnail: "".to_string(),
                additional_attributes,
                custom_attributes,
                last_activity_at,
            }
        })
        .collect()
}

/// Helper untuk mencari conversations
pub async fn filter_conversations(
    pool: &PgPool,
    account_id: i32,
    query: &SearchQuery,
) -> Vec<SearchConversationItem> {
    let q_term = query.q.as_deref().unwrap_or("").trim();
    let search_pattern = format!("%{}%", q_term);
    let page = query.page.unwrap_or(1).max(1);
    let offset = (page - 1) * PER_PAGE;
    let since_dt = parse_time_bound(query.since.as_deref());
    let until_dt = parse_time_bound(query.until.as_deref());

    let rows = match sqlx::query(
        r#"
        SELECT c.id as conv_id, c.display_id, c.account_id, c.inbox_id, c.assignee_id, c.contact_id,
               c.additional_attributes as conv_additional_attributes,
               extract(epoch from c.created_at)::bigint as conv_created_at,
               ct.id as contact_id_val, ct.name as contact_name, ct.email as contact_email,
               ct.phone_number as contact_phone, ct.identifier as contact_identifier,
               ct.additional_attributes as contact_additional_attributes,
               ct.custom_attributes as contact_custom_attributes,
               extract(epoch from ct.last_activity_at)::bigint as contact_last_activity_at,
               i.id as inbox_id_val, i.channel_id, i.name as inbox_name, i.channel_type,
               u.id as agent_id, u.name as agent_name, u.email as agent_email,
               au.role as agent_role
        FROM conversations c
        LEFT JOIN contacts ct ON c.contact_id = ct.id
        LEFT JOIN inboxes i ON c.inbox_id = i.id
        LEFT JOIN users u ON c.assignee_id = u.id
        LEFT JOIN account_users au ON (au.user_id = u.id AND au.account_id = c.account_id)
        WHERE c.account_id = $1
          AND (
            $2 = '%%'
            OR cast(c.display_id as text) ILIKE $2
            OR ct.name ILIKE $2
            OR ct.email ILIKE $2
            OR ct.phone_number ILIKE $2
            OR ct.identifier ILIKE $2
            OR c.additional_attributes->>'mail_subject' ILIKE $2
          )
          AND ($3::timestamp IS NULL OR c.created_at >= $3)
          AND ($4::timestamp IS NULL OR c.created_at <= $4)
        ORDER BY c.created_at DESC
        LIMIT $5 OFFSET $6
        "#,
    )
    .bind(account_id)
    .bind(&search_pattern)
    .bind(since_dt)
    .bind(until_dt)
    .bind(PER_PAGE)
    .bind(offset)
    .fetch_all(pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Failed to query search conversations: {:?}", e);
            return Vec::new();
        }
    };

    let mut result = Vec::new();

    for row in rows {
        let conv_id: i32 = row.try_get("conv_id").unwrap_or(0);
        let display_id: i32 = row.try_get("display_id").unwrap_or(0);
        let account_id_val: i32 = row.try_get("account_id").unwrap_or(account_id);
        let created_at: i64 = row.try_get("conv_created_at").unwrap_or(0);
        let conv_additional_attributes: serde_json::Value = row
            .try_get("conv_additional_attributes")
            .unwrap_or_else(|_| json!({}));

        let contact = if let Ok(cid) = row.try_get::<i32, _>("contact_id_val") {
            Some(SearchContactItem {
                id: cid,
                name: row.try_get("contact_name").ok(),
                email: row.try_get("contact_email").ok(),
                phone_number: row.try_get("contact_phone").ok(),
                identifier: row.try_get("contact_identifier").ok(),
                thumbnail: "".to_string(),
                additional_attributes: row
                    .try_get("contact_additional_attributes")
                    .unwrap_or_else(|_| json!({})),
                custom_attributes: row
                    .try_get("contact_custom_attributes")
                    .unwrap_or_else(|_| json!({})),
                last_activity_at: row.try_get("contact_last_activity_at").ok(),
            })
        } else {
            None
        };

        let inbox = if let Ok(inbox_id_val) = row.try_get::<i32, _>("inbox_id_val") {
            Some(SearchInboxItem {
                id: inbox_id_val,
                channel_id: row.try_get("channel_id").ok(),
                name: row.try_get("inbox_name").unwrap_or_default(),
                channel_type: row
                    .try_get("channel_type")
                    .unwrap_or_else(|_| "Channel::Api".to_string()),
            })
        } else {
            None
        };

        let agent = if let Ok(agent_id) = row.try_get::<i64, _>("agent_id") {
            let role_int: i32 = row.try_get("agent_role").unwrap_or(0);
            let role_str = if role_int == 1 {
                "administrator"
            } else {
                "agent"
            };
            let name: String = row.try_get("agent_name").unwrap_or_default();
            Some(SearchAgentItem {
                id: agent_id,
                available_name: Some(name.clone()),
                email: row.try_get("agent_email").ok(),
                name,
                role: role_str.to_string(),
            })
        } else {
            None
        };

        // Ambil first message untuk conversation ini
        let first_msg: Option<MessageResponse> = match sqlx::query_as::<_, Message>(
            "SELECT * FROM messages WHERE conversation_id = $1 ORDER BY created_at ASC LIMIT 1",
        )
        .bind(conv_id)
        .fetch_optional(pool)
        .await
        {
            Ok(Some(m)) => {
                let attachments = sqlx::query_as::<_, Attachment>(
                    "SELECT * FROM attachments WHERE message_id = $1",
                )
                .bind(m.id)
                .fetch_all(pool)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(AttachmentResponse::from)
                .collect();

                let sender = match (m.sender_type.as_deref(), m.sender_id) {
                    (Some("User"), Some(user_id)) => {
                        let u_name = sqlx::query_scalar::<_, String>(
                            "SELECT name FROM users WHERE id = $1",
                        )
                        .bind(user_id as i32)
                        .fetch_optional(pool)
                        .await
                        .ok()
                        .flatten();

                        Some(MessageSender {
                            id: user_id,
                            name: u_name.unwrap_or_else(|| "Agent".to_string()),
                            avatar_url: "".to_string(),
                            r#type: "user".to_string(),
                        })
                    }
                    (Some("Contact"), Some(contact_id)) => {
                        let c_name = sqlx::query_scalar::<_, Option<String>>(
                            "SELECT name FROM contacts WHERE id = $1",
                        )
                        .bind(contact_id as i32)
                        .fetch_optional(pool)
                        .await
                        .ok()
                        .flatten()
                        .flatten();

                        Some(MessageSender {
                            id: contact_id,
                            name: c_name.unwrap_or_else(|| "Customer".to_string()),
                            avatar_url: "".to_string(),
                            r#type: "contact".to_string(),
                        })
                    }
                    _ => None,
                };

                Some(m.to_response(display_id, sender, attachments))
            }
            _ => None,
        };

        result.push(SearchConversationItem {
            id: display_id,
            account_id: account_id_val,
            created_at,
            message: first_msg,
            contact,
            inbox,
            agent,
            additional_attributes: conv_additional_attributes,
        });
    }

    result
}

/// Helper untuk mencari messages
pub async fn filter_messages(
    pool: &PgPool,
    account_id: i32,
    query: &SearchQuery,
) -> Vec<MessageResponse> {
    let q_term = query.q.as_deref().unwrap_or("").trim();
    let search_pattern = format!("%{}%", q_term);
    let page = query.page.unwrap_or(1).max(1);
    let offset = (page - 1) * PER_PAGE;
    let since_dt = parse_time_bound(query.since.as_deref());
    let until_dt = parse_time_bound(query.until.as_deref());

    // Sender filter (e.g., "agent:1" atau "contact:2")
    let (sender_type, sender_id) = if let Some(ref from_str) = query.from {
        let parts: Vec<&str> = from_str.split(':').collect();
        if parts.len() == 2 {
            let st = if parts[0] == "agent" {
                "User"
            } else {
                "Contact"
            };
            let sid = parts[1].parse::<i64>().ok();
            (Some(st.to_string()), sid)
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    let rows = match sqlx::query(
        r#"
        SELECT m.id, m.content, m.account_id, m.inbox_id, m.conversation_id, m.message_type,
               m.created_at, m.updated_at, m.private, m.status, m.source_id, m.content_type,
               m.content_attributes, m.sender_type, m.sender_id, m.external_source_ids,
               m.additional_attributes, m.processed_message_content, m.sentiment,
               c.display_id as conversation_display_id
        FROM messages m
        INNER JOIN conversations c ON m.conversation_id = c.id
        WHERE m.account_id = $1
          AND ($2 = '%%' OR m.content ILIKE $2)
          AND ($3::timestamp IS NULL OR m.created_at >= $3)
          AND ($4::timestamp IS NULL OR m.created_at <= $4)
          AND ($5::text IS NULL OR m.sender_type = $5)
          AND ($6::bigint IS NULL OR m.sender_id = $6)
          AND ($7::bigint IS NULL OR m.inbox_id = $7)
        ORDER BY m.created_at DESC, m.id DESC
        LIMIT $8 OFFSET $9
        "#,
    )
    .bind(account_id)
    .bind(&search_pattern)
    .bind(since_dt)
    .bind(until_dt)
    .bind(sender_type)
    .bind(sender_id)
    .bind(query.inbox_id)
    .bind(PER_PAGE)
    .bind(offset)
    .fetch_all(pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Failed to query search messages: {:?}", e);
            return Vec::new();
        }
    };

    let mut result = Vec::new();

    for row in rows {
        let id: i32 = row.try_get("id").unwrap_or(0);
        let content: Option<String> = row.try_get("content").ok();
        let inbox_id: i32 = row.try_get("inbox_id").unwrap_or(0);
        let conversation_id: i32 = row.try_get("conversation_id").unwrap_or(0);
        let conversation_display_id: i32 =
            row.try_get("conversation_display_id").unwrap_or(conversation_id);
        let message_type: i32 = row.try_get("message_type").unwrap_or(0);
        let created_at: NaiveDateTime = row
            .try_get("created_at")
            .unwrap_or_else(|_| chrono::Utc::now().naive_utc());
        let updated_at: NaiveDateTime = row
            .try_get("updated_at")
            .unwrap_or_else(|_| chrono::Utc::now().naive_utc());
        let private: bool = row.try_get("private").unwrap_or(false);
        let status: Option<i32> = row.try_get("status").ok();
        let source_id: Option<String> = row.try_get("source_id").ok();
        let content_type: i32 = row.try_get("content_type").unwrap_or(0);
        let content_attributes: Option<serde_json::Value> = row.try_get("content_attributes").ok();
        let sender_type: Option<String> = row.try_get("sender_type").ok();
        let sender_id: Option<i64> = row.try_get("sender_id").ok();
        let external_source_ids: Option<serde_json::Value> =
            row.try_get("external_source_ids").ok();
        let additional_attributes: Option<serde_json::Value> =
            row.try_get("additional_attributes").ok();
        let processed_message_content: Option<String> =
            row.try_get("processed_message_content").ok();
        let sentiment: Option<serde_json::Value> = row.try_get("sentiment").ok();

        let msg = Message {
            id,
            content,
            account_id,
            inbox_id,
            conversation_id,
            message_type,
            created_at,
            updated_at,
            private,
            status,
            source_id,
            content_type,
            content_attributes,
            sender_type: sender_type.clone(),
            sender_id,
            external_source_ids,
            additional_attributes,
            processed_message_content,
            sentiment,
        };

        let attachments = sqlx::query_as::<_, Attachment>(
            "SELECT * FROM attachments WHERE message_id = $1",
        )
        .bind(id)
        .fetch_all(pool)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(AttachmentResponse::from)
        .collect();

        let sender = match (sender_type.as_deref(), sender_id) {
            (Some("User"), Some(user_id)) => {
                let u_name = sqlx::query_scalar::<_, String>(
                    "SELECT name FROM users WHERE id = $1",
                )
                .bind(user_id as i32)
                .fetch_optional(pool)
                .await
                .ok()
                .flatten();

                Some(MessageSender {
                    id: user_id,
                    name: u_name.unwrap_or_else(|| "Agent".to_string()),
                    avatar_url: "".to_string(),
                    r#type: "user".to_string(),
                })
            }
            (Some("Contact"), Some(contact_id)) => {
                let c_name = sqlx::query_scalar::<_, Option<String>>(
                    "SELECT name FROM contacts WHERE id = $1",
                )
                .bind(contact_id as i32)
                .fetch_optional(pool)
                .await
                .ok()
                .flatten()
                .flatten();

                Some(MessageSender {
                    id: contact_id,
                    name: c_name.unwrap_or_else(|| "Customer".to_string()),
                    avatar_url: "".to_string(),
                    r#type: "contact".to_string(),
                })
            }
            _ => None,
        };

        result.push(msg.to_response(conversation_display_id, sender, attachments));
    }

    result
}

/// Helper untuk mencari articles
pub async fn filter_articles(
    pool: &PgPool,
    account_id: i32,
    query: &SearchQuery,
) -> Vec<SearchArticleItem> {
    let q_term = query.q.as_deref().unwrap_or("").trim();
    let search_pattern = format!("%{}%", q_term);
    let page = query.page.unwrap_or(1).max(1);
    let offset = (page - 1) * PER_PAGE;
    let since_dt = parse_time_bound(query.since.as_deref());
    let until_dt = parse_time_bound(query.until.as_deref());

    let rows = match sqlx::query(
        r#"
        SELECT a.id, a.title, a.description, a.content, a.account_id, a.status,
               extract(epoch from a.updated_at)::bigint as updated_at,
               p.slug as portal_slug
        FROM articles a
        LEFT JOIN portals p ON a.portal_id = p.id
        WHERE a.account_id = $1
          AND (
            $2 = '%%'
            OR a.title ILIKE $2
            OR a.content ILIKE $2
            OR a.description ILIKE $2
          )
          AND ($3::timestamp IS NULL OR a.updated_at >= $3)
          AND ($4::timestamp IS NULL OR a.updated_at <= $4)
        ORDER BY a.updated_at DESC
        LIMIT $5 OFFSET $6
        "#,
    )
    .bind(account_id)
    .bind(&search_pattern)
    .bind(since_dt)
    .bind(until_dt)
    .bind(PER_PAGE)
    .bind(offset)
    .fetch_all(pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Failed to query search articles: {:?}", e);
            return Vec::new();
        }
    };

    rows.into_iter()
        .map(|row| {
            let id: i32 = row.try_get("id").unwrap_or(0);
            let title: String = row.try_get("title").unwrap_or_default();
            let description: Option<String> = row.try_get("description").ok();
            let content: Option<String> = row.try_get("content").ok();
            let status_int: i32 = row.try_get("status").unwrap_or(1);
            let status_str = match status_int {
                0 => "draft",
                2 => "archived",
                _ => "published",
            };
            let updated_at: i64 = row.try_get("updated_at").unwrap_or(0);
            let portal_slug: String = row
                .try_get("portal_slug")
                .unwrap_or_else(|_| "default".to_string());
            let slug = format!("article-{}", id);

            SearchArticleItem {
                id,
                title,
                description,
                locale: "en".to_string(),
                content,
                slug,
                portal_slug,
                account_id,
                category: None,
                category_name: None,
                status: status_str.to_string(),
                updated_at,
            }
        })
        .collect()
}

/// GET /api/v1/accounts/:account_id/search?q=...
pub async fn search_all(
    Path(account_id): Path<i32>,
    Query(query): Query<SearchQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let contacts = filter_contacts(&state.db, account_id, &query).await;
    let conversations = filter_conversations(&state.db, account_id, &query).await;
    let messages = filter_messages(&state.db, account_id, &query).await;
    let articles = filter_articles(&state.db, account_id, &query).await;

    (
        StatusCode::OK,
        Json(json!({
            "payload": {
                "contacts": contacts,
                "conversations": conversations,
                "messages": messages,
                "articles": articles,
            }
        })),
    )
}

/// GET /api/v1/accounts/:account_id/search/contacts?q=...&page=...
pub async fn search_contacts(
    Path(account_id): Path<i32>,
    Query(query): Query<SearchQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let contacts = filter_contacts(&state.db, account_id, &query).await;
    (
        StatusCode::OK,
        Json(json!({
            "payload": {
                "contacts": contacts,
            }
        })),
    )
}

/// GET /api/v1/accounts/:account_id/search/conversations?q=...&page=...
pub async fn search_conversations(
    Path(account_id): Path<i32>,
    Query(query): Query<SearchQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let conversations = filter_conversations(&state.db, account_id, &query).await;
    (
        StatusCode::OK,
        Json(json!({
            "payload": {
                "conversations": conversations,
            }
        })),
    )
}

/// GET /api/v1/accounts/:account_id/search/messages?q=...&page=...
pub async fn search_messages(
    Path(account_id): Path<i32>,
    Query(query): Query<SearchQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let messages = filter_messages(&state.db, account_id, &query).await;
    (
        StatusCode::OK,
        Json(json!({
            "payload": {
                "messages": messages,
            }
        })),
    )
}

/// GET /api/v1/accounts/:account_id/search/articles?q=...&page=...
pub async fn search_articles(
    Path(account_id): Path<i32>,
    Query(query): Query<SearchQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let articles = filter_articles(&state.db, account_id, &query).await;
    (
        StatusCode::OK,
        Json(json!({
            "payload": {
                "articles": articles,
            }
        })),
    )
}
