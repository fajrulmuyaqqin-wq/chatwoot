use serde_json::json;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::debug;

use crate::state::{AccountId, AppState, CableBroadcastMsg, UserId};

pub const PRESENCE_TIMEOUT: Duration = Duration::from_secs(35);

/// Mencatat aktivitas terbaru agen di in-memory DashMap
pub fn record_presence(state: &AppState, account_id: AccountId, user_id: UserId) {
    let account_map = state.presence.entry(account_id).or_default();
    account_map.insert(user_id, Instant::now());
}

/// Menghasilkan map online users {"<user_id>": "online"}
pub fn get_online_users(state: &AppState, account_id: AccountId) -> HashMap<String, String> {
    let mut online_users = HashMap::new();
    if let Some(account_map) = state.presence.get(&account_id) {
        let now = Instant::now();
        for item in account_map.iter() {
            let (user_id, last_seen) = (*item.key(), *item.value());
            if now.duration_since(last_seen) <= PRESENCE_TIMEOUT {
                online_users.insert(user_id.to_string(), "online".to_string());
            }
        }
    }
    online_users
}

/// Membangun payload event presence.update
pub fn build_presence_event(
    account_id: AccountId,
    online_users: HashMap<String, String>,
) -> serde_json::Value {
    json!({
        "account_id": account_id,
        "users": online_users,
        "contacts": {}
    })
}

/// Background worker pembersih session presence yang idle dan broadcast update
pub fn spawn_presence_reaper(state: AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(15));
        loop {
            interval.tick().await;

            let now = Instant::now();
            let mut accounts_to_notify = Vec::new();

            for account_entry in state.presence.iter() {
                let account_id = *account_entry.key();
                let user_map = account_entry.value();
                let mut expired_users = Vec::new();

                for item in user_map.iter() {
                    let (user_id, last_seen) = (*item.key(), *item.value());
                    if now.duration_since(last_seen) > PRESENCE_TIMEOUT {
                        expired_users.push(user_id);
                    }
                }

                if !expired_users.is_empty() {
                    for user_id in expired_users {
                        user_map.remove(&user_id);
                    }
                    accounts_to_notify.push(account_id);
                }
            }

            for account_id in accounts_to_notify {
                let online_users = get_online_users(&state, account_id);
                debug!(
                    "Presence updated untuk account {}: {} online",
                    account_id,
                    online_users.len()
                );
                let _ = state.broadcast_tx.send(CableBroadcastMsg {
                    account_id,
                    event: "presence.update".to_string(),
                    data: build_presence_event(account_id, online_users),
                });
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    #[tokio::test]
    async fn test_presence_lifecycle() {
        let pool = PgPoolOptions::new().connect_lazy("postgres://localhost/test").unwrap();
        let config = crate::config::Config::from_env().unwrap();
        let state = AppState::new(pool, config);

        record_presence(&state, 1, 100);
        let online = get_online_users(&state, 1);
        assert_eq!(online.get("100"), Some(&"online".to_string()));

        let event = build_presence_event(1, online);
        assert_eq!(event["account_id"], 1);
        assert_eq!(event["users"]["100"], "online");
    }
}
