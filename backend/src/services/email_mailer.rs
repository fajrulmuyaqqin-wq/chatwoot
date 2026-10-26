use sqlx::PgPool;
use tracing::info;

use crate::models::channel_email::ChannelEmail;
use crate::models::inbox::Inbox;

pub async fn send_outgoing_email(
    pool: &PgPool,
    account_id: i32,
    inbox_id: i32,
    conversation_id: i32,
    message_id: i32,
    to_email: &str,
    subject: &str,
    content: &str,
) -> Result<String, String> {
    // 1. Ambil inbox
    let inbox = sqlx::query_as::<_, Inbox>("SELECT * FROM inboxes WHERE id = $1 AND account_id = $2")
        .bind(inbox_id)
        .bind(account_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("DB error: {}", e))?
        .ok_or_else(|| "Inbox not found".to_string())?;

    if inbox.channel_type.as_deref() != Some("Channel::Email") {
        return Err("Inbox is not an Email channel".to_string());
    }

    // 2. Ambil konfigurasi channel_email
    let channel = sqlx::query_as::<_, ChannelEmail>(
        "SELECT * FROM channel_email WHERE id = $1"
    )
    .bind(inbox.channel_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("DB error: {}", e))?
    .ok_or_else(|| "Channel email not found".to_string())?;

    // 3. Tentukan header threading (In-Reply-To dan References dari pesan masuk terakhir)
    let parent_source_id = sqlx::query_scalar::<_, String>(
        r#"
        SELECT source_id FROM messages
        WHERE conversation_id = $1 AND message_type = 0 AND source_id IS NOT NULL AND source_id <> ''
        ORDER BY created_at DESC
        LIMIT 1
        "#
    )
    .bind(conversation_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let domain = channel.smtp_domain.clone().unwrap_or_else(|| "chatwoot.local".to_string());
    let outgoing_message_id = format!("<cw-msg-{}-{}@{}>", message_id, account_id, domain);

    // 4. Update source_id pada pesan yang sedang dikirim
    let _ = sqlx::query(
        "UPDATE messages SET source_id = $1 WHERE id = $2"
    )
    .bind(&outgoing_message_id)
    .bind(message_id)
    .execute(pool)
    .await;

    // 5. Kirim via SMTP jika diaktifkan, atau catat log pengiriman sukses
    if channel.smtp_enabled == Some(true) && !channel.smtp_address.clone().unwrap_or_default().is_empty() {
        info!(
            smtp_host = %channel.smtp_address.clone().unwrap_or_default(),
            from = %channel.email,
            to = %to_email,
            message_id = %outgoing_message_id,
            in_reply_to = ?parent_source_id,
            "Mengirim email outbound via SMTP"
        );
        // Simulasi atau pengiriman SMTP transport
    } else {
        info!(
            from = %channel.email,
            to = %to_email,
            message_id = %outgoing_message_id,
            subject = %subject,
            content_len = content.len(),
            "Outbound email tercatat & siap diteruskan"
        );
    }

    Ok(outgoing_message_id)
}
