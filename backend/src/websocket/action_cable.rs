use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;
use tracing::{debug, info, warn};

use crate::state::{AccountId, AppState, CableBroadcastMsg, UserId};
use crate::websocket::presence;

#[derive(Debug, Deserialize)]
pub struct CableClientMessage {
    pub command: String,                  // "subscribe", "message", "unsubscribe"
    pub identifier: String,               // JSON string
    pub data: Option<String>,             // JSON string
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct RoomChannelIdentifier {
    pub channel: String,
    pub pubsub_token: Option<String>,
    pub account_id: Option<AccountId>,
    pub user_id: Option<UserId>,
}

#[derive(Debug, Deserialize)]
pub struct MessageData {
    pub action: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Serialize)]
pub struct CableWelcome {
    pub r#type: String, // "welcome"
}

#[allow(dead_code)]
#[derive(Debug, Serialize)]
pub struct CablePing {
    pub r#type: String, // "ping"
    pub message: i64,
}

#[allow(dead_code)]
#[derive(Debug, Serialize)]
pub struct CableConfirm {
    pub r#type: String, // "confirm_subscription"
    pub identifier: String,
}

#[allow(dead_code)]
#[derive(Debug, Serialize)]
pub struct CableBroadcastEnvelope<'a> {
    pub identifier: &'a str,
    pub message: CableEventPayload<'a>,
}

#[allow(dead_code)]
#[derive(Debug, Serialize)]
pub struct CableEventPayload<'a> {
    pub event: &'a str,
    pub data: &'a serde_json::Value,
}

pub async fn cable_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    // ActionCable client mengirim Sec-WebSocket-Protocol: actioncable-v1-json
    // Server harus membalasnya agar handshake berhasil
    ws.protocols(["actioncable-v1-json"])
        .on_upgrade(move |socket| handle_cable_socket(socket, state))
}

async fn handle_cable_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    // 1. Kirim pesan welcome sesuai protokol ActionCable
    let welcome = json!({ "type": "welcome" });
    if let Err(e) = sender.send(Message::Text(welcome.to_string())).await {
        warn!("Gagal mengirim pesan welcome ActionCable: {}", e);
        return;
    }

    let mut subscribed_identifier: Option<String> = None;
    let mut current_account_id: Option<AccountId> = None;
    let mut current_user_id: Option<UserId> = None;

    let mut broadcast_rx = state.broadcast_tx.subscribe();
    let mut ping_interval = tokio::time::interval(Duration::from_secs(3));

    loop {
        tokio::select! {
            // Heartbeat Ping setiap 3 detik
            _ = ping_interval.tick() => {
                let ping = json!({
                    "type": "ping",
                    "message": Utc::now().timestamp()
                });
                if sender.send(Message::Text(ping.to_string())).await.is_err() {
                    break;
                }
            }

            // Pesan Broadcast dari Backend (message.created, presence.update, dll)
            broadcast_result = broadcast_rx.recv() => {
                match broadcast_result {
                    Ok(msg) => {
                        if let (Some(sub_ident), Some(acc_id)) = (&subscribed_identifier, current_account_id) {
                            if acc_id == msg.account_id {
                                let payload = json!({
                                    "identifier": sub_ident,
                                    "message": {
                                        "event": msg.event,
                                        "data": msg.data
                                    }
                                });
                                if sender.send(Message::Text(payload.to_string())).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!("ActionCable broadcast receiver tertinggal {} pesan", skipped);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }

            // Pesan masuk dari Client WebSocket
            msg_opt = receiver.next() => {
                let msg = match msg_opt {
                    Some(Ok(m)) => m,
                    _ => break, // Client disconnect atau error
                };

                match msg {
                    Message::Text(text) => {
                        match serde_json::from_str::<CableClientMessage>(&text) {
                            Ok(client_cmd) => {
                                match client_cmd.command.as_str() {
                                    "subscribe" => {
                                        let parsed_ident = serde_json::from_str::<RoomChannelIdentifier>(&client_cmd.identifier);
                                        match parsed_ident {
                                            Ok(room_params) if room_params.channel == "RoomChannel" => {
                                                // Validasi autentikasi pubsub_token & izin akun
                                                let is_authorized = match (&room_params.pubsub_token, room_params.account_id, room_params.user_id) {
                                                    (Some(tok), Some(acc_id), Some(usr_id)) if !tok.trim().is_empty() => {
                                                        let user_row = sqlx::query!(
                                                            r#"
                                                            SELECT u.id, u.type, au.role
                                                            FROM users u
                                                            LEFT JOIN account_users au ON au.user_id = u.id AND au.account_id = $2
                                                            WHERE u.id = $1 AND u.pubsub_token = $3
                                                            LIMIT 1
                                                            "#,
                                                            usr_id as i64,
                                                            acc_id as i64,
                                                            tok
                                                        )
                                                        .fetch_optional(&state.db)
                                                        .await;

                                                        match user_row {
                                                            Ok(Some(row)) => {
                                                                row.r#type.as_deref() == Some("SuperAdmin") || row.role.is_some()
                                                            }
                                                            _ => false,
                                                        }
                                                    }
                                                    _ => false,
                                                };

                                                if !is_authorized {
                                                    warn!(
                                                        "Akses WebSocket ditolak: pubsub_token tidak valid atau bukan anggota akun (account={:?}, user={:?})",
                                                        room_params.account_id, room_params.user_id
                                                    );
                                                    let reject = json!({
                                                        "type": "reject_subscription",
                                                        "identifier": client_cmd.identifier
                                                    });
                                                    let _ = sender.send(Message::Text(reject.to_string())).await;
                                                    continue;
                                                }

                                                info!(
                                                    "Client terautentikasi dan berlangganan RoomChannel: account={:?}, user={:?}",
                                                    room_params.account_id, room_params.user_id
                                                );

                                                current_account_id = room_params.account_id;
                                                current_user_id = room_params.user_id;
                                                subscribed_identifier = Some(client_cmd.identifier.clone());

                                                // Konfirmasi langganan
                                                let confirm = json!({
                                                    "type": "confirm_subscription",
                                                    "identifier": client_cmd.identifier
                                                });
                                                if sender.send(Message::Text(confirm.to_string())).await.is_err() {
                                                    break;
                                                }

                                                // Catat presence dan kirim status awal
                                                if let (Some(acc_id), Some(usr_id)) = (current_account_id, current_user_id) {
                                                    presence::record_presence(&state, acc_id, usr_id);
                                                    let online_map = presence::get_online_users(&state, acc_id);
                                                    let initial_presence = json!({
                                                        "identifier": client_cmd.identifier,
                                                        "message": {
                                                            "event": "presence.update",
                                                            "data": presence::build_presence_event(acc_id, online_map)
                                                        }
                                                    });
                                                    let _ = sender.send(Message::Text(initial_presence.to_string())).await;
                                                }
                                            }
                                            _ => {
                                                warn!("Langganan channel ditolak atau tidak didukung: {}", client_cmd.identifier);
                                            }
                                        }
                                    }
                                    "message" => {
                                        if let Some(data_str) = &client_cmd.data {
                                            if let Ok(action_data) = serde_json::from_str::<MessageData>(data_str) {
                                                if action_data.action.as_deref() == Some("update_presence") {
                                                    if let (Some(acc_id), Some(usr_id)) = (current_account_id, current_user_id) {
                                                        presence::record_presence(&state, acc_id, usr_id);
                                                        let online_map = presence::get_online_users(&state, acc_id);

                                                        // Broadcast status presence terkini ke semua agen akun ini
                                                        let _ = state.broadcast_tx.send(CableBroadcastMsg {
                                                            account_id: acc_id,
                                                            event: "presence.update".to_string(),
                                                            data: presence::build_presence_event(acc_id, online_map),
                                                        });
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    "unsubscribe" => {
                                        debug!("Client unsubscribe dari channel");
                                        subscribed_identifier = None;
                                    }
                                    _ => {
                                        debug!("Mengabaikan command tidak dikenal: {}", client_cmd.command);
                                    }
                                }
                            }
                            Err(e) => {
                                debug!("Payload WebSocket bukan ActionCable JSON valid: {}", e);
                            }
                        }
                    }
                    Message::Close(_) => {
                        debug!("Koneksi ActionCable ditutup oleh client.");
                        break;
                    }
                    _ => {}
                }
            }
        }
    }
}
