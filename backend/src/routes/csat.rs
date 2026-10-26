use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use tracing::warn;

use crate::models::contact::Contact;
use crate::models::conversation::Conversation;
use crate::models::csat::{
    CreateCsatPayload, CsatMetricsResponse, CsatSurveyResponse, CsatSurveyResponseData,
};
use crate::models::message::Message;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CsatFilterQuery {
    pub page: Option<i64>,
    pub rating: Option<i32>,
    pub user_ids: Option<String>,
    pub inbox_id: Option<i64>,
}

/// GET /api/v1/accounts/:account_id/csat_survey_responses
pub async fn list_csat_responses(
    Path(account_id): Path<i64>,
    Query(query): Query<CsatFilterQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let page = query.page.unwrap_or(1).max(1);
    let per_page: i64 = 25;
    let offset = (page - 1) * per_page;

    let responses = match sqlx::query_as::<_, CsatSurveyResponse>(
        r#"
        SELECT * FROM csat_survey_responses
        WHERE account_id = $1
        ORDER BY created_at DESC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(account_id)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to fetch csat responses: {}", e);
            vec![]
        }
    };

    let mut payload = Vec::new();
    for r in responses {
        // Ambil display_id percakapan
        let conv_display_id: i32 = sqlx::query_scalar("SELECT display_id FROM conversations WHERE id = $1")
            .bind(r.conversation_id)
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None)
            .unwrap_or(1);

        // Ambil nama contact
        let contact_json = if let Ok(Some(ct)) = sqlx::query_as::<_, Contact>("SELECT * FROM contacts WHERE id = $1")
            .bind(r.contact_id)
            .fetch_optional(&state.db)
            .await
        {
            Some(json!({
                "id": ct.id,
                "name": ct.name,
                "email": ct.email,
                "phone_number": ct.phone_number
            }))
        } else {
            None
        };

        // Ambil assigned agent
        let agent_json = if let Some(aid) = r.assigned_agent_id {
            let aname: Option<String> = sqlx::query_scalar("SELECT name FROM users WHERE id = $1")
                .bind(aid)
                .fetch_optional(&state.db)
                .await
                .unwrap_or(None);

            Some(json!({
                "id": aid,
                "name": aname.unwrap_or_else(|| "Agent".to_string())
            }))
        } else {
            None
        };

        payload.push(CsatSurveyResponseData {
            id: r.id,
            rating: r.rating,
            feedback_message: r.feedback_message,
            csat_review_notes: r.csat_review_notes,
            review_notes_updated_at: r.review_notes_updated_at.map(|t| t.and_utc().timestamp()),
            review_notes_updated_by: None,
            account_id: r.account_id,
            message_id: r.message_id,
            contact: contact_json,
            conversation_id: conv_display_id,
            assigned_agent: agent_json,
            created_at: r.created_at.and_utc().timestamp(),
        });
    }

    Json(payload).into_response()
}

/// GET /api/v1/accounts/:account_id/csat_survey_responses/metrics
pub async fn get_csat_metrics(
    Path(account_id): Path<i64>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let total_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM csat_survey_responses WHERE account_id = $1",
    )
    .bind(account_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let rating_rows: Vec<(i32, i64)> = sqlx::query_as(
        "SELECT rating, COUNT(*) FROM csat_survey_responses WHERE account_id = $1 GROUP BY rating",
    )
    .bind(account_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let mut ratings_map = serde_json::Map::new();
    for (rating, count) in rating_rows {
        ratings_map.insert(rating.to_string(), json!(count));
    }

    // Hitung pesan CSAT yang dikirim (content_type = 5 atau message_type = 1 outgoing)
    let total_sent_messages_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages WHERE account_id = $1 AND content_type = 5",
    )
    .bind(account_id as i32)
    .fetch_one(&state.db)
    .await
    .unwrap_or(total_count);

    Json(CsatMetricsResponse {
        total_count,
        ratings_count: serde_json::Value::Object(ratings_map),
        total_sent_messages_count: total_sent_messages_count.max(total_count),
    })
    .into_response()
}

/// POST /api/v1/accounts/:account_id/csat_survey_responses
pub async fn create_csat_response(
    Path(account_id): Path<i64>,
    State(state): State<AppState>,
    Json(body): Json<CreateCsatPayload>,
) -> impl IntoResponse {
    let p = body.csat_survey_response;
    let message_id = p.as_ref().and_then(|c| c.message_id).or(body.message_id);
    let rating = p.as_ref().and_then(|c| c.rating).or(body.rating).unwrap_or(5);
    let feedback = p.as_ref().and_then(|c| c.feedback_message.clone()).or(body.feedback_message);

    if !(1..=5).contains(&rating) {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": "Rating must be an integer between 1 and 5" })),
        )
            .into_response();
    }

    let mid = match message_id {
        Some(m) => m,
        None => {
            // Ambil pesan terakhir di akun jika tidak disertakan
            let latest: Option<i64> = sqlx::query_scalar(
                "SELECT id::bigint FROM messages WHERE account_id = $1 ORDER BY id DESC LIMIT 1",
            )
            .bind(account_id as i32)
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);

            match latest {
                Some(l) => l,
                None => {
                    return (
                        StatusCode::UNPROCESSABLE_ENTITY,
                        Json(json!({ "error": "No message found for CSAT survey" })),
                    )
                        .into_response();
                }
            }
        }
    };

    // Ambil data conversation_id & contact_id dari pesan
    let msg = match sqlx::query_as::<_, Message>("SELECT * FROM messages WHERE id = $1")
        .bind(mid as i32)
        .fetch_optional(&state.db)
        .await
    {
        Ok(Some(m)) => m,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Message not found" })),
            )
                .into_response();
        }
    };

    let conv = match sqlx::query_as::<_, Conversation>("SELECT * FROM conversations WHERE id = $1")
        .bind(msg.conversation_id)
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

    let contact_id = conv.contact_id.unwrap_or(1);
    let assigned_agent_id = conv.assignee_id.map(|a| a as i64);

    let csat = match sqlx::query_as::<_, CsatSurveyResponse>(
        r#"
        INSERT INTO csat_survey_responses (
            account_id, conversation_id, message_id, rating, feedback_message,
            contact_id, assigned_agent_id, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW())
        ON CONFLICT (message_id) DO UPDATE
        SET rating = EXCLUDED.rating, feedback_message = EXCLUDED.feedback_message, updated_at = NOW()
        RETURNING *
        "#,
    )
    .bind(account_id)
    .bind(conv.id as i64)
    .bind(mid)
    .bind(rating)
    .bind(&feedback)
    .bind(contact_id)
    .bind(assigned_agent_id)
    .fetch_one(&state.db)
    .await
    {
        Ok(c) => c,
        Err(e) => {
            warn!("Failed to create csat response: {}", e);
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let resp = CsatSurveyResponseData {
        id: csat.id,
        rating: csat.rating,
        feedback_message: csat.feedback_message,
        csat_review_notes: csat.csat_review_notes,
        review_notes_updated_at: None,
        review_notes_updated_by: None,
        account_id: csat.account_id,
        message_id: csat.message_id,
        contact: Some(json!({ "id": contact_id })),
        conversation_id: conv.display_id,
        assigned_agent: assigned_agent_id.map(|aid| json!({ "id": aid })),
        created_at: csat.created_at.and_utc().timestamp(),
    };

    (StatusCode::OK, Json(resp)).into_response()
}

/// GET /api/v1/accounts/:account_id/csat_survey_responses/download
pub async fn download_csat_csv(
    Path(account_id): Path<i64>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let responses: Vec<CsatSurveyResponse> = sqlx::query_as(
        "SELECT * FROM csat_survey_responses WHERE account_id = $1 ORDER BY id DESC LIMIT 1000",
    )
    .bind(account_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let mut csv_out = String::from("id,rating,feedback_message,conversation_id,contact_id,created_at\n");
    for r in responses {
        let clean_feedback = r.feedback_message.unwrap_or_default().replace(',', " ").replace('\n', " ");
        csv_out.push_str(&format!(
            "{},{},\"{}\",{},{},{}\n",
            r.id, r.rating, clean_feedback, r.conversation_id, r.contact_id, r.created_at
        ));
    }

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/csv"), (header::CONTENT_DISPOSITION, "attachment; filename=csat_report.csv")],
        csv_out,
    )
}
