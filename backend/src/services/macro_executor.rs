use crate::models::conversation::Conversation;
use crate::models::message::Message;
use crate::services::event_dispatcher;
use crate::services::webhook_dispatcher::dispatch_webhook_event;
use crate::state::AppState;
use serde_json::{json, Value};
use tracing::{info, warn};

pub async fn execute_macro_actions(
    state: &AppState,
    account_id: i32,
    user_id: i64,
    actions_list: &[Value],
    conversation_display_ids: &[i32],
) -> Result<(), String> {
    for &display_id in conversation_display_ids {
        let conv = match sqlx::query_as::<_, Conversation>(
            "SELECT * FROM conversations WHERE account_id = $1 AND display_id = $2",
        )
        .bind(account_id)
        .bind(display_id)
        .fetch_optional(&state.db)
        .await
        {
            Ok(Some(c)) => c,
            Ok(None) => {
                warn!("Conversation with display_id {} not found for macro execution", display_id);
                continue;
            }
            Err(e) => {
                warn!("DB error fetching conversation {}: {}", display_id, e);
                continue;
            }
        };

        for act in actions_list {
            let action_name = act.get("action_name").and_then(|a| a.as_str()).unwrap_or("");
            let params = act.get("action_params").and_then(|p| p.as_array());

            match action_name {
                "resolve_conversation" => {
                    let _ = sqlx::query(
                        "UPDATE conversations SET status = 1, status_changed_at = NOW(), updated_at = NOW() WHERE id = $1",
                    )
                    .bind(conv.id)
                    .execute(&state.db)
                    .await;
                }

                "change_status" => {
                    let status_str = params
                        .and_then(|p| p.first())
                        .and_then(|v| v.as_str())
                        .unwrap_or("open");
                    let status_int = match status_str {
                        "resolved" => 1,
                        "pending" => 2,
                        "snoozed" => 3,
                        _ => 0, // open
                    };
                    let _ = sqlx::query(
                        "UPDATE conversations SET status = $1, status_changed_at = NOW(), updated_at = NOW() WHERE id = $2",
                    )
                    .bind(status_int)
                    .bind(conv.id)
                    .execute(&state.db)
                    .await;
                }

                "snooze_conversation" | "mute_conversation" => {
                    let _ = sqlx::query(
                        r#"
                        UPDATE conversations 
                        SET status = 3, 
                            snoozed_until = NOW() + INTERVAL '24 hours',
                            status_changed_at = NOW(), 
                            updated_at = NOW() 
                        WHERE id = $1
                        "#,
                    )
                    .bind(conv.id)
                    .execute(&state.db)
                    .await;
                }

                "change_priority" => {
                    let priority_str = params
                        .and_then(|p| p.first())
                        .and_then(|v| v.as_str())
                        .unwrap_or("medium");
                    let priority_int = match priority_str {
                        "urgent" => 3,
                        "high" => 2,
                        "low" => 0,
                        _ => 1, // medium
                    };
                    let _ = sqlx::query(
                        "UPDATE conversations SET priority = $1, updated_at = NOW() WHERE id = $2",
                    )
                    .bind(priority_int)
                    .bind(conv.id)
                    .execute(&state.db)
                    .await;
                }

                "assign_agent" => {
                    if let Some(agent_id_val) = params.and_then(|p| p.first()) {
                        let agent_id = agent_id_val.as_i64();
                        let _ = sqlx::query(
                            "UPDATE conversations SET assignee_id = $1, updated_at = NOW() WHERE id = $2",
                        )
                        .bind(agent_id)
                        .bind(conv.id)
                        .execute(&state.db)
                        .await;
                    }
                }

                "assign_team" => {
                    if let Some(team_id_val) = params.and_then(|p| p.first()) {
                        let team_id = team_id_val.as_i64();
                        let _ = sqlx::query(
                            "UPDATE conversations SET team_id = $1, updated_at = NOW() WHERE id = $2",
                        )
                        .bind(team_id)
                        .bind(conv.id)
                        .execute(&state.db)
                        .await;
                    }
                }

                "add_label" => {
                    if let Some(labels_arr) = params {
                        for lbl in labels_arr {
                            if let Some(label_title) = lbl.as_str() {
                                let tag_row = sqlx::query!(
                                    r#"
                                    INSERT INTO tags (name)
                                    VALUES ($1)
                                    ON CONFLICT (name) DO UPDATE SET name = EXCLUDED.name
                                    RETURNING id
                                    "#,
                                    label_title
                                )
                                .fetch_one(&state.db)
                                .await;

                                if let Ok(tag) = tag_row {
                                    let _ = sqlx::query!(
                                        r#"
                                        INSERT INTO taggings (tag_id, taggable_type, taggable_id, context, created_at)
                                        VALUES ($1, 'Conversation', $2, 'labels', NOW())
                                        ON CONFLICT DO NOTHING
                                        "#,
                                        tag.id,
                                        conv.id as i64
                                    )
                                    .execute(&state.db)
                                    .await;
                                }
                            }
                        }
                    }
                }

                "add_private_note" => {
                    let note_text = params
                        .and_then(|p| p.first())
                        .and_then(|v| v.as_str())
                        .unwrap_or("Internal note via macro");

                    let note_res = sqlx::query_as::<_, Message>(
                        r#"
                        INSERT INTO messages (
                            account_id, inbox_id, conversation_id,
                            message_type, private, content, sender_type, sender_id,
                            created_at, updated_at
                        )
                        VALUES ($1, $2, $3, 2, true, $4, 'User', $5, NOW(), NOW())
                        RETURNING *
                        "#,
                    )
                    .bind(account_id)
                    .bind(conv.inbox_id)
                    .bind(conv.id)
                    .bind(note_text)
                    .bind(user_id)
                    .fetch_optional(&state.db)
                    .await;

                    if let Ok(Some(msg)) = note_res {
                        event_dispatcher::dispatch_event(
                            state,
                            account_id,
                            "message.created",
                            json!({ "message": msg }),
                        );
                    }
                }

                "send_message" => {
                    let reply_text = params
                        .and_then(|p| p.first())
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    if !reply_text.is_empty() {
                        let msg_res = sqlx::query_as::<_, Message>(
                            r#"
                            INSERT INTO messages (
                                account_id, inbox_id, conversation_id,
                                message_type, private, content, sender_type, sender_id,
                                created_at, updated_at
                            )
                            VALUES ($1, $2, $3, 1, false, $4, 'User', $5, NOW(), NOW())
                            RETURNING *
                            "#,
                        )
                        .bind(account_id)
                        .bind(conv.inbox_id)
                        .bind(conv.id)
                        .bind(reply_text)
                        .bind(user_id)
                        .fetch_optional(&state.db)
                        .await;

                        if let Ok(Some(msg)) = msg_res {
                            event_dispatcher::dispatch_event(
                                state,
                                account_id,
                                "message.created",
                                json!({ "message": msg }),
                            );
                        }
                    }
                }

                "send_webhook_event" => {
                    let event_payload = json!({
                        "event": "macro_triggered",
                        "conversation_id": conv.id,
                        "display_id": conv.display_id,
                        "account_id": account_id,
                        "user_id": user_id
                    });
                    dispatch_webhook_event(&state.db, account_id as i64, "macro_triggered", event_payload).await;
                }

                _ => {
                    warn!("Unknown macro action: {}", action_name);
                }
            }
        }

        // Ambil percakapan terbaru untuk broadcast
        if let Ok(Some(updated_conv)) = sqlx::query_as::<_, Conversation>(
            "SELECT * FROM conversations WHERE id = $1",
        )
        .bind(conv.id)
        .fetch_optional(&state.db)
        .await
        {
            event_dispatcher::dispatch_event(
                state,
                account_id,
                "conversation.updated",
                json!({ "conversation": updated_conv }),
            );

            // Dispatch webhook outbound
            let payload = json!({
                "event": "conversation_updated",
                "id": updated_conv.id,
                "display_id": updated_conv.display_id,
                "status": updated_conv.status,
                "account_id": account_id
            });
            dispatch_webhook_event(&state.db, account_id as i64, "conversation_updated", payload).await;
        }

        info!("Macro executed successfully for conversation {}", display_id);
    }

    Ok(())
}
