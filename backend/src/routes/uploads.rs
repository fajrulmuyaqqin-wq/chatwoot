use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::json;
use tracing::warn;

use crate::models::conversation::Conversation;
use crate::services::storage;
use crate::state::AppState;

pub async fn direct_upload(
    Path((account_id, display_id)): Path<(i32, i32)>,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let conv = match sqlx::query_as::<_, Conversation>(
        "SELECT * FROM conversations WHERE account_id = $1 AND display_id = $2",
    )
    .bind(account_id)
    .bind(display_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(c)) => c,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Conversation not found" })),
            );
        }
    };

    let mut file_data: Option<(Vec<u8>, Option<String>)> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" || name == "attachment" || name == "blob" {
            let filename = field.file_name().map(|f| f.to_string());
            if let Ok(bytes) = field.bytes().await {
                file_data = Some((bytes.to_vec(), filename));
                break;
            }
        }
    }

    let (bytes, filename) = match file_data {
        Some(d) => d,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "No file field found in multipart request" })),
            );
        }
    };

    let stored = match storage::save_upload(&bytes, filename.as_deref()).await {
        Ok(s) => s,
        Err(e) => {
            warn!("Gagal menyimpan file upload: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to write file to storage" })),
            );
        }
    };

    let file_type_int = match stored.extension.as_str() {
        "jpg" | "jpeg" | "png" | "gif" | "webp" => 0, // image
        "mp3" | "wav" | "ogg" | "m4a" => 1,           // audio
        "mp4" | "mov" | "webm" => 2,                 // video
        _ => 3,                                      // file / document
    };

    // Insert ke tabel attachments dengan message_id 0 jika belum terikat ke pesan spesifik
    let attachment_id: (i32,) = match sqlx::query_as(
        r#"
        INSERT INTO attachments (
            account_id, message_id, file_type, external_url, extension, created_at, updated_at
        )
        VALUES (
            $1, 0, $2, $3, $4, NOW(), NOW()
        )
        RETURNING id
        "#,
    )
    .bind(account_id)
    .bind(file_type_int)
    .bind(&stored.relative_url)
    .bind(&stored.extension)
    .fetch_one(&state.db)
    .await
    {
        Ok(row) => row,
        Err(_) => (0,),
    };

    (
        StatusCode::OK,
        Json(json!({
            "file_url": stored.relative_url,
            "data_url": stored.relative_url,
            "thumb_url": stored.relative_url,
            "id": attachment_id.0,
            "blob_id": attachment_id.0,
            "file_type": if file_type_int == 0 { "image" } else { "file" },
            "conversation_id": conv.display_id,
            "account_id": account_id
        })),
    )
}
