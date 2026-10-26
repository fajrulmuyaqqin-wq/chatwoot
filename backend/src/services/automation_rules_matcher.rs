use crate::models::automation_rule::AutomationRule;
use crate::models::conversation::Conversation;
use crate::models::message::Message;
use crate::services::event_dispatcher;
use crate::services::notification_dispatcher::create_and_dispatch_notification;
use crate::state::AppState;
use serde_json::Value;
use tracing::{info, warn};

pub async fn evaluate_and_execute_rules(
    state: &AppState,
    account_id: i64,
    event_name: &str,
    conversation_id: i32,
    message_id: Option<i32>,
) {
    // 1. Ambil semua rule aktif untuk event_name ini
    let rules = match sqlx::query_as::<_, AutomationRule>(
        "SELECT * FROM automation_rules WHERE account_id = $1 AND event_name = $2 AND active = TRUE ORDER BY id ASC",
    )
    .bind(account_id)
    .bind(event_name)
    .fetch_all(&state.db)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to fetch automation rules for account {}: {}", account_id, e);
            return;
        }
    };

    if rules.is_empty() {
        return;
    }

    // 2. Ambil data conversation
    let conv = match sqlx::query_as::<_, Conversation>(
        "SELECT * FROM conversations WHERE id = $1 AND account_id = $2",
    )
    .bind(conversation_id)
    .bind(account_id as i32)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(c)) => c,
        _ => return,
    };

    // 3. Ambil data message jika ada
    let msg = if let Some(mid) = message_id {
        sqlx::query_as::<_, Message>("SELECT * FROM messages WHERE id = $1")
            .bind(mid)
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None)
    } else {
        None
    };

    for rule in rules {
        if matches_conditions(&rule.conditions, &conv, msg.as_ref()) {
            info!(
                "Automation Rule #{} '{}' matched for conversation #{} (event: {})",
                rule.id, rule.name, conv.id, event_name
            );
            execute_rule_actions(state, account_id, &rule, &conv).await;
        }
    }
}

fn matches_conditions(
    conditions: &Value,
    conv: &Conversation,
    msg: Option<&Message>,
) -> bool {
    let cond_array = match conditions.as_array() {
        Some(arr) if !arr.is_empty() => arr,
        _ => return true, // jika tidak ada kondisi, rule dianggap match
    };

    let mut overall_match = true;
    let mut is_first = true;

    for cond in cond_array {
        let attr_key = cond.get("attribute_key").and_then(|v| v.as_str()).unwrap_or("");
        let filter_op = cond.get("filter_operator").and_then(|v| v.as_str()).unwrap_or("equal_to");
        let query_op = cond.get("query_operator").and_then(|v| v.as_str()).unwrap_or("and");
        let values = cond.get("values").and_then(|v| v.as_array());

        let matched = evaluate_single_condition(attr_key, filter_op, values, conv, msg);

        if is_first {
            overall_match = matched;
            is_first = false;
        } else if query_op.eq_ignore_ascii_case("or") {
            overall_match = overall_match || matched;
        } else {
            overall_match = overall_match && matched;
        }
    }

    overall_match
}

fn evaluate_single_condition(
    attr_key: &str,
    filter_op: &str,
    values: Option<&Vec<Value>>,
    conv: &Conversation,
    msg: Option<&Message>,
) -> bool {
    let empty_vec = Vec::new();
    let vals = values.unwrap_or(&empty_vec);

    let val_strs: Vec<String> = vals
        .iter()
        .filter_map(|v| {
            if v.is_string() {
                v.as_str().map(|s| s.to_string())
            } else if v.is_number() {
                Some(v.to_string())
            } else {
                None
            }
        })
        .collect();

    match attr_key {
        "inbox_id" => {
            let actual = conv.inbox_id.to_string();
            match_string_filter(&actual, filter_op, &val_strs)
        }
        "status" => {
            let actual = match conv.status {
                1 => "resolved",
                2 => "pending",
                3 => "snoozed",
                _ => "open",
            };
            match_string_filter(actual, filter_op, &val_strs)
        }
        "priority" => {
            let actual = match conv.priority {
                Some(0) => "low",
                Some(1) => "medium",
                Some(2) => "high",
                Some(3) => "urgent",
                _ => "nil",
            };
            match_string_filter(actual, filter_op, &val_strs)
        }
        "content" | "message_content" => {
            let actual = msg.and_then(|m| m.content.as_deref()).unwrap_or("");
            match_string_filter(actual, filter_op, &val_strs)
        }
        "message_type" => {
            let actual = msg.map(|m| {
                match m.message_type {
                    0 => "incoming",
                    1 => "outgoing",
                    2 => "activity",
                    3 => "template",
                    _ => "incoming",
                }
            }).unwrap_or("");
            match_string_filter(actual, filter_op, &val_strs)
        }
        "team_id" => {
            let actual = conv.team_id.map(|t| t.to_string()).unwrap_or_else(|| "nil".to_string());
            match_string_filter(&actual, filter_op, &val_strs)
        }
        "assignee_id" => {
            let actual = conv.assignee_id.map(|a| a.to_string()).unwrap_or_else(|| "nil".to_string());
            match_string_filter(&actual, filter_op, &val_strs)
        }
        _ => true,
    }
}

fn match_string_filter(actual: &str, filter_op: &str, values: &[String]) -> bool {
    let actual_lower = actual.to_lowercase();
    match filter_op {
        "equal_to" => values.iter().any(|v| actual_lower == v.to_lowercase()),
        "not_equal_to" => values.iter().all(|v| actual_lower != v.to_lowercase()),
        "contains" => values.iter().any(|v| actual_lower.contains(&v.to_lowercase())),
        "does_not_contain" => values.iter().all(|v| !actual_lower.contains(&v.to_lowercase())),
        "starts_with" => values.iter().any(|v| actual_lower.starts_with(&v.to_lowercase())),
        "is_present" => !actual.is_empty() && actual != "nil",
        "is_not_present" => actual.is_empty() || actual == "nil",
        _ => true,
    }
}

async fn execute_rule_actions(
    state: &AppState,
    account_id: i64,
    rule: &AutomationRule,
    conv: &Conversation,
) {
    let actions_list = match rule.actions.as_array() {
        Some(a) => a,
        None => return,
    };

    for act in actions_list {
        let action_name = act.get("action_name").and_then(|a| a.as_str()).unwrap_or("");
        let params = act.get("action_params").and_then(|p| p.as_array());

        match action_name {
            "assign_team" => {
                let team_id: Option<i32> = params
                    .and_then(|p| p.first())
                    .and_then(|v| v.as_i64().map(|i| i as i32));
                let _ = sqlx::query("UPDATE conversations SET team_id = $1, updated_at = NOW() WHERE id = $2")
                    .bind(team_id)
                    .bind(conv.id)
                    .execute(&state.db)
                    .await;
            }
            "assign_agent" => {
                let agent_id: Option<i32> = params
                    .and_then(|p| p.first())
                    .and_then(|v| v.as_i64().map(|i| i as i32));

                let _ = sqlx::query("UPDATE conversations SET assignee_id = $1, updated_at = NOW() WHERE id = $2")
                    .bind(agent_id)
                    .bind(conv.id)
                    .execute(&state.db)
                    .await;

                // Kirim notifikasi ke agen jika di-assign
                if let Some(uid) = agent_id {
                    let _ = create_and_dispatch_notification(
                        state,
                        account_id,
                        uid as i64,
                        2, // conversation_assignment
                        conv.id as i64,
                        "Conversation",
                        None,
                        None,
                        None,
                    )
                    .await;
                }
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
                    _ => 0,
                };
                let _ = sqlx::query("UPDATE conversations SET status = $1, updated_at = NOW() WHERE id = $2")
                    .bind(status_int)
                    .bind(conv.id)
                    .execute(&state.db)
                    .await;
            }
            "change_priority" => {
                let priority_str = params
                    .and_then(|p| p.first())
                    .and_then(|v| v.as_str());
                let priority_val: Option<i32> = match priority_str {
                    Some("urgent") => Some(3),
                    Some("high") => Some(2),
                    Some("medium") => Some(1),
                    Some("low") => Some(0),
                    _ => None,
                };
                let _ = sqlx::query("UPDATE conversations SET priority = $1, updated_at = NOW() WHERE id = $2")
                    .bind(priority_val)
                    .bind(conv.id)
                    .execute(&state.db)
                    .await;
            }
            "send_message" => {
                let msg_text = params
                    .and_then(|p| p.first())
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if !msg_text.is_empty() {
                    let new_msg = sqlx::query_as::<_, Message>(
                        r#"
                        INSERT INTO messages (
                            account_id, inbox_id, conversation_id, message_type, content,
                            sender_type, sender_id, private, created_at, updated_at
                        )
                        VALUES ($1, $2, $3, 1, $4, 'User', NULL, FALSE, NOW(), NOW())
                        RETURNING *
                        "#,
                    )
                    .bind(account_id as i32)
                    .bind(conv.inbox_id)
                    .bind(conv.id)
                    .bind(msg_text)
                    .fetch_optional(&state.db)
                    .await
                    .unwrap_or(None);

                    if let Some(m) = new_msg {
                        event_dispatcher::dispatch_event(
                            state,
                            account_id as i32,
                            "message.created",
                            serde_json::to_value(&m).unwrap_or(Value::Null),
                        );
                    }
                }
            }
            "add_private_note" => {
                let note_text = params
                    .and_then(|p| p.first())
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if !note_text.is_empty() {
                    let new_msg = sqlx::query_as::<_, Message>(
                        r#"
                        INSERT INTO messages (
                            account_id, inbox_id, conversation_id, message_type, content,
                            sender_type, sender_id, private, created_at, updated_at
                        )
                        VALUES ($1, $2, $3, 1, $4, 'User', NULL, TRUE, NOW(), NOW())
                        RETURNING *
                        "#,
                    )
                    .bind(account_id as i32)
                    .bind(conv.inbox_id)
                    .bind(conv.id)
                    .bind(note_text)
                    .fetch_optional(&state.db)
                    .await
                    .unwrap_or(None);

                    if let Some(m) = new_msg {
                        event_dispatcher::dispatch_event(
                            state,
                            account_id as i32,
                            "message.created",
                            serde_json::to_value(&m).unwrap_or(Value::Null),
                        );
                    }
                }
            }
            _ => {}
        }
    }

    // Broadcast update conversation setelah action selesai
    if let Ok(Some(updated_conv)) = sqlx::query_as::<_, Conversation>(
        "SELECT * FROM conversations WHERE id = $1",
    )
    .bind(conv.id)
    .fetch_optional(&state.db)
    .await
    {
        event_dispatcher::dispatch_event(
            state,
            account_id as i32,
            "conversation.updated",
            serde_json::to_value(&updated_conv).unwrap_or(Value::Null),
        );
    }
}
