use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use tracing::{error, info};

use crate::models::contact::{Contact, ContactResponse};
use crate::state::{AppState, CableBroadcastMsg};

#[derive(Debug, Deserialize)]
pub struct ContactMergePayload {
    pub base_contact_id: i64,
    pub mergee_contact_id: i64,
}

pub async fn merge_contacts(
    Path(account_id): Path<i32>,
    State(state): State<AppState>,
    Json(payload): Json<ContactMergePayload>,
) -> impl IntoResponse {
    let base_id = payload.base_contact_id as i32;
    let mergee_id = payload.mergee_contact_id as i32;

    if base_id == mergee_id {
        let contact = sqlx::query_as::<_, Contact>(
            "SELECT * FROM contacts WHERE id = $1 AND account_id = $2"
        )
        .bind(base_id)
        .bind(account_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();

        return match contact {
            Some(c) => (StatusCode::OK, Json(json!(ContactResponse::from(c)))),
            None => (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Contact not found" })),
            ),
        };
    }

    let mut tx = match state.db.begin().await {
        Ok(t) => t,
        Err(e) => {
            error!("Gagal memulai transaksi database: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Database transaction error" })),
            );
        }
    };

    // 1. Validasi keberadaan kedua contact dan pastikan keduanya milik account_id yang sama
    let base_opt = sqlx::query_as::<_, Contact>(
        "SELECT * FROM contacts WHERE id = $1 AND account_id = $2 FOR UPDATE"
    )
    .bind(base_id)
    .bind(account_id)
    .fetch_optional(&mut *tx)
    .await
    .unwrap_or(None);

    let mergee_opt = sqlx::query_as::<_, Contact>(
        "SELECT * FROM contacts WHERE id = $1 AND account_id = $2 FOR UPDATE"
    )
    .bind(mergee_id)
    .bind(account_id)
    .fetch_optional(&mut *tx)
    .await
    .unwrap_or(None);

    let (base_contact, mergee_contact) = match (base_opt, mergee_opt) {
        (Some(b), Some(m)) => (b, m),
        _ => {
            let _ = tx.rollback().await;
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": "Both contacts must exist and belong to the same account" })),
            );
        }
    };

    // 2. Re-parent conversations
    if let Err(e) = sqlx::query(
        "UPDATE conversations SET contact_id = $1 WHERE contact_id = $2 AND account_id = $3"
    )
    .bind(base_id)
    .bind(mergee_id)
    .bind(account_id)
    .execute(&mut *tx)
    .await
    {
        error!("Gagal re-parent conversations: {:?}", e);
        let _ = tx.rollback().await;
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Failed to merge conversations" })),
        );
    }

    // 3. Re-parent messages (polymorphic sender)
    let _ = sqlx::query(
        "UPDATE messages SET sender_id = $1 WHERE sender_type = 'Contact' AND sender_id = $2 AND account_id = $3"
    )
    .bind(base_id)
    .bind(mergee_id)
    .bind(account_id)
    .execute(&mut *tx)
    .await;

    // 4. Re-parent contact_inboxes dengan penanganan duplikat
    // Jika base contact sudah memiliki contact_inbox di inbox yang sama, hapus kepunyaan mergee
    let _ = sqlx::query(
        r#"
        DELETE FROM contact_inboxes
        WHERE contact_id = $1 AND inbox_id IN (
            SELECT inbox_id FROM contact_inboxes WHERE contact_id = $2
        )
        "#
    )
    .bind(mergee_id)
    .bind(base_id)
    .execute(&mut *tx)
    .await;

    // Sisanya re-parent ke base contact
    let _ = sqlx::query(
        "UPDATE contact_inboxes SET contact_id = $1 WHERE contact_id = $2"
    )
    .bind(base_id)
    .bind(mergee_id)
    .execute(&mut *tx)
    .await;

    // 6. Siapkan atribut gabungan (preferensi base_contact)
    let merged_name = match (&base_contact.name, &mergee_contact.name) {
        (Some(b), _) if !b.trim().is_empty() => b.clone(),
        (_, Some(m)) if !m.trim().is_empty() => m.clone(),
        _ => "Customer".to_string(),
    };

    let merged_email = match (&base_contact.email, &mergee_contact.email) {
        (Some(b), _) if !b.trim().is_empty() => Some(b.clone()),
        (_, Some(m)) if !m.trim().is_empty() => Some(m.clone()),
        _ => None,
    };

    let merged_phone = match (&base_contact.phone_number, &mergee_contact.phone_number) {
        (Some(b), _) if !b.trim().is_empty() => Some(b.clone()),
        (_, Some(m)) if !m.trim().is_empty() => Some(m.clone()),
        _ => None,
    };

    let merged_identifier = match (&base_contact.identifier, &mergee_contact.identifier) {
        (Some(b), _) if !b.trim().is_empty() => Some(b.clone()),
        (_, Some(m)) if !m.trim().is_empty() => Some(m.clone()),
        _ => None,
    };

    // Gabungkan JSON custom_attributes
    let mut custom_attrs = mergee_contact.custom_attributes.unwrap_or_else(|| json!({}));
    if let Some(base_attrs) = base_contact.custom_attributes {
        if let (Some(m_obj), Some(b_obj)) = (custom_attrs.as_object_mut(), base_attrs.as_object()) {
            for (k, v) in b_obj {
                m_obj.insert(k.clone(), v.clone());
            }
        }
    }

    // 7. Hapus mergee contact TERLEBIH DAHULU agar tidak melanggar unique index (email, account_id)
    if let Err(e) = sqlx::query(
        "DELETE FROM contacts WHERE id = $1 AND account_id = $2"
    )
    .bind(mergee_id)
    .bind(account_id)
    .execute(&mut *tx)
    .await
    {
        error!("Gagal menghapus mergee contact: {:?}", e);
        let _ = tx.rollback().await;
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Failed to remove merged contact" })),
        );
    }

    // 8. Update base contact dengan atribut yang digabungkan
    if let Err(e) = sqlx::query(
        r#"
        UPDATE contacts
        SET name = $1, email = $2, phone_number = $3, identifier = $4,
            custom_attributes = $5, updated_at = NOW()
        WHERE id = $6 AND account_id = $7
        "#
    )
    .bind(&merged_name)
    .bind(&merged_email)
    .bind(&merged_phone)
    .bind(&merged_identifier)
    .bind(&custom_attrs)
    .bind(base_id)
    .bind(account_id)
    .execute(&mut *tx)
    .await
    {
        error!("Gagal mengupdate base contact: {:?}", e);
        let _ = tx.rollback().await;
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Failed to update target contact attributes" })),
        );
    }

    // 9. Ambil data contact yang sudah diperbarui
    let updated_contact = match sqlx::query_as::<_, Contact>(
        "SELECT * FROM contacts WHERE id = $1 AND account_id = $2"
    )
    .bind(base_id)
    .bind(account_id)
    .fetch_one(&mut *tx)
    .await
    {
        Ok(c) => c,
        Err(e) => {
            error!("Gagal mengambil updated base contact: {:?}", e);
            let _ = tx.rollback().await;
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to fetch merged contact" })),
            );
        }
    };

    if let Err(e) = tx.commit().await {
        error!("Gagal commit transaksi merge contact: {:?}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Failed to commit merge transaction" })),
        );
    }

    info!(
        account_id = account_id,
        base_id = base_id,
        mergee_id = mergee_id,
        "Berhasil menggabungkan (merge) contact"
    );

    let contact_resp = ContactResponse::from(updated_contact);

    // 10. Broadcast event ke ActionCable
    let _ = state.broadcast_tx.send(CableBroadcastMsg {
        account_id,
        event: "contact.merged".to_string(),
        data: json!(contact_resp),
    });

    (StatusCode::OK, Json(json!(contact_resp)))
}
